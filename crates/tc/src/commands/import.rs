use chrono::Utc;
use serde::Deserialize;
use tc_core::status::StatusId;
use tc_core::task::Task;

use crate::cli::{GithubImportArgs, ImportArgs, ImportSource, LinearImportArgs};
use crate::error::CliError;
use crate::output;

pub async fn run(args: ImportArgs) -> Result<(), CliError> {
    match args.source {
        ImportSource::Github(gh) => import_github(gh).await,
        ImportSource::Linear(lin) => import_linear(lin).await,
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
///
/// GitHub responses look like:
/// `Link: <https://api.github.com/...?page=2>; rel="next", <...>; rel="last"`.
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
    // Fallback: try `gh auth token`
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

async fn import_github(args: GithubImportArgs) -> Result<(), CliError> {
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

    // Paginate through GitHub's `Link: <...>; rel="next"` header. Without
    // this, any repo with more than 100 matching issues silently truncated.
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

    // Filter out pull requests (GitHub API returns PRs as issues too)
    let issues: Vec<&GithubIssue> = issues
        .iter()
        .filter(|i| !i.labels.iter().any(|l| l.name == "pull_request"))
        .collect();

    if issues.is_empty() {
        output::print_warning("No issues found matching filters");
        return Ok(());
    }

    create_tasks_from_external(
        &issues
            .iter()
            .map(|i| ExternalIssue {
                source_ref: format!("{}#{}", args.repo, i.number),
                title: i.title.clone(),
                body: i.body.clone().unwrap_or_default(),
                labels: i.labels.iter().map(|l| l.name.clone()).collect(),
            })
            .collect::<Vec<_>>(),
        &args.epic,
        args.dry_run,
    )
}

// ── Linear ──────────────────────────────────────────────────────────

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
    // Requested in GraphQL query for completeness; not read locally.
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

async fn import_linear(args: LinearImportArgs) -> Result<(), CliError> {
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

    create_tasks_from_external(
        &issues
            .iter()
            .map(|i| ExternalIssue {
                source_ref: i.identifier.clone(),
                title: i.title.clone(),
                body: i.description.clone().unwrap_or_default(),
                labels: i.labels.nodes.iter().map(|l| l.name.clone()).collect(),
            })
            .collect::<Vec<_>>(),
        &args.epic,
        args.dry_run,
    )
}

// ── Shared task creation ────────────────────────────────────────────

struct ExternalIssue {
    source_ref: String,
    title: String,
    body: String,
    labels: Vec<String>,
}

fn create_tasks_from_external(
    issues: &[ExternalIssue],
    epic: &str,
    dry_run: bool,
) -> Result<(), CliError> {
    let store = tc_storage::Store::discover()?;
    let mut tasks = store.load_tasks()?;

    // Detect already-imported issues by checking notes for source_ref
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

    for issue in issues {
        if already_imported.contains(&issue.source_ref) {
            skipped.push(&issue.source_ref);
            continue;
        }

        if dry_run {
            created.push(format!("[dry-run] {}: {}", issue.source_ref, issue.title));
            continue;
        }

        let id = store.next_task_id(&tasks);

        let mut notes = format!("Imported from: {}", issue.source_ref);
        if !issue.body.is_empty() {
            notes.push_str("\n\n");
            notes.push_str(&issue.body);
        }
        if !issue.labels.is_empty() {
            notes.push_str(&format!("\n\nLabels: {}", issue.labels.join(", ")));
        }

        let task = Task {
            id: id.clone(),
            title: issue.title.clone(),
            epic: epic.to_string(),
            status: StatusId("todo".to_string()),
            priority: Default::default(),
            depends_on: vec![],
            files: vec![],
            pack_exclude: vec![],
            notes,
            acceptance_criteria: vec![],
            assignee: None,
            created_at: Utc::now(),
        };

        created.push(format!("{id}: {}", issue.title));
        tasks.push(task);
    }

    if !dry_run && !created.is_empty() {
        store.save_tasks(&tasks)?;
    }

    if !skipped.is_empty() {
        output::print_warning(&format!(
            "Skipped {} already-imported issue(s): {}",
            skipped.len(),
            skipped
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
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
    fn external_issue_to_task_notes_format() {
        let issue = ExternalIssue {
            source_ref: "owner/repo#42".to_string(),
            title: "Fix login bug".to_string(),
            body: "Users cannot login".to_string(),
            labels: vec!["bug".to_string(), "auth".to_string()],
        };

        let mut notes = format!("Imported from: {}", issue.source_ref);
        notes.push_str("\n\n");
        notes.push_str(&issue.body);
        notes.push_str(&format!("\n\nLabels: {}", issue.labels.join(", ")));

        assert!(notes.contains("Imported from: owner/repo#42"));
        assert!(notes.contains("Users cannot login"));
        assert!(notes.contains("Labels: bug, auth"));
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
    fn dedup_no_match_for_normal_notes() {
        let existing_notes = "Regular notes without import marker";
        let source_ref: Option<String> = existing_notes
            .lines()
            .find(|l| l.starts_with("Imported from: "))
            .map(|l| l.trim_start_matches("Imported from: ").to_string());

        assert_eq!(source_ref, None);
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
