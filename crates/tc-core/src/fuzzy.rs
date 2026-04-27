//! Fuzzy task matching backed by `nucleo-matcher`.
//!
//! The index over each task is the concatenation of `id`, `title`, and tags
//! (joined with `|` so tag boundaries are tokenized). Scoring follows nucleo:
//! higher is better. We sort descending by score and return up to `limit`
//! results.
//!
//! Kept in `tc-core` so the same matcher can drive both `tc find` (M-6.6) and
//! the TUI fuzzy finder (M-7.1).

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32String};

use crate::task::{Task, TaskId};

#[derive(Debug, Clone)]
pub struct FuzzyHit {
    pub id: TaskId,
    pub score: u32,
}

/// Build the haystack string for a task. Public so callers can preview the
/// indexed form (useful in tests and TUI listings).
pub fn haystack(task: &Task) -> String {
    if task.tags.is_empty() {
        format!("{} {}", task.id, task.title)
    } else {
        format!("{} {} | {}", task.id, task.title, task.tags.join(" "))
    }
}

/// Run a fuzzy query against the task list, returning matches ordered by
/// descending score, capped at `limit`. An empty `query` returns the first
/// `limit` tasks in input order with score 0 -- handy for "open the picker
/// with no filter" flows.
pub fn search(tasks: &[Task], query: &str, limit: usize) -> Vec<FuzzyHit> {
    if limit == 0 {
        return Vec::new();
    }
    if query.trim().is_empty() {
        return tasks
            .iter()
            .take(limit)
            .map(|t| FuzzyHit {
                id: t.id.clone(),
                score: 0,
            })
            .collect();
    }

    let mut matcher = Matcher::new(Config::DEFAULT);
    let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);

    let mut hits: Vec<FuzzyHit> = tasks
        .iter()
        .filter_map(|t| {
            let hay = Utf32String::from(haystack(t));
            pattern
                .score(hay.slice(..), &mut matcher)
                .map(|score| FuzzyHit {
                    id: t.id.clone(),
                    score,
                })
        })
        .collect();

    // Stable secondary sort on `id` so equal scores tie-break deterministically.
    hits.sort_by(|a, b| b.score.cmp(&a.score).then(a.id.0.cmp(&b.id.0)));
    hits.truncate(limit);
    hits
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    use crate::status::StatusId;
    use crate::task::Priority;

    fn task(id: &str, title: &str, tags: &[&str]) -> Task {
        Task {
            id: TaskId(id.into()),
            title: title.into(),
            epic: "default".into(),
            status: StatusId("todo".into()),
            priority: Priority::default(),
            tags: tags.iter().map(|s| (*s).to_string()).collect(),
            due: None,
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

    #[test]
    fn empty_query_returns_input_order_capped() {
        let tasks = vec![
            task("T-001", "First", &[]),
            task("T-002", "Second", &[]),
            task("T-003", "Third", &[]),
        ];
        let hits = search(&tasks, "", 2);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].id.0, "T-001");
        assert_eq!(hits[1].id.0, "T-002");
    }

    #[test]
    fn empty_corpus_yields_no_hits() {
        let hits = search(&[], "anything", 10);
        assert!(hits.is_empty());
    }

    #[test]
    fn matches_by_title_substring() {
        let tasks = vec![
            task("T-001", "Implement login flow", &[]),
            task("T-002", "Refactor logger", &[]),
            task("T-003", "Optimise build cache", &[]),
        ];
        let hits = search(&tasks, "log", 5);
        let ids: Vec<&str> = hits.iter().map(|h| h.id.0.as_str()).collect();
        assert!(ids.contains(&"T-001"));
        assert!(ids.contains(&"T-002"));
        assert!(!ids.contains(&"T-003"));
    }

    #[test]
    fn matches_by_tag() {
        let tasks = vec![
            task("T-001", "Misc", &["backend"]),
            task("T-002", "Misc", &["frontend"]),
        ];
        let hits = search(&tasks, "backend", 5);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id.0, "T-001");
    }

    #[test]
    fn matches_by_id_prefix() {
        let tasks = vec![task("T-042", "Random", &[]), task("T-100", "Other", &[])];
        let hits = search(&tasks, "T-04", 5);
        assert!(!hits.is_empty());
        assert_eq!(hits[0].id.0, "T-042");
    }

    #[test]
    fn limit_zero_returns_empty() {
        let tasks = vec![task("T-001", "Anything", &[])];
        let hits = search(&tasks, "any", 0);
        assert!(hits.is_empty());
    }

    #[test]
    fn ranks_better_match_higher() {
        let tasks = vec![
            task("T-001", "fuzzy match search engine", &[]),
            task("T-002", "unrelated task name", &[]),
            task("T-003", "fuzz", &[]),
        ];
        let hits = search(&tasks, "fuzz", 5);
        // T-003 is a tighter substring match than T-001's "fuzzy", but
        // either ordering is acceptable -- what we *must* see is T-002
        // ranked last (or filtered out).
        assert!(hits.iter().any(|h| h.id.0 == "T-001"));
        assert!(hits.iter().any(|h| h.id.0 == "T-003"));
        let ids_in_order: Vec<&str> = hits.iter().map(|h| h.id.0.as_str()).collect();
        if let Some(pos) = ids_in_order.iter().position(|&id| id == "T-002") {
            assert_eq!(
                pos,
                ids_in_order.len() - 1,
                "T-002 should rank last if matched at all"
            );
        }
    }

    #[test]
    fn haystack_includes_tags() {
        let t = task("T-007", "Hello", &["alpha", "beta"]);
        let h = haystack(&t);
        assert!(h.contains("T-007"));
        assert!(h.contains("Hello"));
        assert!(h.contains("alpha"));
        assert!(h.contains("beta"));
    }
}
