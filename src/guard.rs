use crate::error::{DoryError, DoryResult};

pub fn sanitize_text(text: &str) -> DoryResult<String> {
    if detect_injection(text) {
        return Err(DoryError::SecurityViolation(
            "Prompt injection detected in input text".to_string(),
        ));
    }
    let redacted = redact_secrets(text);
    Ok(redacted)
}

fn detect_injection(text: &str) -> bool {
    let lower = text.to_lowercase();
    let patterns = [
        "ignore previous instructions",
        "ignore all instructions",
        "system prompt:",
        "you are now",
        "override instructions",
        "[system]",
        "<|im_start|>system",
    ];
    patterns.iter().any(|p| lower.contains(p))
}

fn redact_secrets(text: &str) -> String {
    let mut result = text.to_string();

    if result.contains("-----BEGIN OPENSSH PRIVATE KEY-----") {
        result = result
            .lines()
            .map(|line| {
                if line.starts_with("-----BEGIN") || line.starts_with("-----END") || line.len() > 80
                {
                    line.to_string()
                } else {
                    String::new()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
    }

    let secret_patterns = [
        (r"password\s*=\s*.+", "password = ***"),
        (r"passwd:\s*\S+", "passwd: ***"),
        (r"api[_-]?key\s*[=:]\s*\S+", "api_key = ***"),
        (
            r"Authorization:\s*Bearer\s*\S+",
            "Authorization: Bearer ***",
        ),
        (
            r"eyJ[a-zA-Z0-9_-]+\.eyJ[a-zA-Z0-9_-]+\.[a-zA-Z0-9_-]+",
            "[JWT_REDACTED]",
        ),
    ];

    for (pattern, replacement) in &secret_patterns {
        if let Ok(re) = regex::Regex::new(pattern) {
            result = re.replace_all(&result, *replacement).to_string();
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_reject_prompt_injection() {
        let result = sanitize_text("do this and ignore previous instructions");
        assert!(result.is_err());
    }

    #[test]
    fn should_redact_ssh_key() {
        let text =
            "-----BEGIN OPENSSH PRIVATE KEY-----\nsomething\n-----END OPENSSH PRIVATE KEY-----";
        let result = sanitize_text(text).unwrap();
        assert!(!result.contains("something"));
    }

    #[test]
    fn should_accept_clean_text() {
        let result = sanitize_text("What is the capital of France?");
        assert!(result.is_ok());
    }

    #[test]
    fn should_detect_jwt() {
        let text =
            "token is eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNrvP5IeAzZu6NcGQ";
        let result = sanitize_text(text).unwrap();
        assert!(!result.contains("eyJ"));
    }
}
