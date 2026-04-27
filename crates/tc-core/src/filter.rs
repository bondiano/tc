//! Filter DSL for `tc list` and other task-listing commands.
//!
//! Grammar (intentionally tiny):
//! ```text
//! query  := term (WS+ term)*
//! term   := KEY ":" VALUE | "!" KEY ":" VALUE
//! KEY    := priority | tag | status | epic | due | scheduled | id | title
//! VALUE  := word | "today" | "tomorrow" | "overdue" | "any" | "none"
//!         | YYYY-MM-DD | YYYY-MM-DD..YYYY-MM-DD
//! ```
//!
//! All terms are AND-combined. Repeating a key like `tag:a tag:b` requires
//! *both* tags. Negation with a leading `!` (e.g. `!tag:wip`) flips a single
//! term. Unknown keys produce a parse error rather than silently skipping --
//! a typo like `prio:p1` returning every task is worse than failing loudly.
//!
//! The DSL is a `tc-core` concern (no I/O) so the same parser drives the CLI
//! `tc list "..."` (M-6.4), the smart-view shortcuts (M-6.5), and the future
//! TUI live filter (M-7.1).

use std::str::FromStr;

use chrono::NaiveDate;

use crate::status::StatusId;
use crate::task::{Priority, Task, TaskId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DateMatcher {
    Exact(NaiveDate),
    Range { start: NaiveDate, end: NaiveDate },
    Today,
    Tomorrow,
    Overdue,
    Any,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Term {
    Priority(Priority),
    Tag(String),
    Status(StatusId),
    Epic(String),
    Id(String),
    TitleContains(String),
    Due(DateMatcher),
    Scheduled(DateMatcher),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Predicate {
    pub term: Term,
    pub negate: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    pub predicates: Vec<Predicate>,
}

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum FilterError {
    #[error(
        "unknown filter key '{0}' (valid: priority, tag, status, epic, due, scheduled, id, title)"
    )]
    #[diagnostic(code(tc::core::filter::unknown_key))]
    UnknownKey(String),

    #[error("missing ':' in filter term '{0}'; use KEY:VALUE")]
    #[diagnostic(code(tc::core::filter::missing_colon))]
    MissingColon(String),

    #[error("invalid priority '{0}' (valid: p1, p2, p3, p4, p5)")]
    #[diagnostic(code(tc::core::filter::invalid_priority))]
    InvalidPriority(String),

    #[error("invalid date '{value}': {reason}")]
    #[diagnostic(code(tc::core::filter::invalid_date))]
    InvalidDate { value: String, reason: String },

    #[error("empty value for key '{0}'")]
    #[diagnostic(code(tc::core::filter::empty_value))]
    EmptyValue(String),
}

impl FilterError {
    fn invalid_date(value: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::InvalidDate {
            value: value.into(),
            reason: reason.into(),
        }
    }
}

impl Filter {
    /// Parse a filter expression. Empty string -> empty (matches everything).
    pub fn parse(input: &str) -> Result<Self, FilterError> {
        let mut predicates = Vec::new();
        'tokens: for raw in input.split_whitespace() {
            if raw.is_empty() {
                continue 'tokens;
            }
            predicates.push(parse_term(raw)?);
        }
        Ok(Self { predicates })
    }

    /// True when no predicates -- matches every task.
    pub fn is_empty(&self) -> bool {
        self.predicates.is_empty()
    }

    /// Evaluate the filter against a task using `today` as "now".
    pub fn matches(&self, task: &Task, today: NaiveDate) -> bool {
        self.predicates
            .iter()
            .all(|p| eval_predicate(p, task, today))
    }
}

impl FromStr for Filter {
    type Err = FilterError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

fn parse_term(raw: &str) -> Result<Predicate, FilterError> {
    let (negate, body) = match raw.strip_prefix('!') {
        Some(rest) => (true, rest),
        None => (false, raw),
    };

    let (key, value) = body
        .split_once(':')
        .ok_or_else(|| FilterError::MissingColon(raw.to_string()))?;

    if value.is_empty() {
        return Err(FilterError::EmptyValue(key.to_string()));
    }

    let term = match key {
        "priority" | "p" | "prio" => Term::Priority(parse_priority(value)?),
        "tag" | "tags" => Term::Tag(value.to_string()),
        "status" | "s" => Term::Status(StatusId(value.to_string())),
        "epic" | "e" => Term::Epic(value.to_string()),
        "id" => Term::Id(value.to_string()),
        "title" | "t" => Term::TitleContains(value.to_lowercase()),
        "due" => Term::Due(parse_date_matcher(value)?),
        "scheduled" | "sched" => Term::Scheduled(parse_date_matcher(value)?),
        other => return Err(FilterError::UnknownKey(other.to_string())),
    };

    Ok(Predicate { term, negate })
}

fn parse_priority(value: &str) -> Result<Priority, FilterError> {
    match value.to_lowercase().as_str() {
        "p1" | "1" | "critical" => Ok(Priority::P1),
        "p2" | "2" | "high" => Ok(Priority::P2),
        "p3" | "3" | "normal" => Ok(Priority::P3),
        "p4" | "4" | "low" => Ok(Priority::P4),
        "p5" | "5" => Ok(Priority::P5),
        _ => Err(FilterError::InvalidPriority(value.to_string())),
    }
}

fn parse_date_matcher(value: &str) -> Result<DateMatcher, FilterError> {
    match value.to_lowercase().as_str() {
        "today" => return Ok(DateMatcher::Today),
        "tomorrow" => return Ok(DateMatcher::Tomorrow),
        "overdue" => return Ok(DateMatcher::Overdue),
        "any" | "set" => return Ok(DateMatcher::Any),
        "none" | "unset" => return Ok(DateMatcher::None),
        _ => {}
    }

    if let Some((a, b)) = value.split_once("..") {
        let start = parse_date(a)?;
        let end = parse_date(b)?;
        return Ok(DateMatcher::Range { start, end });
    }

    parse_date(value).map(DateMatcher::Exact)
}

fn parse_date(value: &str) -> Result<NaiveDate, FilterError> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|e| FilterError::invalid_date(value, e.to_string()))
}

