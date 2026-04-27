use std::time::Duration;

use chrono::{NaiveDate, Utc};
use serde::Deserialize;
use tc_core::status::StatusId;
use tc_core::task::{Priority, Task};

use crate::cli::{GithubImportArgs, ImportArgs, ImportFormat, ImportSource, LinearImportArgs};
use crate::error::CliError;
use crate::output;

pub async fn run(args: ImportArgs) -> Result<(), CliError> {
    if args.format.is_some() && args.source.is_some() {
        return Err(CliError::user(
            "tc import: --format is mutually exclusive with the github/linear subcommands",
        ));
    }
    if let Some(format) = args.format {
        return run_file(format, args).await;
    }
    match args.source {
        Some(ImportSource::Github(gh)) => import_github(gh, args.dry_run).await,
        Some(ImportSource::Linear(lin)) => import_linear(lin, args.dry_run).await,
        None => Err(CliError::user(
            "tc import: pass `--format <json|kairo-md|linear-csv> --file <path> --epic <name>` \
             or use a subcommand: `tc import github`, `tc import linear`",
        )),
    }
}

// ── File-based import (M-6.7) ──────────────────────────────────────────

async fn run_file(format: ImportFormat, args: ImportArgs) -> Result<(), CliError> {
    let path = args
        .file
        .ok_or_else(|| CliError::user("--file <path> is required with --format"))?;
    let epic = args
        .epic
        .ok_or_else(|| CliError::user("--epic <name> is required with --format"))?;

    let raw = std::fs::read_to_string(&path)
        .map_err(|e| CliError::user(format!("read {}: {e}", path.display())))?;

    let records = match format {
        ImportFormat::Json => parse_json(&raw)?,
        ImportFormat::KairoMd => parse_kairo_md(&raw),
        ImportFormat::LinearCsv => parse_linear_csv(&raw)?,
    };

    if records.is_empty() {
        output::print_warning(&format!("No tasks found in {}", path.display()));
        return Ok(());
    }

    apply_imports(records, &epic, args.dry_run, &path.display().to_string())
}

// JSON: array of records. Title is the only required field.
#[derive(Debug, Deserialize)]
struct JsonRecord {
    title: String,
    #[serde(default)]
    epic: Option<String>,
    #[serde(default)]
    priority: Option<Priority>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    due: Option<NaiveDate>,
    #[serde(default)]
    scheduled: Option<NaiveDate>,
    #[serde(default, with = "humantime_serde")]
    estimate: Option<Duration>,
    #[serde(default)]
    notes: String,
    #[serde(default)]
    acceptance_criteria: Vec<String>,
    /// Optional stable identifier for dedup. If set, we record it as
    /// "Imported from: <source_ref>" in notes -- same convention used by
    /// the GitHub/Linear importers so re-runs skip what's already there.
    #[serde(default)]
    source_ref: Option<String>,
}

fn parse_json(raw: &str) -> Result<Vec<ImportRecord>, CliError> {
    let records: Vec<JsonRecord> =
        serde_json::from_str(raw).map_err(|e| CliError::user(format!("invalid JSON: {e}")))?;

    Ok(records
        .into_iter()
        .map(|r| ImportRecord {
            source_ref: r.source_ref,
            title: r.title,
            epic_override: r.epic,
            priority: r.priority.unwrap_or_default(),
            tags: r.tags,
            due: r.due,
            scheduled: r.scheduled,
            estimate: r.estimate,
            notes: r.notes,
            acceptance_criteria: r.acceptance_criteria,
        })
        .collect())
}

