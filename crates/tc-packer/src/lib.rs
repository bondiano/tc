pub mod collect;
pub mod error;
pub mod format;
pub mod security;
pub mod tokens;

use std::path::PathBuf;

pub use error::{PackerError, PackerResult};

use collect::collect_files;
use format::format_files;
use security::scan_for_secrets;
use tokens::estimate_tokens;

#[derive(Debug, Clone)]
pub struct PackOptions {
    pub root: PathBuf,
    pub include_paths: Vec<String>,
    pub exclude_patterns: Vec<String>,
    pub token_budget: usize,
    pub style: PackStyle,
}

#[derive(Debug, Clone)]
pub enum PackStyle {
    Markdown,
    Xml,
}

impl From<&str> for PackStyle {
    fn from(s: &str) -> Self {
        match s {
            "xml" => PackStyle::Xml,
            _ => PackStyle::Markdown,
        }
    }
}

#[derive(Debug)]
pub struct PackResult {
    pub content: String,
    pub token_estimate: usize,
    pub file_count: usize,
    pub warnings: Vec<String>,
}

/// Pack codebase files into a single formatted string.
///
/// Pipeline: collect -> security scan -> format -> token budget check.
/// Files from `include_paths` get priority when token budget is exceeded.
pub fn pack(options: &PackOptions) -> Result<PackResult, PackerError> {
    let mut files = collect_files(options)?;
    let mut warnings = Vec::new();

    'scan: for file in &files {
        let secrets = scan_for_secrets(&file.content);
        'secret: for secret in secrets {
            warnings.push(format!(
                "potential secret in '{}': {secret}",
                file.path.display()
            ));
            continue 'secret;
        }
        continue 'scan;
    }

    let has_includes = !options.include_paths.is_empty();
    if has_includes && options.token_budget > 0 {
        let (priority, rest): (Vec<_>, Vec<_>) = files.into_iter().partition(|f| {
            let path_str = f.path.to_string_lossy();
            options
                .include_paths
                .iter()
                .any(|p| path_str.starts_with(p.trim_end_matches('*').trim_end_matches('/')))
        });

        files = priority;
        files.extend(rest);
    }

    let content = format_files(&files, &options.style);
    let token_estimate = estimate_tokens(&content);
    let file_count = files.len();

    // Soft limit: callers decide whether to truncate.
    if options.token_budget > 0 && token_estimate > options.token_budget {
        warnings.push(format!(
            "token budget exceeded: ~{token_estimate} tokens > {} budget",
            options.token_budget
        ));
    }

    Ok(PackResult {
        content,
        token_estimate,
        file_count,
        warnings,
    })
}

/// Estimate-only mode: collect and count tokens without formatting.
pub fn estimate(options: &PackOptions) -> Result<(usize, usize), PackerError> {
    let files = collect_files(options)?;
    let combined: String = files.iter().map(|f| f.content.as_str()).collect();
    Ok((estimate_tokens(&combined), files.len()))
}
