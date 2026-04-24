/// Scan content for potential secrets (API keys, tokens, private keys, etc.)
/// Returns a list of human-readable descriptions for each detected secret.
///
/// Implementation note: each line is lowercased *once* (ASCII fast-path) and
/// the result is threaded through every detector. Detectors that need the
/// original casing (AWS AKIA, GitHub `ghp_` prefixes) receive both views.
/// Previously each detector called `line.to_lowercase()` itself, which
/// allocated `O(detectors) = 12` copies per line -- noticeable on large packs.
pub fn scan_for_secrets(content: &str) -> Vec<String> {
    let mut findings = Vec::new();
    let mut lower = String::new();

    for line in content.lines() {
        // ASCII-lowercase is enough for the tokens we detect (all English,
        // all ASCII), and it sidesteps the Unicode table lookups that
        // `str::to_lowercase` does per char.
        lower.clear();
        lower.push_str(line);
        lower.make_ascii_lowercase();

        for (pattern_fn, description) in DETECTORS {
            if pattern_fn(line, &lower) {
                findings.push(description.to_string());
            }
        }
    }

    findings
}

type Detector = (fn(&str, &str) -> bool, &'static str);

const DETECTORS: &[Detector] = &[
    (detect_aws_access_key, "AWS access key (AKIA...)"),
    (detect_aws_secret_key, "AWS secret access key"),
    (
        detect_github_token,
        "GitHub token (ghp_/gho_/ghs_/ghr_/github_pat_)",
    ),
    (detect_openai_key, "OpenAI API key (sk-)"),
    (detect_anthropic_key, "Anthropic API key (sk-ant-)"),
    (detect_private_key_header, "Private key (PEM header)"),
    (detect_generic_api_key, "Generic API key assignment"),
    (detect_generic_secret, "Generic secret assignment"),
    (detect_password_assignment, "Password assignment"),
    (detect_bearer_token, "Bearer token"),
    (detect_basic_auth, "Basic auth credentials"),
    (
        detect_connection_string,
        "Database connection string with credentials",
    ),
];

fn detect_aws_access_key(line: &str, _lower: &str) -> bool {
    // AWS access key IDs start with AKIA (case-sensitive) and are 20 chars
    let Some(pos) = line.find("AKIA") else {
        return false;
    };
    let after = &line[pos..];
    after.len() >= 20 && after[..20].chars().all(|c| c.is_ascii_alphanumeric())
}

fn detect_aws_secret_key(line: &str, lower: &str) -> bool {
    (lower.contains("aws_secret_access_key") || lower.contains("aws_secret_key"))
        && contains_value_assignment(line)
}

fn detect_github_token(line: &str, _lower: &str) -> bool {
    // Prefixes are case-sensitive; never lowered upstream.
    line.contains("ghp_")
        || line.contains("gho_")
        || line.contains("ghs_")
        || line.contains("ghr_")
        || line.contains("github_pat_")
}

fn detect_openai_key(line: &str, _lower: &str) -> bool {
    // sk- followed by enough alphanumeric chars, but not sk-ant-
    if let Some(pos) = line.find("sk-") {
        let after = &line[pos..];
        !after.starts_with("sk-ant-") && after.len() >= 20
    } else {
        false
    }
}

fn detect_anthropic_key(line: &str, _lower: &str) -> bool {
    line.contains("sk-ant-")
}

fn detect_private_key_header(line: &str, _lower: &str) -> bool {
    line.contains("-----BEGIN") && line.contains("PRIVATE KEY-----")
}

fn detect_generic_api_key(line: &str, lower: &str) -> bool {
    (lower.contains("api_key") || lower.contains("apikey") || lower.contains("api-key"))
        && contains_value_assignment(line)
        && !is_likely_placeholder(lower)
}

fn detect_generic_secret(line: &str, lower: &str) -> bool {
    (lower.contains("secret_key") || lower.contains("secret-key") || lower.contains("secretkey"))
        && contains_value_assignment(line)
        && !is_likely_placeholder(lower)
}

fn detect_password_assignment(_line: &str, lower: &str) -> bool {
    (lower.contains("password=") || lower.contains("password =") || lower.contains("password:"))
        && !is_likely_placeholder(lower)
}

fn detect_bearer_token(line: &str, _lower: &str) -> bool {
    line.contains("Bearer ") && {
        let after = line.split("Bearer ").nth(1).unwrap_or("");
        // Only lowercase the trailing slice, not the whole line, to check
        // for placeholder markers like "YOUR_TOKEN".
        after.len() >= 20 && !is_likely_placeholder(&after.to_ascii_lowercase())
    }
}

fn detect_basic_auth(line: &str, _lower: &str) -> bool {
    // URLs with embedded credentials: ://user:pass@
    line.contains("://") && line.contains('@') && {
        if let Some(proto_end) = line.find("://") {
            let after_proto = &line[proto_end + 3..];
            after_proto.contains(':')
                && after_proto.contains('@')
                && after_proto.find(':') < after_proto.find('@')
        } else {
            false
        }
    }
}

fn detect_connection_string(_line: &str, lower: &str) -> bool {
    (lower.contains("postgres://")
        || lower.contains("mysql://")
        || lower.contains("mongodb://")
        || lower.contains("redis://"))
        && lower.contains('@')
}

/// Check if a line contains a value assignment (=, :, or after quotes).
fn contains_value_assignment(line: &str) -> bool {
    line.contains('=') || line.contains(':')
}

/// Check if the value is likely a placeholder, not a real secret.
fn is_likely_placeholder(lower: &str) -> bool {
    lower.contains("your_")
        || lower.contains("xxx")
        || lower.contains("<your")
        || lower.contains("example")
        || lower.contains("placeholder")
        || lower.contains("changeme")
        || lower.contains("todo")
        || lower.contains("fixme")
        || lower.contains("${")
        || lower.contains("{{")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_aws_access_key_id() {
        let content = "AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE";
        let findings = scan_for_secrets(content);
        assert!(findings.iter().any(|f| f.contains("AWS access key")));
    }

    #[test]
    fn detect_aws_secret() {
        let content = "aws_secret_access_key = wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";
        let findings = scan_for_secrets(content);
        assert!(findings.iter().any(|f| f.contains("AWS secret")));
    }

    #[test]
    fn detect_github_pat() {
        let content = "GITHUB_TOKEN=ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";
        let findings = scan_for_secrets(content);
        assert!(findings.iter().any(|f| f.contains("GitHub")));
    }

    #[test]
    fn detect_github_pat_variants() {
        for prefix in &["gho_", "ghs_", "ghr_", "github_pat_"] {
            let content = format!("TOKEN={prefix}xxxxxxxxxxxxxxxxxxxx");
            let findings = scan_for_secrets(&content);
            assert!(
                findings.iter().any(|f| f.contains("GitHub")),
                "should detect {prefix}"
            );
        }
    }

    #[test]
    fn detect_openai_api_key() {
        let content = "OPENAI_API_KEY=sk-proj-xxxxxxxxxxxxxxxxxxxxxxxxxxxx";
        let findings = scan_for_secrets(content);
        assert!(findings.iter().any(|f| f.contains("OpenAI")));
    }

    #[test]
    fn detect_anthropic_api_key() {
        let content = "ANTHROPIC_API_KEY=sk-ant-xxxxxxxxxxxxxxxxxxxx";
        let findings = scan_for_secrets(content);
        assert!(findings.iter().any(|f| f.contains("Anthropic")));
    }

    #[test]
    fn detect_rsa_private_key() {
        let content = "-----BEGIN RSA PRIVATE KEY-----\nMIIEpAIBAAK...";
        let findings = scan_for_secrets(content);
        assert!(findings.iter().any(|f| f.contains("Private key")));
    }

    #[test]
    fn detect_ec_private_key() {
        let content = "-----BEGIN EC PRIVATE KEY-----";
        let findings = scan_for_secrets(content);
        assert!(findings.iter().any(|f| f.contains("Private key")));
    }

    #[test]
    fn detect_generic_api_key_assignment() {
        let content = "api_key = \"abc123def456ghi789jkl012\"";
        let findings = scan_for_secrets(content);
        assert!(findings.iter().any(|f| f.contains("API key")));
    }

    #[test]
    fn detect_password_in_env() {
        let content = "DATABASE_PASSWORD=supersecretpassword123";
        let findings = scan_for_secrets(content);
        assert!(findings.iter().any(|f| f.contains("Password")));
    }

    #[test]
    fn detect_bearer_token() {
        let content = r#"Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.abcdefg"#;
        let findings = scan_for_secrets(content);
        assert!(findings.iter().any(|f| f.contains("Bearer")));
    }

    #[test]
    fn detect_connection_string_postgres() {
        let content = "DATABASE_URL=postgres://user:pass@localhost:5432/db";
        let findings = scan_for_secrets(content);
        assert!(findings.iter().any(|f| f.contains("connection string")));
    }

    #[test]
    fn no_false_positive_on_placeholder() {
        let content = "api_key = your_api_key_here";
        let findings = scan_for_secrets(content);
        assert!(
            !findings.iter().any(|f| f.contains("API key")),
            "should not flag placeholders"
        );
    }

    #[test]
    fn no_false_positive_on_comment() {
        let content = "// Use AKIA for AWS authentication";
        let findings = scan_for_secrets(content);
        // "AKIA" alone without 20 alphanumeric chars shouldn't trigger
        assert!(findings.is_empty() || !findings.iter().any(|f| f.contains("AWS access key")));
    }

    #[test]
    fn no_false_positive_on_env_variable_reference() {
        let content = "api_key = ${API_KEY}";
        let findings = scan_for_secrets(content);
        assert!(
            !findings.iter().any(|f| f.contains("API key")),
            "should not flag env variable references"
        );
    }

    #[test]
    fn multiple_secrets_in_content() {
        let content = "AKIAIOSFODNN7EXAMPLE1\nghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\n-----BEGIN RSA PRIVATE KEY-----";
        let findings = scan_for_secrets(content);
        assert!(findings.len() >= 3, "should detect multiple secrets");
    }

    #[test]
    fn clean_content_no_secrets() {
        let content = "fn main() {\n    println!(\"Hello, world!\");\n}";
        let findings = scan_for_secrets(content);
        assert!(findings.is_empty());
    }

    #[test]
    fn detects_case_insensitive_api_key_assignment() {
        // Regression test for the shared-lowercase refactor: the uppercase
        // variant must still be flagged.
        let content = "API_KEY = \"abc123def456ghi789jkl012\"";
        let findings = scan_for_secrets(content);
        assert!(
            findings.iter().any(|f| f.contains("API key")),
            "should detect uppercase API_KEY: {findings:?}"
        );
    }

    #[test]
    fn detects_mixed_case_password() {
        let content = "Password: hunter2realsecret";
        let findings = scan_for_secrets(content);
        assert!(findings.iter().any(|f| f.contains("Password")));
    }
}
