use crate::PackStyle;
use crate::collect::CollectedFile;

/// Format collected files into a single string using the specified style.
pub fn format_files(files: &[CollectedFile], style: &PackStyle) -> String {
    match style {
        PackStyle::Markdown => format_markdown(files),
        PackStyle::Xml => format_xml(files),
    }
}

fn format_markdown(files: &[CollectedFile]) -> String {
    let mut output = String::new();
    for file in files {
        let path = file.path.display();
        let lang = detect_language(&file.path);
        output.push_str(&format!(
            "### `{path}`\n\n```{lang}\n{content}\n```\n\n",
            lang = lang.fence_label(),
            content = file.content
        ));
    }
    output.truncate(output.trim_end().len());
    output.push('\n');
    output
}

fn format_xml(files: &[CollectedFile]) -> String {
    let mut output = String::from("<files>\n");
    for file in files {
        let path = file.path.display();
        let escaped = xml_escape(&file.content);
        output.push_str(&format!("<file path=\"{path}\">\n{escaped}\n</file>\n"));
    }
    output.push_str("</files>\n");
    output
}

/// Programming or markup language inferred from a file's extension.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Language {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Jsx,
    Tsx,
    Ruby,
    Go,
    Java,
    Kotlin,
    C,
    Cpp,
    CSharp,
    Swift,
    Bash,
    Zsh,
    Fish,
    PowerShell,
    Yaml,
    Toml,
    Json,
    Xml,
    Html,
    Css,
    Scss,
    Less,
    Markdown,
    Sql,
    Lua,
    R,
    Elixir,
    Erlang,
    Haskell,
    OCaml,
    Php,
    Perl,
    Dockerfile,
    Hcl,
    Protobuf,
    GraphQl,
    Zig,
    Nim,
    Dart,
    Vue,
    Svelte,
    Unknown(String),
}

impl Language {
    fn from_extension(ext: &str) -> Self {
        match ext {
            "rs" => Self::Rust,
            "py" => Self::Python,
            "js" => Self::JavaScript,
            "ts" => Self::TypeScript,
            "jsx" => Self::Jsx,
            "tsx" => Self::Tsx,
            "rb" => Self::Ruby,
            "go" => Self::Go,
            "java" => Self::Java,
            "kt" | "kts" => Self::Kotlin,
            "c" => Self::C,
            "cpp" | "cc" | "cxx" | "h" | "hpp" => Self::Cpp,
            "cs" => Self::CSharp,
            "swift" => Self::Swift,
            "sh" | "bash" => Self::Bash,
            "zsh" => Self::Zsh,
            "fish" => Self::Fish,
            "ps1" => Self::PowerShell,
            "yaml" | "yml" => Self::Yaml,
            "toml" => Self::Toml,
            "json" => Self::Json,
            "xml" => Self::Xml,
            "html" | "htm" => Self::Html,
            "css" => Self::Css,
            "scss" | "sass" => Self::Scss,
            "less" => Self::Less,
            "md" | "markdown" => Self::Markdown,
            "sql" => Self::Sql,
            "lua" => Self::Lua,
            "r" => Self::R,
            "ex" | "exs" => Self::Elixir,
            "erl" => Self::Erlang,
            "hs" => Self::Haskell,
            "ml" | "mli" => Self::OCaml,
            "php" => Self::Php,
            "pl" | "pm" => Self::Perl,
            "dockerfile" => Self::Dockerfile,
            "tf" => Self::Hcl,
            "proto" => Self::Protobuf,
            "graphql" | "gql" => Self::GraphQl,
            "zig" => Self::Zig,
            "nim" => Self::Nim,
            "dart" => Self::Dart,
            "vue" => Self::Vue,
            "svelte" => Self::Svelte,
            other => Self::Unknown(other.to_string()),
        }
    }

    fn fence_label(&self) -> &str {
        match self {
            Self::Rust => "rust",
            Self::Python => "python",
            Self::JavaScript => "javascript",
            Self::TypeScript => "typescript",
            Self::Jsx => "jsx",
            Self::Tsx => "tsx",
            Self::Ruby => "ruby",
            Self::Go => "go",
            Self::Java => "java",
            Self::Kotlin => "kotlin",
            Self::C => "c",
            Self::Cpp => "cpp",
            Self::CSharp => "csharp",
            Self::Swift => "swift",
            Self::Bash => "bash",
            Self::Zsh => "zsh",
            Self::Fish => "fish",
            Self::PowerShell => "powershell",
            Self::Yaml => "yaml",
            Self::Toml => "toml",
            Self::Json => "json",
            Self::Xml => "xml",
            Self::Html => "html",
            Self::Css => "css",
            Self::Scss => "scss",
            Self::Less => "less",
            Self::Markdown => "markdown",
            Self::Sql => "sql",
            Self::Lua => "lua",
            Self::R => "r",
            Self::Elixir => "elixir",
            Self::Erlang => "erlang",
            Self::Haskell => "haskell",
            Self::OCaml => "ocaml",
            Self::Php => "php",
            Self::Perl => "perl",
            Self::Dockerfile => "dockerfile",
            Self::Hcl => "hcl",
            Self::Protobuf => "protobuf",
            Self::GraphQl => "graphql",
            Self::Zig => "zig",
            Self::Nim => "nim",
            Self::Dart => "dart",
            Self::Vue => "vue",
            Self::Svelte => "svelte",
            Self::Unknown(ext) => ext.as_str(),
        }
    }
}

