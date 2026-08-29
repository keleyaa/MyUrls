use std::net::{Ipv4Addr, Ipv6Addr};

use ipnet::{Ipv4Net, Ipv6Net};
use unicode_general_category::{GeneralCategory, get_general_category};
use url::{Host, Url};

use crate::{config::MAX_URL_BYTES, error::DomainError};

const BLOCKED_HOST_SUFFIXES: [&str; 4] = ["localhost", "local", "internal", "home.arpa"];

pub fn normalize_target_url(input: &str) -> Result<String, DomainError> {
    if input.len() > MAX_URL_BYTES {
        return Err(DomainError::InvalidRequest);
    }
    if contains_disallowed_literal_character(input) {
        return Err(DomainError::UrlNotAllowed);
    }

    let parsed = Url::parse(input).map_err(|_| DomainError::UrlNotAllowed)?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host().is_none()
        || !parsed.username().is_empty()
        || parsed
            .password()
            .is_some_and(|password| !password.is_empty())
    {
        return Err(DomainError::UrlNotAllowed);
    }
    if parsed.host().is_some_and(is_blocked_host) {
        return Err(DomainError::UrlNotAllowed);
    }

    Ok(parsed.into())
}

fn contains_disallowed_literal_character(input: &str) -> bool {
    input.chars().any(|character| {
        character.is_whitespace()
            || character.is_control()
            || get_general_category(character) == GeneralCategory::Format
    })
}

fn is_blocked_host(host: Host<&str>) -> bool {
    match host {
        Host::Domain(hostname) => is_blocked_hostname(hostname),
        Host::Ipv4(address) => is_blocked_ipv4(address),
        Host::Ipv6(address) => is_blocked_ipv6(address),
    }
}

fn is_blocked_hostname(hostname: &str) -> bool {
    let normalized = hostname.trim_end_matches('.').to_ascii_lowercase();
    BLOCKED_HOST_SUFFIXES.iter().any(|suffix| {
        normalized == *suffix
            || normalized
                .strip_suffix(suffix)
                .is_some_and(|prefix| prefix.ends_with('.'))
    })
}

fn is_blocked_ipv4(address: Ipv4Addr) -> bool {
    [
        (Ipv4Addr::new(0, 0, 0, 0), 8),
        (Ipv4Addr::new(255, 255, 255, 255), 32),
        (Ipv4Addr::new(224, 0, 0, 0), 4),
        (Ipv4Addr::new(169, 254, 0, 0), 16),
        (Ipv4Addr::new(127, 0, 0, 0), 8),
        (Ipv4Addr::new(10, 0, 0, 0), 8),
        (Ipv4Addr::new(172, 16, 0, 0), 12),
        (Ipv4Addr::new(192, 168, 0, 0), 16),
        (Ipv4Addr::new(100, 64, 0, 0), 10),
        (Ipv4Addr::new(192, 0, 0, 0), 24),
        (Ipv4Addr::new(192, 0, 2, 0), 24),
        (Ipv4Addr::new(192, 88, 99, 0), 24),
        (Ipv4Addr::new(198, 18, 0, 0), 15),
        (Ipv4Addr::new(198, 51, 100, 0), 24),
        (Ipv4Addr::new(203, 0, 113, 0), 24),
        (Ipv4Addr::new(240, 0, 0, 0), 4),
    ]
    .into_iter()
    .any(|(network, prefix)| {
        Ipv4Net::new(network, prefix)
            .expect("blocked IPv4 network prefix is valid")
            .contains(&address)
    })
}

fn is_blocked_ipv6(address: Ipv6Addr) -> bool {
    if let Some(address) = address.to_ipv4_mapped() {
        return is_blocked_ipv4(address);
    }

    [
        (Ipv6Addr::UNSPECIFIED, 128),
        (Ipv6Addr::new(0xff00, 0, 0, 0, 0, 0, 0, 0), 8),
        (Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 0), 10),
        (Ipv6Addr::LOCALHOST, 128),
        (Ipv6Addr::new(0xfc00, 0, 0, 0, 0, 0, 0, 0), 7),
        (Ipv6Addr::new(0x2001, 0, 0, 0, 0, 0, 0, 0), 23),
        (Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 0), 32),
        (Ipv6Addr::new(0x3fff, 0, 0, 0, 0, 0, 0, 0), 20),
    ]
    .into_iter()
    .any(|(network, prefix)| {
        Ipv6Net::new(network, prefix)
            .expect("blocked IPv6 network prefix is valid")
            .contains(&address)
    })
}

#[cfg(test)]
mod tests {
    use crate::{AppError, DomainError, ErrorCode, config::MAX_URL_BYTES};

    use super::normalize_target_url;

