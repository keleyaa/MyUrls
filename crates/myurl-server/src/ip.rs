use std::{
    fmt::Write,
    net::{IpAddr, SocketAddr},
};

use hmac::{Hmac, Mac};
use ipnet::IpNet;
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Parses an IP address after trimming whitespace and balanced address brackets.
pub fn parse_ip_address(value: &str) -> Option<IpAddr> {
    let candidate = strip_brackets(value.trim());
    candidate.parse::<IpAddr>().ok().map(normalize_ip_address)
}

/// Returns the standard textual representation of a parsed IP address.
pub fn canonicalize_ip(value: &str) -> Option<String> {
    parse_ip_address(value).map(|address| address.to_string())
}

/// Parses a CIDR without propagating untrusted configuration input as a panic.
pub fn parse_cidr(value: &str) -> Option<IpNet> {
    value.trim().parse::<IpNet>().ok().map(normalize_ip_network)
}

/// Reports whether a valid address is in at least one trusted CIDR.
pub fn is_ip_in_cidrs(value: &str, cidrs: &[IpNet]) -> bool {
    parse_ip_address(value).is_some_and(|address| is_address_in_cidrs(address, cidrs))
}

/// Resolves the client address from the direct peer and optional forwarding headers.
///
/// Forwarding headers are considered only when the direct peer is trusted. The
/// selected forwarding chain is consumed right to left until it reaches an
/// untrusted hop or an invalid candidate.
pub fn get_client_ip(
    remote_address: Option<&str>,
    x_forwarded_for: Option<&str>,
    forwarded: Option<&str>,
    trusted_proxy_cidrs: &[IpNet],
) -> String {
    let Some(direct) = remote_address.and_then(parse_remote_ip) else {
        return "unknown".to_owned();
    };
    if !is_address_in_cidrs(direct, trusted_proxy_cidrs) {
        return direct.to_string();
    }

    let mut current = direct;
    for candidate in forwarded_chain(x_forwarded_for, forwarded)
        .into_iter()
        .rev()
    {
        if !is_address_in_cidrs(current, trusted_proxy_cidrs) {
            break;
        }
        let Some(candidate) = parse_forwarded_candidate(&candidate) else {
            break;
        };
        current = candidate;
    }

    current.to_string()
}

/// Computes a stable lowercase SHA-256 HMAC for a canonical client address.
pub fn fingerprint_ip(secret: &[u8], client_ip: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts keys of any length");
    mac.update(client_ip.as_bytes());

    let mut fingerprint = String::with_capacity(64);
    for byte in mac.finalize().into_bytes() {
        write!(&mut fingerprint, "{byte:02x}").expect("writing to a String cannot fail");
    }
    fingerprint
}

fn strip_brackets(value: &str) -> &str {
    value
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(value)
}

fn normalize_ip_address(address: IpAddr) -> IpAddr {
    match address {
        IpAddr::V6(address) => address
            .to_ipv4_mapped()
            .map_or(IpAddr::V6(address), IpAddr::V4),
        IpAddr::V4(address) => IpAddr::V4(address),
    }
}

fn normalize_ip_network(network: IpNet) -> IpNet {
    match network {
        IpNet::V6(network) if network.prefix_len() >= 96 => {
            let prefix_length = network.prefix_len() - 96;
            network
                .addr()
                .to_ipv4_mapped()
                .and_then(|address| IpNet::new(IpAddr::V4(address), prefix_length).ok())
                .unwrap_or(IpNet::V6(network))
        }
        network => network,
    }
}

fn parse_remote_ip(value: &str) -> Option<IpAddr> {
    let candidate = value.trim();
    candidate
        .parse::<SocketAddr>()
        .ok()
        .map(|address| normalize_ip_address(address.ip()))
        .or_else(|| parse_ip_address(candidate))
}

fn is_address_in_cidrs(address: IpAddr, cidrs: &[IpNet]) -> bool {
    cidrs.iter().any(|cidr| cidr.contains(&address))
}

fn forwarded_chain(x_forwarded_for: Option<&str>, forwarded: Option<&str>) -> Vec<String> {
    if let Some(x_forwarded_for) = x_forwarded_for {
        return x_forwarded_for
            .split(',')
            .map(|value| value.trim().to_owned())
            .collect();
    }

    forwarded.map_or_else(Vec::new, parse_forwarded_values)
}

fn parse_forwarded_values(value: &str) -> Vec<String> {
    let mut values = Vec::new();
    for element in value.split(',') {
        for parameter in element.split(';') {
            let Some((name, candidate)) = parameter.trim().split_once('=') else {
                continue;
            };
            if !name.eq_ignore_ascii_case("for") {
                continue;
            }
            values.push(strip_quotes(candidate.trim()).to_owned());
        }
    }
    values
}

fn strip_quotes(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(value)
}

fn parse_forwarded_candidate(value: &str) -> Option<IpAddr> {
    let candidate = value.trim();
    if candidate.eq_ignore_ascii_case("unknown") || candidate.eq_ignore_ascii_case("_hidden") {
        return None;
    }
    parse_ip_address(candidate)
}