// Kairo-style markdown:
//   - [ ] Title #tag1 #tag2 !p1 due:2026-05-01
// Indented bullets become acceptance criteria for the parent task. Anything
// else is ignored, so users can drop a list inside a larger note file.
fn parse_kairo_md(raw: &str) -> Vec<ImportRecord> {
    let mut out: Vec<ImportRecord> = Vec::new();
    'lines: for line in raw.lines() {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();

        let Some(rest) = trimmed
            .strip_prefix("- [ ] ")
            .or_else(|| trimmed.strip_prefix("- [x] "))
            .or_else(|| trimmed.strip_prefix("- [X] "))
        else {
            continue 'lines;
        };

        if indent > 0
            && let Some(parent) = out.last_mut()
        {
            parent.acceptance_criteria.push(rest.trim().to_string());
            continue 'lines;
        }

        out.push(parse_kairo_line(rest));
    }
    out
}

fn parse_kairo_line(rest: &str) -> ImportRecord {
    let mut tags = Vec::new();
    let mut priority = Priority::default();
    let mut due: Option<NaiveDate> = None;
    let mut scheduled: Option<NaiveDate> = None;
    let mut title_parts: Vec<&str> = Vec::new();

    'tokens: for tok in rest.split_whitespace() {
        if let Some(tag) = tok.strip_prefix('#') {
            tags.push(tag.to_string());
            continue 'tokens;
        }
        if let Some(prio_str) = tok.strip_prefix('!') {
            if let Some(p) = parse_priority_token(prio_str) {
                priority = p;
                continue 'tokens;
            }
        }
        if let Some(d) = tok.strip_prefix("due:") {
            if let Ok(date) = NaiveDate::parse_from_str(d, "%Y-%m-%d") {
                due = Some(date);
                continue 'tokens;
            }
        }
        if let Some(d) = tok.strip_prefix("scheduled:") {
            if let Ok(date) = NaiveDate::parse_from_str(d, "%Y-%m-%d") {
                scheduled = Some(date);
                continue 'tokens;
            }
        }
        title_parts.push(tok);
    }

    ImportRecord {
        source_ref: None,
        title: title_parts.join(" "),
        epic_override: None,
        priority,
        tags,
        due,
        scheduled,
        estimate: None,
        notes: String::new(),
        acceptance_criteria: Vec::new(),
    }
}

fn parse_priority_token(s: &str) -> Option<Priority> {
    match s.to_lowercase().as_str() {
        "p1" | "1" => Some(Priority::P1),
        "p2" | "2" => Some(Priority::P2),
        "p3" | "3" => Some(Priority::P3),
        "p4" | "4" => Some(Priority::P4),
        "p5" | "5" => Some(Priority::P5),
        _ => None,
    }
}

// Linear CSV export columns (case-insensitive lookup; we accept the standard
// "ID, Title, Description, Priority, Status, Labels" header set Linear emits).
fn parse_linear_csv(raw: &str) -> Result<Vec<ImportRecord>, CliError> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_reader(raw.as_bytes());

    let header = reader
        .headers()
        .map_err(|e| CliError::user(format!("invalid CSV header: {e}")))?
        .clone();

    let idx = |name: &str| header.iter().position(|h| h.eq_ignore_ascii_case(name));

    let i_id = idx("ID").or_else(|| idx("Identifier"));
    let i_title = idx("Title")
        .or_else(|| idx("Name"))
        .ok_or_else(|| CliError::user("CSV missing required Title column"))?;
    let i_desc = idx("Description").or_else(|| idx("Body"));
    let i_priority = idx("Priority");
    let i_labels = idx("Labels").or_else(|| idx("Tags"));
    let i_due = idx("Due Date").or_else(|| idx("Due"));

    let mut out = Vec::new();
    for record in reader.records() {
        let row = record.map_err(|e| CliError::user(format!("CSV row error: {e}")))?;

        let title = row.get(i_title).unwrap_or("").trim();
        if title.is_empty() {
            continue;
        }

        let priority = i_priority
            .and_then(|i| row.get(i))
            .and_then(linear_priority_from_str)
            .unwrap_or_default();

        let tags = i_labels
            .and_then(|i| row.get(i))
            .map(|s| {
                s.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();

        let due = i_due.and_then(|i| row.get(i)).and_then(|s| {
            let s = s.trim();
            if s.is_empty() {
                None
            } else {
                NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
            }
        });

        out.push(ImportRecord {
            source_ref: i_id.and_then(|i| row.get(i)).map(str::to_string),
            title: title.to_string(),
            epic_override: None,
            priority,
            tags,
            due,
            scheduled: None,
            estimate: None,
            notes: i_desc
                .and_then(|i| row.get(i))
                .map(str::to_string)
                .unwrap_or_default(),
            acceptance_criteria: Vec::new(),
        });
    }

    Ok(out)
}

fn linear_priority_from_str(s: &str) -> Option<Priority> {
    // Linear exports priority as either "Urgent/High/Medium/Low/No priority"
    // or as numeric 1..4 (1=urgent). Map to our P1..P5.
    match s.trim().to_lowercase().as_str() {
        "urgent" | "1" => Some(Priority::P1),
        "high" | "2" => Some(Priority::P2),
        "medium" | "normal" | "3" => Some(Priority::P3),
        "low" | "4" => Some(Priority::P4),
        "no priority" | "" | "0" => Some(Priority::P5),
        _ => None,
    }
}

// ── GitHub Issues ───────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct GithubIssue {
    number: u64,
    title: String,
    body: Option<String>,
    labels: Vec<GithubLabel>,
}

