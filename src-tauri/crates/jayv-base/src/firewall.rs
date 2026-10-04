use crate::{config::PrivacyConfig, model::Context};
use globset::{Glob, GlobSet, GlobSetBuilder};
use regex::{Captures, Regex};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilePrivacyInfo { pub path: String, pub is_sensitive: bool, pub local_only: bool, pub matched_rule: Option<String> }

pub struct ContextFirewall {
    config: PrivacyConfig,
    deny: GlobSet,
    local: GlobSet,
    deny_patterns: Vec<String>,
    local_patterns: Vec<String>,
    secrets: Vec<(Regex, &'static str)>,
    assignments: Regex,
    digits: Regex,
}

impl ContextFirewall {
    pub fn new(config: PrivacyConfig) -> Self {
        Self {
            deny: build_globs(&config.deny), local: build_globs(&config.local_only),
            deny_patterns: config.deny.clone(), local_patterns: config.local_only.clone(), config,
            secrets: vec![
                (Regex::new(r"(?i)\b[a-z0-9](?:[a-z0-9._%+-]{0,62}[a-z0-9])?@(?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z]{2,24}\b").unwrap(), "[EMAIL_REDACTED]"),
                (Regex::new(r"(?i)\bbearer\s+[a-z0-9\-._~+/]{20,}={0,2}").unwrap(), "Bearer [SECRET_REDACTED]"),
                (Regex::new(r"\b(?:sk|pk|rk)-[A-Za-z0-9_-]{16,}\b").unwrap(), "[SECRET_REDACTED]"),
                (Regex::new(r"\b(?:gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|AKIA[0-9A-Z]{16}|xox[abprs]-[A-Za-z0-9-]{10,})\b").unwrap(), "[SECRET_REDACTED]"),
                (Regex::new(r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----").unwrap(), "[PRIVATE_KEY_REDACTED]"),
            ],
            assignments: Regex::new(r#"(?i)\b(api[_-]?key|api[_-]?secret|access[_-]?token|auth[_-]?token|refresh[_-]?token|secret[_-]?key|client[_-]?secret|private[_-]?token|password|passwd|token|secret)\s*[:=]\s*(?:["']([^"'\n]{6,})["']|([A-Za-z0-9_\-./+=~]{16,}))"#).unwrap(),
            digits: Regex::new(r"\b\d{3,6}(?:[ -]\d{2,6}){1,5}\b|\b\d{13,19}\b").unwrap(),
        }
    }
    pub fn check_file(&self, path: impl AsRef<Path>) -> FilePrivacyInfo {
        let normalized = path.as_ref().to_string_lossy().replace('\\', "/");
        let file_name = path.as_ref().file_name().map(|x| x.to_string_lossy()).unwrap_or_default();
        let denied = first_match(&self.deny, &self.deny_patterns, &normalized, file_name.as_ref());
        let confined = first_match(&self.local, &self.local_patterns, &normalized, file_name.as_ref());
        let matched_rule = denied.map(|pattern| format!("privacy.deny · {pattern}")).or_else(|| confined.map(|pattern| format!("privacy.local_only · {pattern}")));
        FilePrivacyInfo { path: normalized, is_sensitive: denied.is_some(), local_only: confined.is_some(), matched_rule }
    }
    pub fn redact_secrets(&self, content: &str) -> String {
        if !self.config.redact_secrets { return content.into(); }
        let value = self.secrets.iter().fold(content.to_string(), |value, (pattern, replacement)| pattern.replace_all(&value, *replacement).into_owned());
        let value = self.assignments.replace_all(&value, |caps: &Captures| {
            let quoted = caps.get(2);
            let raw = quoted.or_else(|| caps.get(3)).map(|m| m.as_str()).unwrap_or_default();
            if is_redacted(raw) || (quoted.is_none() && !is_opaque(raw)) { caps[0].to_string() } else { format!("{}=[SECRET_REDACTED]", &caps[1]) }
        }).into_owned();
        self.digits.replace_all(&value, |caps: &Captures| if is_card_number(&caps[0]) { "[CARD_REDACTED]".to_string() } else { caps[0].to_string() }).into_owned()
    }
    pub fn filter_context(&self, context: &Context, external: bool) -> Context {
        let mut filtered = context.clone();
        filtered.relevant_files.retain(|path| { let info = self.check_file(path); !info.is_sensitive && !(external && info.local_only) });
        filtered.snippets.retain(|snippet| filtered.relevant_files.contains(&snippet.path));
        for snippet in &mut filtered.snippets { snippet.content = self.redact_secrets(&snippet.content); }
        filtered
    }
}

fn build_globs(patterns: &[String]) -> GlobSet {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns { if let Ok(glob) = Glob::new(pattern) { builder.add(glob); } }
    builder.build().expect("glob set")
}

fn first_match<'a>(globs: &GlobSet, patterns: &'a [String], path: &str, file_name: &str) -> Option<&'a str> {
    globs.matches(path).into_iter().chain(globs.matches(file_name)).min().and_then(|index| patterns.get(index)).map(String::as_str)
}

fn is_redacted(value: &str) -> bool { value.starts_with('[') && value.ends_with("_REDACTED]") }
fn is_opaque(value: &str) -> bool { value.bytes().any(|b| b.is_ascii_digit()) && value.bytes().any(|b| b.is_ascii_alphabetic()) && !value.contains("::") }
fn luhn_ok(digits: &str) -> bool { let (mut sum, mut double) = (0u32, false); for byte in digits.bytes().rev() { let mut digit = u32::from(byte - b'0'); if double { digit *= 2; if digit > 9 { digit -= 9; } } sum += digit; double = !double; } sum % 10 == 0 }
fn has_card_prefix(digits: &str) -> bool {
    let prefix = |len: usize| digits.get(..len).and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
    match (digits.len(), prefix(1)) {
        (13 | 16 | 19, 4) => true,
        (16, 5) => (51..=55).contains(&prefix(2)),
        (16, 2) => (2221..=2720).contains(&prefix(4)),
        (15, 3) => prefix(2) == 34 || prefix(2) == 37,
        (14, 3) => prefix(2) == 36 || prefix(2) == 38 || (300..=305).contains(&prefix(3)),
        (16..=19, 3) => (3528..=3589).contains(&prefix(4)),
        (16..=19, 6) => prefix(4) == 6011 || prefix(2) == 65 || prefix(2) == 62 || (644..=649).contains(&prefix(3)),
        _ => false,
    }
}
fn is_card_number(raw: &str) -> bool { let digits: String = raw.chars().filter(char::is_ascii_digit).collect(); (13..=19).contains(&digits.len()) && has_card_prefix(&digits) && luhn_ok(&digits) }

#[cfg(test)]
mod tests {
    use super::*;
    fn firewall() -> ContextFirewall { ContextFirewall::new(PrivacyConfig::default()) }
    #[test] fn protects_sensitive_files_and_values() { let f = firewall(); assert!(f.check_file(".env").is_sensitive); assert!(f.redact_secrets("Email: test@example.com").contains("[EMAIL_REDACTED]")); }
    #[test] fn names_the_rule_a_path_matched() {
        let f = firewall();
        assert_eq!(f.check_file("deploy/keys/server.pem").matched_rule.as_deref(), Some("privacy.deny · *.pem"));
        assert_eq!(f.check_file("internal/notes.md").matched_rule.as_deref(), Some("privacy.local_only · internal/**"));
        assert_eq!(f.check_file("src/router.rs").matched_rule, None);
    }
    #[test] fn keeps_numeric_literals_intact() { let f = firewall(); for code in ["let started_at = 1727270400000000;", "const MAX_ID: u64 = 9007199254740991;", "build 20240917123456", "digest 123456789012345678", "version 1.20.3 rev 987654321012345"] { assert_eq!(f.redact_secrets(code), code); } }
    #[test] fn redacts_card_numbers() { let f = firewall(); assert!(f.redact_secrets("pay with 4111111111111111 now").contains("[CARD_REDACTED]")); assert!(f.redact_secrets("pay with 4111 1111 1111 1111 now").contains("[CARD_REDACTED]")); assert!(f.redact_secrets("pay with 5500-0000-0000-0004 now").contains("[CARD_REDACTED]")); }
    #[test] fn keeps_source_declarations_intact() { let f = firewall(); for code in ["pub token: String,", "let secret = compute_secret();", "password: Option<String>", "api_key: &str", "fn refresh_token(token: &Token) -> Token"] { assert_eq!(f.redact_secrets(code), code); } }
    #[test] fn redacts_real_secret_values() { let f = firewall(); assert!(f.redact_secrets("api_key = \"sk-live-01234567890abcdef\"").contains("[SECRET_REDACTED]")); assert!(f.redact_secrets("password: \"hunter2a\"").contains("[SECRET_REDACTED]")); assert!(f.redact_secrets("AUTH_TOKEN=aB3dEf9hIj2lMn5pQr8t").contains("[SECRET_REDACTED]")); }
    #[test] fn redaction_is_idempotent() { let f = firewall(); let input = "mail test@example.com card 4111111111111111 key api_key=\"abc123def4567\" stamp 1727270400000000 auth Bearer abcdef0123456789abcdef0123456789"; let once = f.redact_secrets(input); assert_eq!(f.redact_secrets(&once), once); assert!(once.contains("1727270400000000") && once.contains("[CARD_REDACTED]") && once.contains("[EMAIL_REDACTED]") && once.contains("[SECRET_REDACTED]")); }
}