/// Detect programming language from file extension for code fence annotation.
fn detect_language(path: &std::path::Path) -> Language {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(Language::from_extension)
        .unwrap_or_else(|| Language::Unknown(String::new()))
}

/// Minimal XML escaping for content inside tags.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_file(path: &str, content: &str) -> CollectedFile {
        CollectedFile {
            path: PathBuf::from(path),
            content: content.to_string(),
        }
    }

    #[test]
    fn markdown_single_file() {
        let files = vec![make_file("src/main.rs", "fn main() {}")];
        let result = format_files(&files, &PackStyle::Markdown);
        assert!(result.contains("### `src/main.rs`"));
        assert!(result.contains("```rust"));
        assert!(result.contains("fn main() {}"));
        assert!(result.contains("```"));
    }

    #[test]
    fn markdown_multiple_files() {
        let files = vec![
            make_file("src/main.rs", "fn main() {}"),
            make_file("config.yaml", "key: value"),
        ];
        let result = format_files(&files, &PackStyle::Markdown);
        assert!(result.contains("### `src/main.rs`"));
        assert!(result.contains("```rust"));
        assert!(result.contains("### `config.yaml`"));
        assert!(result.contains("```yaml"));
    }

    #[test]
    fn xml_single_file() {
        let files = vec![make_file("src/main.rs", "fn main() {}")];
        let result = format_files(&files, &PackStyle::Xml);
        assert!(result.contains("<files>"));
        assert!(result.contains("<file path=\"src/main.rs\">"));
        assert!(result.contains("fn main() {}"));
        assert!(result.contains("</file>"));
        assert!(result.contains("</files>"));
    }

    #[test]
    fn xml_escapes_special_chars() {
        let files = vec![make_file("test.rs", "if a < b && c > d {}")];
        let result = format_files(&files, &PackStyle::Xml);
        assert!(result.contains("&lt;"));
        assert!(result.contains("&amp;"));
        assert!(result.contains("&gt;"));
    }

    #[test]
    fn empty_file_content() {
        let files = vec![make_file("empty.txt", "")];

        let md = format_files(&files, &PackStyle::Markdown);
        assert!(md.contains("### `empty.txt`"));

        let xml = format_files(&files, &PackStyle::Xml);
        assert!(xml.contains("<file path=\"empty.txt\">"));
    }

    #[test]
    fn file_without_extension() {
        let files = vec![make_file("Makefile", "all: build")];
        let result = format_files(&files, &PackStyle::Markdown);
        // No language annotation -- empty string after ```
        assert!(result.contains("```\n"));
        assert!(result.contains("all: build"));
    }

    #[test]
    fn language_detection() {
        assert_eq!(detect_language(&PathBuf::from("main.rs")), Language::Rust);
        assert_eq!(detect_language(&PathBuf::from("app.py")), Language::Python);
        assert_eq!(detect_language(&PathBuf::from("index.tsx")), Language::Tsx);
        assert_eq!(
            detect_language(&PathBuf::from("config.toml")),
            Language::Toml
        );
        assert_eq!(
            detect_language(&PathBuf::from("Makefile")),
            Language::Unknown(String::new())
        );
        assert_eq!(
            detect_language(&PathBuf::from("mystery.xyz")),
            Language::Unknown("xyz".to_string())
        );
    }

    #[test]
    fn unknown_language_preserves_extension_in_fence_label() {
        assert_eq!(Language::Unknown(String::new()).fence_label(), "");
        assert_eq!(Language::Unknown("xyz".to_string()).fence_label(), "xyz");
    }

    #[test]
    fn empty_files_list() {
        let md = format_files(&[], &PackStyle::Markdown);
        assert_eq!(md, "\n");

        let xml = format_files(&[], &PackStyle::Xml);
        assert!(xml.contains("<files>"));
        assert!(xml.contains("</files>"));
    }
}