#[derive(Debug, Deserialize)]
struct GithubLabel {
    name: String,
}

/// Parse the GitHub pagination `Link` header and return the URL for `rel="next"`.
fn parse_github_next_link(header: Option<&reqwest::header::HeaderValue>) -> Option<String> {
    let raw = header?.to_str().ok()?;
    'parts: for part in raw.split(',') {
        let part = part.trim();
        let Some(end) = part.find('>') else {
            continue 'parts;
        };
        let Some(start) = part.find('<') else {
            continue 'parts;
        };
        if end <= start {
            continue 'parts;
        }
        let url = &part[start + 1..end];
        let rest = &part[end + 1..];
        if rest.contains("rel=\"next\"") {
            return Some(url.to_string());
        }
    }
    None
}

fn github_token() -> Result<String, CliError> {
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        return Ok(token);
    }
    let output = std::process::Command::new("gh")
        .args(["auth", "token"])
        .output()
        .map_err(|_| {
            CliError::user(
                "GITHUB_TOKEN not set and `gh` CLI not found. \
                 Set GITHUB_TOKEN or install gh: https://cli.github.com",
            )
        })?;

    if !output.status.success() {
        return Err(CliError::user(
            "GITHUB_TOKEN not set and `gh auth token` failed. \
             Run `gh auth login` or export GITHUB_TOKEN.",
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

async fn import_github(args: GithubImportArgs, dry_run_flag: bool) -> Result<(), CliError> {
    let token = github_token()?;
    let client = reqwest::Client::new();

    let mut url = format!(
        "https://api.github.com/repos/{}/issues?state={}&per_page=100",
        args.repo, args.state,
    );

    if let Some(ref label) = args.label {
        url.push_str(&format!("&labels={label}"));
    }
    if let Some(ref milestone) = args.milestone {
        url.push_str(&format!("&milestone={milestone}"));
    }

    let mut issues: Vec<GithubIssue> = Vec::new();
    let mut next_url = Some(url);
    const MAX_PAGES: usize = 100;
    let mut page = 0;

    while let Some(page_url) = next_url.take() {
        page += 1;
        if page > MAX_PAGES {
            output::print_warning(&format!(
                "hit pagination safety limit ({MAX_PAGES} pages); truncating"
            ));
            break;
        }

        let resp = client
            .get(&page_url)
            .header("Authorization", format!("Bearer {token}"))
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "tc-task-commander")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .await
            .map_err(|e| CliError::user(format!("GitHub API request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp
                .text()
                .await
                .unwrap_or_else(|_| "<no body>".to_string());
            return Err(CliError::user(format!(
                "GitHub API returned {status}: {body}"
            )));
        }

        next_url = parse_github_next_link(resp.headers().get(reqwest::header::LINK));

        let page_issues: Vec<GithubIssue> = resp
            .json()
            .await
            .map_err(|e| CliError::user(format!("failed to parse GitHub response: {e}")))?;
        issues.extend(page_issues);
    }

    let issues: Vec<&GithubIssue> = issues
        .iter()
        .filter(|i| !i.labels.iter().any(|l| l.name == "pull_request"))
        .collect();

    if issues.is_empty() {
        output::print_warning("No issues found matching filters");
        return Ok(());
    }

    let dry_run = args.dry_run || dry_run_flag;
    let records: Vec<ImportRecord> = issues
        .iter()
        .map(|i| ImportRecord {
            source_ref: Some(format!("{}#{}", args.repo, i.number)),
            title: i.title.clone(),
            epic_override: None,
            priority: Priority::default(),
            tags: i.labels.iter().map(|l| l.name.clone()).collect(),
            due: None,
            scheduled: None,
            estimate: None,
            notes: i.body.clone().unwrap_or_default(),
            acceptance_criteria: Vec::new(),
        })
        .collect();

    apply_imports(records, &args.epic, dry_run, "github")
}

// ── Linear (GraphQL) ──────────────────────────────────────────────────

fn linear_api_key() -> Result<String, CliError> {
    std::env::var("LINEAR_API_KEY").map_err(|_| {
        CliError::user(
            "LINEAR_API_KEY not set. \
             Create an API key at https://linear.app/settings/api",
        )
    })
}

#[derive(Debug, Deserialize)]
struct LinearResponse {
    data: Option<LinearData>,
    errors: Option<Vec<LinearError>>,
}

#[derive(Debug, Deserialize)]
struct LinearError {
    message: String,
}

#[derive(Debug, Deserialize)]
struct LinearData {
    issues: LinearIssueConnection,
}

#[derive(Debug, Deserialize)]
struct LinearIssueConnection {
    nodes: Vec<LinearIssue>,
}

#[derive(Debug, Deserialize)]
struct LinearIssue {
    identifier: String,
    title: String,
    description: Option<String>,
    labels: LinearLabelConnection,
    #[allow(dead_code)]
    state: LinearState,
    #[allow(dead_code)]
    project: Option<LinearProject>,
}

#[derive(Debug, Deserialize)]
struct LinearLabelConnection {
    nodes: Vec<LinearLabel>,
}

#[derive(Debug, Deserialize)]
struct LinearLabel {
    name: String,
}

#[derive(Debug, Deserialize)]
struct LinearState {
    #[allow(dead_code)]
    name: String,
}

#[derive(Debug, Deserialize)]
struct LinearProject {
    #[allow(dead_code)]
    name: String,
}

async fn import_linear(args: LinearImportArgs, dry_run_flag: bool) -> Result<(), CliError> {
    let api_key = linear_api_key()?;
    let client = reqwest::Client::new();

    let mut filters = vec![format!(
        "{{ team: {{ key: {{ eq: \"{}\" }} }} }}",
        args.team
    )];

    if let Some(ref state) = args.state {
        filters.push(format!("{{ state: {{ name: {{ eq: \"{state}\" }} }} }}"));
    }
    if let Some(ref label) = args.label {
        filters.push(format!(
            "{{ labels: {{ some: {{ name: {{ eq: \"{label}\" }} }} }} }}"
        ));
    }
    if let Some(ref project) = args.project {
        filters.push(format!(
            "{{ project: {{ name: {{ eq: \"{project}\" }} }} }}"
        ));
    }

    let filter = if filters.len() == 1 {
        filters.into_iter().next().unwrap_or_default()
    } else {
        format!("{{ and: [{}] }}", filters.join(", "))
    };

    let query = format!(
        r#"{{
  issues(filter: {filter}, first: 100) {{
    nodes {{
      identifier
      title
      description
      labels {{
        nodes {{
          name
        }}
      }}
      state {{
        name
      }}
      project {{
        name
      }}
    }}
  }}
}}"#
    );

    let body = serde_json::json!({ "query": query });

    let resp = client
        .post("https://api.linear.app/graphql")
        .header("Authorization", api_key)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| CliError::user(format!("Linear API request failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp
            .text()
            .await
            .unwrap_or_else(|_| "<no body>".to_string());
        return Err(CliError::user(format!(
            "Linear API returned {status}: {body}"
        )));
    }

    let linear_resp: LinearResponse = resp
        .json()
        .await
        .map_err(|e| CliError::user(format!("failed to parse Linear response: {e}")))?;

    if let Some(errors) = linear_resp.errors {
        let msgs: Vec<String> = errors.into_iter().map(|e| e.message).collect();
        return Err(CliError::user(format!(
            "Linear API errors: {}",
            msgs.join("; ")
        )));
    }

    let data = linear_resp
        .data
        .ok_or_else(|| CliError::user("Linear API returned no data"))?;

    let issues = &data.issues.nodes;

    if issues.is_empty() {
        output::print_warning("No issues found matching filters");
        return Ok(());
    }

    let dry_run = args.dry_run || dry_run_flag;
    let records: Vec<ImportRecord> = issues
        .iter()
        .map(|i| ImportRecord {
            source_ref: Some(i.identifier.clone()),
            title: i.title.clone(),
            epic_override: None,
            priority: Priority::default(),
            tags: i.labels.nodes.iter().map(|l| l.name.clone()).collect(),
            due: None,
            scheduled: None,
            estimate: None,
            notes: i.description.clone().unwrap_or_default(),
            acceptance_criteria: Vec::new(),
        })
        .collect();

    apply_imports(records, &args.epic, dry_run, "linear")
}

// ── Shared apply pipeline ───────────────────────────────────────────

#[derive(Debug, Clone)]
struct ImportRecord {
    /// Optional stable key used for dedup -- recorded in notes as
    /// "Imported from: <source_ref>" so re-running the import is a no-op.
    source_ref: Option<String>,
    title: String,
    /// Per-record epic override; falls back to the import-wide epic.
    epic_override: Option<String>,
    priority: Priority,
    tags: Vec<String>,
    due: Option<NaiveDate>,
    scheduled: Option<NaiveDate>,
    estimate: Option<Duration>,
    notes: String,
    acceptance_criteria: Vec<String>,
}

fn apply_imports(
    records: Vec<ImportRecord>,
    default_epic: &str,
    dry_run: bool,
    source_label: &str,
) -> Result<(), CliError> {
    let store = tc_storage::Store::discover()?;
    let mut tasks = store.load_tasks()?;

    let already_imported: std::collections::HashSet<String> = tasks
        .iter()
        .filter_map(|t| {
            t.notes
                .lines()
                .find(|l| l.starts_with("Imported from: "))
                .map(|l| l.trim_start_matches("Imported from: ").to_string())
        })
        .collect();

    let mut created = Vec::new();
    let mut skipped = Vec::new();

    for r in records {
        if let Some(ref source_ref) = r.source_ref
            && already_imported.contains(source_ref)
        {
            skipped.push(source_ref.clone());
            continue;
        }

        let label = r
            .source_ref
            .clone()
            .unwrap_or_else(|| format!("{source_label}: {}", r.title));

        if dry_run {
            created.push(format!("[dry-run] {label}: {}", r.title));
            continue;
        }

        let id = store.next_task_id(&tasks);

        let mut notes = String::new();
        if let Some(ref source_ref) = r.source_ref {
            notes.push_str(&format!("Imported from: {source_ref}"));
        }
        if !r.notes.is_empty() {
            if !notes.is_empty() {
                notes.push_str("\n\n");
            }
            notes.push_str(&r.notes);
        }

        let task = Task {
            id: id.clone(),
            title: r.title.clone(),
            epic: r.epic_override.unwrap_or_else(|| default_epic.to_string()),
            status: StatusId("todo".to_string()),
            priority: r.priority,
            tags: r.tags,
            due: r.due,
            scheduled: r.scheduled,
            estimate: r.estimate,
            depends_on: vec![],
            files: vec![],
            pack_exclude: vec![],
            notes,
            acceptance_criteria: r.acceptance_criteria,
            assignee: None,
            created_at: Utc::now(),
        };

        created.push(format!("{id}: {}", r.title));
        tasks.push(task);
    }

    if !dry_run && !created.is_empty() {
        store.save_tasks(&tasks)?;
    }

    if !skipped.is_empty() {
        output::print_warning(&format!(
            "Skipped {} already-imported record(s): {}",
            skipped.len(),
            skipped.join(", ")
        ));
    }

    if created.is_empty() {
        output::print_warning("No new tasks to import");
    } else {
        for line in &created {
            output::print_success(line);
        }
        eprintln!(
            "\n{} task(s) {}",
            created.len(),
            if dry_run {
                "would be imported"
            } else {
                "imported"
            }
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_json_minimal() {
        let raw = r#"[{"title": "Hello"}]"#;
        let recs = parse_json(raw).unwrap();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].title, "Hello");
        assert_eq!(recs[0].priority, Priority::P3);
    }

    #[test]
    fn parse_json_full() {
        let raw = r#"[
            {
                "title": "Build login",
                "priority": "p1",
                "tags": ["backend", "auth"],
                "due": "2026-05-01",
                "estimate": "2h 30m",
                "notes": "see RFC",
                "acceptance_criteria": ["tests pass"],
                "source_ref": "spec#42"
            }
        ]"#;
        let recs = parse_json(raw).unwrap();
        assert_eq!(recs.len(), 1);
        let r = &recs[0];
        assert_eq!(r.priority, Priority::P1);
        assert_eq!(r.tags, vec!["backend", "auth"]);
        assert_eq!(r.due, NaiveDate::from_ymd_opt(2026, 5, 1));
        assert_eq!(r.estimate, Some(Duration::from_secs(2 * 3600 + 30 * 60)));
        assert_eq!(r.acceptance_criteria, vec!["tests pass"]);
        assert_eq!(r.source_ref.as_deref(), Some("spec#42"));
    }

    #[test]
    fn parse_kairo_md_basic() {
        let raw = "\
- [ ] Refactor auth #backend !p1 due:2026-05-15
- [ ] Write docs #docs
- this is not a task
- [x] Already done #legacy
";
        let recs = parse_kairo_md(raw);
        assert_eq!(recs.len(), 3);
        assert_eq!(recs[0].title, "Refactor auth");
        assert_eq!(recs[0].priority, Priority::P1);
        assert_eq!(recs[0].tags, vec!["backend"]);
        assert_eq!(recs[0].due, NaiveDate::from_ymd_opt(2026, 5, 15));
        assert_eq!(recs[1].title, "Write docs");
        assert_eq!(recs[1].tags, vec!["docs"]);
        assert_eq!(recs[2].title, "Already done");
    }

    #[test]
    fn parse_kairo_md_indented_become_acceptance_criteria() {
        let raw = "\
- [ ] Parent task #core
  - [ ] Sub item one
  - [ ] Sub item two
- [ ] Sibling
";
        let recs = parse_kairo_md(raw);
        assert_eq!(recs.len(), 2);
        assert_eq!(recs[0].title, "Parent task");
        assert_eq!(
            recs[0].acceptance_criteria,
            vec!["Sub item one", "Sub item two"]
        );
        assert_eq!(recs[1].title, "Sibling");
    }

    #[test]
    fn parse_linear_csv_standard_columns() {
        let raw = "\
ID,Title,Description,Priority,Labels,Due Date
ENG-1,Implement OAuth,Body text,Urgent,\"backend, auth\",2026-06-01
ENG-2,Fix typo,,Low,docs,
";
        let recs = parse_linear_csv(raw).unwrap();
        assert_eq!(recs.len(), 2);
        assert_eq!(recs[0].source_ref.as_deref(), Some("ENG-1"));
        assert_eq!(recs[0].title, "Implement OAuth");
        assert_eq!(recs[0].priority, Priority::P1);
        assert_eq!(recs[0].tags, vec!["backend", "auth"]);
        assert_eq!(recs[0].due, NaiveDate::from_ymd_opt(2026, 6, 1));
        assert_eq!(recs[1].priority, Priority::P4);
        assert_eq!(recs[1].tags, vec!["docs"]);
    }

    #[test]
    fn parse_linear_csv_skips_empty_titles() {
        let raw = "ID,Title\nENG-1,\nENG-2,Real one\n";
        let recs = parse_linear_csv(raw).unwrap();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].title, "Real one");
    }

    #[test]
    fn parse_linear_csv_missing_title_errors() {
        let raw = "ID,Body\nENG-1,whatever\n";
        let err = parse_linear_csv(raw).unwrap_err();
        assert!(err.to_string().contains("Title"));
    }

    #[test]
    fn linear_priority_mapping() {
        assert_eq!(linear_priority_from_str("Urgent"), Some(Priority::P1));
        assert_eq!(linear_priority_from_str("high"), Some(Priority::P2));
        assert_eq!(linear_priority_from_str("medium"), Some(Priority::P3));
        assert_eq!(linear_priority_from_str("Low"), Some(Priority::P4));
        assert_eq!(linear_priority_from_str("No priority"), Some(Priority::P5));
        assert_eq!(linear_priority_from_str(""), Some(Priority::P5));
        assert_eq!(linear_priority_from_str("garbage"), None);
    }

    #[test]
    fn dedup_detects_already_imported() {
        let existing_notes = "Imported from: owner/repo#42\n\nSome body text";
        let source_ref: Option<String> = existing_notes
            .lines()
            .find(|l| l.starts_with("Imported from: "))
            .map(|l| l.trim_start_matches("Imported from: ").to_string());
        assert_eq!(source_ref, Some("owner/repo#42".to_string()));
    }

    #[test]
    fn github_issue_filters_out_prs() {
        let issues = [
            GithubIssue {
                number: 1,
                title: "Real issue".to_string(),
                body: None,
                labels: vec![],
            },
            GithubIssue {
                number: 2,
                title: "A PR".to_string(),
                body: None,
                labels: vec![GithubLabel {
                    name: "pull_request".to_string(),
                }],
            },
        ];
        let filtered: Vec<&GithubIssue> = issues
            .iter()
            .filter(|i| !i.labels.iter().any(|l| l.name == "pull_request"))
            .collect();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].title, "Real issue");
    }

    #[test]
    fn linear_filter_single_team() {
        let team = "ENG";
        let filters = vec![format!("{{ team: {{ key: {{ eq: \"{}\" }} }} }}", team)];
        let filter = if filters.len() == 1 {
            filters.into_iter().next().unwrap_or_default()
        } else {
            format!("{{ and: [{}] }}", filters.join(", "))
        };
        assert!(filter.contains("ENG"));
        assert!(!filter.contains("and:"));
    }

    #[test]
    fn linear_filter_multiple() {
        let filters = [
            r#"{ team: { key: { eq: "ENG" } } }"#.to_string(),
            r#"{ state: { name: { eq: "Todo" } } }"#.to_string(),
        ];
        let filter = format!("{{ and: [{}] }}", filters.join(", "));
        assert!(filter.contains("and:"));
        assert!(filter.contains("ENG"));
        assert!(filter.contains("Todo"));
    }
}
