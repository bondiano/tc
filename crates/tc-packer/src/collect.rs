use std::path::PathBuf;

use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::WalkBuilder;

use crate::PackOptions;
use crate::error::PackerError;

#[derive(Debug)]
pub struct CollectedFile {
    pub path: PathBuf,
    pub content: String,
}

/// Collect files from `options.root`, respecting .gitignore, include/exclude filters.
/// Binary files are skipped. Paths in results are relative to `options.root`.
pub fn collect_files(options: &PackOptions) -> Result<Vec<CollectedFile>, PackerError> {
    let exclude_set = build_glob_set(&options.exclude_patterns)?;
    let include_set = build_glob_set(&options.include_paths)?;
    let has_includes = !options.include_paths.is_empty();

    let walker = WalkBuilder::new(&options.root)
        .hidden(true)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(true)
        .build();

    let mut files = Vec::new();

    'walk: for entry in walker {
        let entry = entry.map_err(|e| PackerError::walk(&options.root, e.to_string()))?;

        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue 'walk;
        }

        let abs_path = entry.path();
        let rel_path = abs_path
            .strip_prefix(&options.root)
            .unwrap_or(abs_path)
            .to_path_buf();

        let rel_str = rel_path.to_string_lossy();

        // Apply include filter: if include_paths given, file must match at least one
        if has_includes && !include_set.is_match(rel_str.as_ref()) {
            continue 'walk;
        }

        // Apply exclude filter
        if exclude_set.is_match(rel_str.as_ref()) {
            continue 'walk;
        }

        let bytes =
            std::fs::read(abs_path).map_err(|e| PackerError::file_read(rel_path.clone(), e))?;

        // Skip binary files (scan first 8KB for null bytes)
        if bytes.iter().take(8192).any(|b| *b == 0) {
            continue 'walk;
        }

        let content = match String::from_utf8(bytes) {
            Ok(s) => s,
            Err(_) => continue 'walk,
        };

        files.push(CollectedFile {
            path: rel_path,
            content,
        });
    }

    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