    fn assert_not_allowed(input: &str) {
        assert_eq!(normalize_target_url(input), Err(DomainError::UrlNotAllowed));
    }

    #[test]
    fn returns_standard_url_serialization_for_valid_http_targets() {
        for (input, expected) in [
            (
                "HTTPS://Example.COM/docs?q=1#intro",
                "https://example.com/docs?q=1#intro",
            ),
            ("http://example.com/", "http://example.com/"),
        ] {
            assert_eq!(normalize_target_url(input), Ok(expected.to_owned()));
        }
    }

    #[test]
    fn rejects_empty_hosts_and_nonempty_credentials() {
        for input in [
            "https://",
            "https://user@example.com/",
            "https://:password@example.com/",
        ] {
            assert_not_allowed(input);
        }
    }

    #[test]
    fn rejects_unsafe_target_urls_from_the_node_policy_tests() {
        for input in [
            "ftp://example.com/file",
            "javascript:alert(1)",
            "https://user:password@example.com/",
            "https://localhost/admin",
            "https://api.local/status",
            "https://service.internal/status",
            "https://router.home.arpa/",
            "https://127.0.0.1/",
            "https://10.0.0.8/",
            "https://169.254.1.2/",
            "https://192.168.1.3/",
            "https://0.0.0.0/",
            "https://224.0.0.1/",
            "https://192.0.2.10/",
            "https://[::1]/",
            "https://[fc00::1]/",
            "https://[fe80::1]/",
            "https://[2001:db8::1]/",
            "https://example.com/with space",
        ] {
            assert_not_allowed(input);
        }
    }

    #[test]
    fn rejects_hostname_suffixes_case_insensitively_after_trailing_dots() {
        assert_not_allowed("https://API.LOCALHOST./");
    }

    #[test]
    fn rejects_each_blocked_ip_range_category() {
        for (category, input) in [
            ("IPv4 unspecified", "https://0.0.0.1/"),
            ("IPv4 broadcast", "https://255.255.255.255/"),
            ("IPv4 multicast", "https://224.0.0.1/"),
            ("IPv4 link-local", "https://169.254.1.2/"),
            ("IPv4 loopback", "https://127.0.0.2/"),
            ("IPv4 private 10/8", "https://10.0.0.1/"),
            ("IPv4 private 172.16/12", "https://172.16.0.1/"),
            ("IPv4 private 192.168/16", "https://192.168.0.1/"),
            ("IPv4 carrier-grade NAT", "https://100.64.0.1/"),
            ("IPv4 benchmark", "https://198.18.0.1/"),
            ("IPv4 reserved", "https://240.0.0.1/"),
            ("IPv4 documentation", "https://198.51.100.1/"),
            ("IPv6 unspecified", "https://[::]/"),
            ("IPv6 multicast", "https://[ff00::1]/"),
            ("IPv6 link-local", "https://[fe80::1]/"),
            ("IPv6 loopback", "https://[::1]/"),
            ("IPv6 unique-local", "https://[fd00::1]/"),
            ("IPv6 benchmark", "https://[2001:2::1]/"),
            ("IPv6 reserved", "https://[3fff::1]/"),
            ("IPv6 documentation", "https://[2001:db8::1]/"),
            ("IPv4-mapped IPv6 loopback", "https://[::ffff:127.0.0.1]/"),
        ] {
            assert_eq!(
                normalize_target_url(input),
                Err(DomainError::UrlNotAllowed),
                "expected {category} to be rejected"
            );
        }
    }

    #[test]
    fn accepts_percent_encoded_controls_but_rejects_literal_disallowed_characters() {
        assert_eq!(
            normalize_target_url("https://example.com/%0A"),
            Ok("https://example.com/%0A".to_owned())
        );
        assert_not_allowed("https://example.com/\n");
        assert_not_allowed("https://example.com/with\u{2003}space");
        assert_not_allowed("https://example.com/\u{200e}");
    }

    #[test]
    fn enforces_the_utf8_byte_limit_as_an_invalid_request() {
        let within_limit = format!("https://example.com/{}", "é".repeat(2_038));
        assert_eq!(within_limit.len(), MAX_URL_BYTES);
        assert!(normalize_target_url(&within_limit).is_ok());

        let oversized = format!("https://example.com/{}", "é".repeat(2_039));
        assert_eq!(oversized.len(), MAX_URL_BYTES + 2);
        assert_eq!(
            normalize_target_url(&oversized),
            Err(DomainError::InvalidRequest)
        );

        let error = AppError::from(DomainError::InvalidRequest);
        assert_eq!(error.code(), ErrorCode::InvalidRequest);
        assert_eq!(error.status_code(), 400);
    }
}