#[cfg(test)]
mod tests {
    use ipnet::IpNet;

    use super::{
        canonicalize_ip, fingerprint_ip, get_client_ip, is_ip_in_cidrs, parse_cidr,
        parse_ip_address,
    };

    fn trusted_cidrs(values: &[&str]) -> Vec<IpNet> {
        values
            .iter()
            .map(|value| parse_cidr(value).expect("test CIDR is valid"))
            .collect()
    }

    #[test]
    fn parses_bracketed_addresses_and_normalizes_mapped_ipv4() {
        assert_eq!(
            canonicalize_ip(" [2001:0db8::1] "),
            Some("2001:db8::1".to_owned())
        );
        assert_eq!(
            canonicalize_ip("[::ffff:192.0.2.7]"),
            Some("192.0.2.7".to_owned())
        );
        assert_eq!(parse_ip_address("[not-an-ip]"), None);
    }

    #[test]
    fn parses_cidrs_safely_and_tests_membership() {
        let trusted = trusted_cidrs(&["10.0.0.0/8", "2001:db8::/32"]);

        assert!(is_ip_in_cidrs("10.0.0.8", &trusted));
        assert!(is_ip_in_cidrs("[2001:db8::8]", &trusted));
        assert!(!is_ip_in_cidrs("198.51.100.8", &trusted));
        assert!(!is_ip_in_cidrs("not-an-ip", &trusted));
        assert_eq!(parse_cidr("not-a-cidr"), None);
        assert_eq!(parse_cidr("10.0.0.0/33"), None);
        assert_eq!(parse_cidr("::ffff:10.0.0.0/104"), parse_cidr("10.0.0.0/8"));
    }

    #[test]
    fn ignores_spoofed_forwarded_headers_from_untrusted_peers() {
        let trusted = trusted_cidrs(&["10.0.0.0/8"]);

        assert_eq!(
            get_client_ip(
                Some("192.168.1.8"),
                Some("198.51.100.4"),
                Some("for=198.51.100.5"),
                &trusted,
            ),
            "192.168.1.8"
        );
    }

    #[test]
    fn consumes_x_forwarded_for_from_right_to_left_through_trusted_proxies() {
        let trusted = trusted_cidrs(&["10.0.0.0/8"]);

        assert_eq!(
            get_client_ip(
                Some("10.0.0.8:443"),
                Some("198.51.100.4, 10.0.0.7"),
                None,
                &trusted,
            ),
            "198.51.100.4"
        );
        assert_eq!(
            get_client_ip(
                Some("10.0.0.8"),
                Some("198.51.100.4, 192.168.1.9"),
                None,
                &trusted,
            ),
            "192.168.1.9"
        );
    }

    #[test]
    fn prefers_x_forwarded_for_and_falls_back_to_forwarded() {
        let trusted = trusted_cidrs(&["127.0.0.0/8", "2001:db8::/32"]);

        assert_eq!(
            get_client_ip(
                Some("[::ffff:127.0.0.1]"),
                Some("[2001:db8::7]"),
                Some("for=198.51.100.7"),
                &trusted,
            ),
            "2001:db8::7"
        );
        assert_eq!(
            get_client_ip(
                Some("127.0.0.1"),
                None,
                Some("for=\"[2001:db8::8]\";proto=https"),
                &trusted,
            ),
            "2001:db8::8"
        );
    }

    #[test]
    fn stops_at_unknown_hidden_or_invalid_forwarded_candidates() {
        let trusted = trusted_cidrs(&["10.0.0.0/8"]);

        for candidate in ["unknown", "_hidden", "not-an-ip", ""] {
            assert_eq!(
                get_client_ip(
                    Some("10.0.0.8"),
                    Some(&format!("198.51.100.4, {candidate}")),
                    None,
                    &trusted,
                ),
                "10.0.0.8",
                "candidate {candidate:?} must stop traversal"
            );
        }
        assert_eq!(
            get_client_ip(
                Some("10.0.0.8"),
                None,
                Some("for=198.51.100.4, for=unknown"),
                &trusted,
            ),
            "10.0.0.8"
        );
    }

    #[test]
    fn returns_unknown_for_missing_or_invalid_direct_peers() {
        let trusted = trusted_cidrs(&["10.0.0.0/8"]);

        for remote_address in [None, Some("not-an-address"), Some("[::1")] {
            assert_eq!(
                get_client_ip(remote_address, Some("198.51.100.4"), None, &trusted),
                "unknown"
            );
        }
    }

    #[test]
    fn creates_a_fixed_length_hmac_fingerprint() {
        let fingerprint = fingerprint_ip(b"secret", "203.0.113.7");

        assert_eq!(
            fingerprint,
            "d1eda5f85436ad2d5e25828b7bc77ca290d6c797a9ab2479327aec37fa3e09d3"
        );
        assert_eq!(fingerprint.len(), 64);
        assert!(fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_eq!(fingerprint, fingerprint.to_ascii_lowercase());
        assert!(!fingerprint.contains("203.0.113.7"));
        assert_ne!(
            fingerprint,
            fingerprint_ip(b"different-secret", "203.0.113.7")
        );
    }
}