fn build_glob_set(patterns: &[String]) -> Result<GlobSet, PackerError> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = Glob::new(pattern)
            .map_err(|e| PackerError::invalid_glob(pattern.clone(), e.to_string()))?;
        builder.add(glob);
    }
    builder
        .build()
        .map_err(|e| PackerError::invalid_glob("<set>", e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_project(files: &[(&str, &str)]) -> TempDir {
        let dir = TempDir::new().unwrap();
        for (path, content) in files {
            let full = dir.path().join(path);
            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&full, content).unwrap();
        }
        dir
    }

    fn default_options(root: PathBuf) -> PackOptions {
        PackOptions {
            root,
            include_paths: vec![],
            exclude_patterns: vec![],
            token_budget: 80_000,
            style: crate::PackStyle::Markdown,
        }
    }

    #[test]
    fn collect_simple_project() {
        let dir = setup_project(&[
            ("src/main.rs", "fn main() {}"),
            ("src/lib.rs", "pub mod foo;"),
            ("README.md", "# Hello"),
        ]);
        let opts = default_options(dir.path().to_path_buf());
        let files = collect_files(&opts).unwrap();
        assert_eq!(files.len(), 3);

        let paths: Vec<String> = files.iter().map(|f| f.path.display().to_string()).collect();
        assert!(paths.contains(&"README.md".to_string()));
        assert!(paths.contains(&"src/main.rs".to_string()));
        assert!(paths.contains(&"src/lib.rs".to_string()));
    }

    #[test]
    fn collect_respects_gitignore() {
        let dir = setup_project(&[
            (".gitignore", "target/\n*.log"),
            ("src/main.rs", "fn main() {}"),
            ("target/debug/bin", "binary stuff"),
            ("app.log", "some log"),
        ]);
        // ignore crate needs a git repo to process .gitignore
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        let opts = default_options(dir.path().to_path_buf());
        let files = collect_files(&opts).unwrap();

        let paths: Vec<String> = files.iter().map(|f| f.path.display().to_string()).collect();
        assert!(paths.contains(&"src/main.rs".to_string()));
        assert!(!paths.contains(&"target/debug/bin".to_string()));
        assert!(!paths.contains(&"app.log".to_string()));
    }

    #[test]
    fn collect_include_paths_filter() {
        let dir = setup_project(&[
            ("src/main.rs", "fn main() {}"),
            ("src/lib.rs", "pub mod foo;"),
            ("docs/readme.md", "# Docs"),
            ("config.yaml", "key: val"),
        ]);
        let mut opts = default_options(dir.path().to_path_buf());
        opts.include_paths = vec!["src/**".to_string()];
        let files = collect_files(&opts).unwrap();

        let paths: Vec<String> = files.iter().map(|f| f.path.display().to_string()).collect();
        assert_eq!(files.len(), 2);
        assert!(paths.contains(&"src/main.rs".to_string()));
        assert!(paths.contains(&"src/lib.rs".to_string()));
    }

    #[test]
    fn collect_exclude_patterns() {
        let dir = setup_project(&[
            ("src/main.rs", "fn main() {}"),
            ("src/test_helper.rs", "// test"),
            ("Cargo.lock", "locked"),
        ]);
        let mut opts = default_options(dir.path().to_path_buf());
        opts.exclude_patterns = vec!["Cargo.lock".to_string(), "**/test_*".to_string()];
        let files = collect_files(&opts).unwrap();

        let paths: Vec<String> = files.iter().map(|f| f.path.display().to_string()).collect();
        assert_eq!(files.len(), 1);
        assert!(paths.contains(&"src/main.rs".to_string()));
    }

    #[test]
    fn collect_skips_binary_files() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("text.txt"), "hello world").unwrap();
        // Binary file with null bytes
        fs::write(dir.path().join("image.png"), b"\x89PNG\r\n\x1a\n\x00\x00").unwrap();

        let opts = default_options(dir.path().to_path_buf());
        let files = collect_files(&opts).unwrap();

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path.display().to_string(), "text.txt");
    }

    #[test]
    fn collect_relative_paths() {
        let dir = setup_project(&[("deep/nested/file.rs", "// code")]);
        let opts = default_options(dir.path().to_path_buf());
        let files = collect_files(&opts).unwrap();

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, PathBuf::from("deep/nested/file.rs"));
    }

    #[test]
    fn collect_empty_dir() {
        let dir = TempDir::new().unwrap();
        let opts = default_options(dir.path().to_path_buf());
        let files = collect_files(&opts).unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn collect_invalid_glob_returns_error() {
        let dir = TempDir::new().unwrap();
        let mut opts = default_options(dir.path().to_path_buf());
        opts.exclude_patterns = vec!["**[invalid".to_string()];
        let result = collect_files(&opts);
        assert!(result.is_err());
    }

    #[test]
    fn collect_sorted_by_path() {
        let dir = setup_project(&[("z.txt", "z"), ("a.txt", "a"), ("m/b.txt", "b")]);
        let opts = default_options(dir.path().to_path_buf());
        let files = collect_files(&opts).unwrap();

        let paths: Vec<String> = files.iter().map(|f| f.path.display().to_string()).collect();
        assert_eq!(paths, vec!["a.txt", "m/b.txt", "z.txt"]);
    }

    #[test]
    fn collect_skips_binary_by_null_bytes() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("text.txt"), "hello").unwrap();
        fs::write(dir.path().join("bin.dat"), b"hello\x00world").unwrap();

        let opts = default_options(dir.path().to_path_buf());
        let files = collect_files(&opts).unwrap();
        let paths: Vec<String> = files.iter().map(|f| f.path.display().to_string()).collect();
        assert!(paths.contains(&"text.txt".to_string()));
        assert!(!paths.contains(&"bin.dat".to_string()));
    }
}
