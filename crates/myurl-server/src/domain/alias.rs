use crate::error::DomainError;

const RESERVED_CODES: [&str; 6] = [
    "api",
    "assets",
    "health",
    "favicon.ico",
    "robots.txt",
    "sitemap.xml",
];

pub fn normalize_alias(input: &str) -> Result<String, DomainError> {
    let trimmed = input.trim();
    if !trimmed.is_ascii() {
        return Err(DomainError::AliasInvalid);
    }

    let normalized = trimmed.to_ascii_lowercase();
    if !(4..=32).contains(&normalized.len())
        || !normalized.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
    {
        return Err(DomainError::AliasInvalid);
    }

    Ok(normalized)
}

pub fn is_reserved_code(input: &str) -> bool {
    RESERVED_CODES
        .iter()
        .any(|reserved| input.eq_ignore_ascii_case(reserved))
}

#[cfg(test)]
mod tests {
    use crate::error::DomainError;

    use super::{is_reserved_code, normalize_alias};

    #[test]
    fn trims_and_lowercases_an_ascii_alias() {
        assert_eq!(normalize_alias("  Launch_42  "), Ok("launch_42".to_owned()));
    }

    #[test]
    fn rejects_invalid_aliases_from_the_node_policy_tests() {
        for alias in [
            "abc",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "hello.world",
            "hello world",
            "аlias",
            "Kaunch",
            "foo/bar",
            "foo%2Fbar",
        ] {
            assert_eq!(
                normalize_alias(alias),
                Err(DomainError::AliasInvalid),
                "expected {alias:?} to be rejected"
            );
        }
    }

    #[test]
    fn recognizes_every_reserved_path_without_locale_sensitive_case_mapping() {
        for reserved in [
            "api",
            "assets",
            "health",
            "favicon.ico",
            "robots.txt",
            "sitemap.xml",
        ] {
            assert!(is_reserved_code(&reserved.to_ascii_uppercase()));
        }

        assert!(!is_reserved_code("launch"));
        assert!(!is_reserved_code("APİ"));
    }
}