fn eval_predicate(p: &Predicate, task: &Task, today: NaiveDate) -> bool {
    let raw = eval_term(&p.term, task, today);
    if p.negate { !raw } else { raw }
}

fn eval_term(term: &Term, task: &Task, today: NaiveDate) -> bool {
    match term {
        Term::Priority(p) => task.priority == *p,
        Term::Tag(t) => task.tags.iter().any(|tt| tt.eq_ignore_ascii_case(t)),
        Term::Status(s) => task.status == *s,
        Term::Epic(e) => task.epic.eq_ignore_ascii_case(e),
        Term::Id(id) => task.id == TaskId(id.clone()) || task.id.0.eq_ignore_ascii_case(id),
        Term::TitleContains(needle) => task.title.to_lowercase().contains(needle),
        Term::Due(m) => match_date(m, task.due, today),
        Term::Scheduled(m) => match_date(m, task.scheduled, today),
    }
}

fn match_date(m: &DateMatcher, value: Option<NaiveDate>, today: NaiveDate) -> bool {
    match (m, value) {
        (DateMatcher::Any, v) => v.is_some(),
        (DateMatcher::None, v) => v.is_none(),
        (_, None) => false,
        (DateMatcher::Exact(d), Some(v)) => v == *d,
        (DateMatcher::Range { start, end }, Some(v)) => v >= *start && v <= *end,
        (DateMatcher::Today, Some(v)) => v == today,
        (DateMatcher::Tomorrow, Some(v)) => v == today.succ_opt().unwrap_or(today),
        (DateMatcher::Overdue, Some(v)) => v < today,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn task(
        id: &str,
        priority: Priority,
        tags: &[&str],
        epic: &str,
        status: &str,
        due: Option<NaiveDate>,
    ) -> Task {
        Task {
            id: TaskId(id.into()),
            title: format!("Task {id}"),
            epic: epic.into(),
            status: StatusId(status.into()),
            priority,
            tags: tags.iter().map(|s| (*s).to_string()).collect(),
            due,
            scheduled: None,
            estimate: None,
            depends_on: vec![],
            files: vec![],
            pack_exclude: vec![],
            notes: String::new(),
            acceptance_criteria: vec![],
            assignee: None,
            created_at: Utc::now(),
        }
    }

    fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).expect("valid date")
    }

    #[test]
    fn empty_filter_matches_everything() {
        let f = Filter::parse("").unwrap();
        let t = task("T-001", Priority::P3, &[], "be", "todo", None);
        assert!(f.is_empty());
        assert!(f.matches(&t, ymd(2026, 4, 26)));
    }

    #[test]
    fn priority_term() {
        let f = Filter::parse("priority:p1").unwrap();
        let p1 = task("T-001", Priority::P1, &[], "be", "todo", None);
        let p3 = task("T-002", Priority::P3, &[], "be", "todo", None);
        let today = ymd(2026, 4, 26);
        assert!(f.matches(&p1, today));
        assert!(!f.matches(&p3, today));
    }

    #[test]
    fn priority_aliases() {
        let f = Filter::parse("p:1").unwrap();
        assert!(matches!(f.predicates[0].term, Term::Priority(Priority::P1)));
    }

    #[test]
    fn tag_term_case_insensitive() {
        let f = Filter::parse("tag:Backend").unwrap();
        let t = task("T-001", Priority::P3, &["backend"], "be", "todo", None);
        let other = task("T-002", Priority::P3, &["frontend"], "be", "todo", None);
        let today = ymd(2026, 4, 26);
        assert!(f.matches(&t, today));
        assert!(!f.matches(&other, today));
    }

    #[test]
    fn multiple_tags_are_anded() {
        let f = Filter::parse("tag:backend tag:perf").unwrap();
        let both = task(
            "T-001",
            Priority::P3,
            &["backend", "perf"],
            "be",
            "todo",
            None,
        );
        let only_be = task("T-002", Priority::P3, &["backend"], "be", "todo", None);
        let today = ymd(2026, 4, 26);
        assert!(f.matches(&both, today));
        assert!(!f.matches(&only_be, today));
    }

    #[test]
    fn negation_flips_a_term() {
        let f = Filter::parse("!tag:wip").unwrap();
        let wip = task("T-001", Priority::P3, &["wip"], "be", "todo", None);
        let other = task("T-002", Priority::P3, &["clean"], "be", "todo", None);
        let today = ymd(2026, 4, 26);
        assert!(!f.matches(&wip, today));
        assert!(f.matches(&other, today));
    }

    #[test]
    fn status_and_epic_terms() {
        let f = Filter::parse("status:done epic:auth").unwrap();
        let hit = task("T-001", Priority::P3, &[], "auth", "done", None);
        let miss = task("T-002", Priority::P3, &[], "auth", "todo", None);
        let today = ymd(2026, 4, 26);
        assert!(f.matches(&hit, today));
        assert!(!f.matches(&miss, today));
    }

    #[test]
    fn due_today() {
        let today = ymd(2026, 4, 26);
        let f = Filter::parse("due:today").unwrap();
        let due = task("T-001", Priority::P3, &[], "be", "todo", Some(today));
        let no_due = task("T-002", Priority::P3, &[], "be", "todo", None);
        assert!(f.matches(&due, today));
        assert!(!f.matches(&no_due, today));
    }

    #[test]
    fn due_overdue() {
        let today = ymd(2026, 4, 26);
        let f = Filter::parse("due:overdue").unwrap();
        let past = task(
            "T-001",
            Priority::P3,
            &[],
            "be",
            "todo",
            Some(ymd(2026, 4, 20)),
        );
        let future = task(
            "T-002",
            Priority::P3,
            &[],
            "be",
            "todo",
            Some(ymd(2026, 5, 20)),
        );
        assert!(f.matches(&past, today));
        assert!(!f.matches(&future, today));
    }

    #[test]
    fn due_range() {
        let today = ymd(2026, 4, 26);
        let f = Filter::parse("due:2026-04-27..2026-05-03").unwrap();
        let inside = task(
            "T-001",
            Priority::P3,
            &[],
            "be",
            "todo",
            Some(ymd(2026, 4, 30)),
        );
        let outside = task(
            "T-002",
            Priority::P3,
            &[],
            "be",
            "todo",
            Some(ymd(2026, 5, 4)),
        );
        assert!(f.matches(&inside, today));
        assert!(!f.matches(&outside, today));
    }

    #[test]
    fn due_any_and_none() {
        let today = ymd(2026, 4, 26);
        let f_any = Filter::parse("due:any").unwrap();
        let f_none = Filter::parse("due:none").unwrap();
        let with_due = task("T-001", Priority::P3, &[], "be", "todo", Some(today));
        let no_due = task("T-002", Priority::P3, &[], "be", "todo", None);

        assert!(f_any.matches(&with_due, today));
        assert!(!f_any.matches(&no_due, today));
        assert!(!f_none.matches(&with_due, today));
        assert!(f_none.matches(&no_due, today));
    }

    #[test]
    fn unknown_key_errors() {
        let err = Filter::parse("prio_typo:p1").unwrap_err();
        assert!(matches!(err, FilterError::UnknownKey(_)));
    }

    #[test]
    fn missing_colon_errors() {
        let err = Filter::parse("tag").unwrap_err();
        assert!(matches!(err, FilterError::MissingColon(_)));
    }

    #[test]
    fn empty_value_errors() {
        let err = Filter::parse("tag:").unwrap_err();
        assert!(matches!(err, FilterError::EmptyValue(_)));
    }

    #[test]
    fn invalid_priority_errors() {
        let err = Filter::parse("priority:p9").unwrap_err();
        assert!(matches!(err, FilterError::InvalidPriority(_)));
    }

    #[test]
    fn invalid_date_errors() {
        let err = Filter::parse("due:2026/05/01").unwrap_err();
        assert!(matches!(err, FilterError::InvalidDate { .. }));
    }

    #[test]
    fn title_contains_is_lowercase_substring() {
        let f = Filter::parse("title:login").unwrap();
        let mut t = task("T-001", Priority::P3, &[], "be", "todo", None);
        t.title = "Fix LOGIN bug".into();
        let today = ymd(2026, 4, 26);
        assert!(f.matches(&t, today));
    }

    #[test]
    fn id_term_matches() {
        let f = Filter::parse("id:T-042").unwrap();
        let hit = task("T-042", Priority::P3, &[], "be", "todo", None);
        let miss = task("T-043", Priority::P3, &[], "be", "todo", None);
        let today = ymd(2026, 4, 26);
        assert!(f.matches(&hit, today));
        assert!(!f.matches(&miss, today));
    }

    #[test]
    fn complex_query_combines_terms() {
        let today = ymd(2026, 4, 26);
        let f = Filter::parse("priority:p1 tag:backend status:todo").unwrap();
        let hit = task("T-001", Priority::P1, &["backend"], "auth", "todo", None);
        let wrong_status = task("T-002", Priority::P1, &["backend"], "auth", "done", None);
        assert!(f.matches(&hit, today));
        assert!(!f.matches(&wrong_status, today));
    }
}
