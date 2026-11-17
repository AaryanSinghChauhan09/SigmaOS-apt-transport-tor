//! SigmaOS-apt-transport-tor — Tor/onion acquisition method for APT.
//!
//! Inspired by Debian's `apt-transport-tor`: lets apt fetch packages over
//! Tor via `.onion` mirrors or a Tor socks proxy. This crate models the
//! transport — recognising `tor://`/`.onion` sources, validating v3 onion
//! addresses (56-char base32 `.onion`), and reporting the method descriptor
//! (proxy required, no direct connections).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// Errors raised by this component.
#[derive(Debug, thiserror::Error)]
pub enum ComponentError {
    /// The onion address was not a valid v3 `.onion` name.
    #[error("invalid onion address: {0}")]
    InvalidOnion(String),
    /// The URI was not a tor/onion source.
    #[error("not a tor source: {0}")]
    NotTor(String),
}

/// Result alias for component operations.
pub type ComponentResult<T> = Result<T, ComponentError>;

/// A parsed Tor/onion source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TorSource {
    /// The `.onion` hostname (lowercase, no port).
    pub onion: String,
    /// Whether this is a v3 (56-char) onion address.
    pub is_v3: bool,
    /// Port (default 80 for http-over-tor, 443 for https).
    pub port: u16,
    /// Path component.
    pub path: String,
    /// Whether TLS is used (https over tor).
    pub tls: bool,
}

/// The Tor transport method descriptor.
#[derive(Debug, Clone, Copy)]
pub struct TorMethod;

impl TorMethod {
    /// Schemes this method handles.
    pub const SCHEMES: &'static [&'static str] = &["tor", "onion"];
    /// Whether a Tor proxy is required (always).
    pub const REQUIRES_PROXY: bool = true;
    /// v3 onion address length (base32, no dot).
    pub const V3_LEN: usize = 56;

    /// Whether `host` looks like an onion address (`.onion` suffix).
    pub fn is_onion_host(host: &str) -> bool {
        host.to_ascii_lowercase().ends_with(".onion")
    }

    /// Validate a v3 onion address body (the part before `.onion`).
    /// v3 onions are 56 chars of base32 (a-z2-7).
    pub fn valid_v3_onion(body: &str) -> bool {
        body.len() == Self::V3_LEN
            && body.chars().all(|c| matches!(c, 'a'..='z' | '2'..='7'))
    }

    /// Parse a `tor://onion.address[:port]/path` or `http(s)://*.onion/...` URI.
    pub fn parse(uri: &str) -> ComponentResult<TorSource> {
        let uri = uri.trim();
        // Determine scheme.
        let (tls, rest) = if let Some(r) = uri.strip_prefix("https://") {
            (true, r)
        } else if let Some(r) = uri.strip_prefix("http://") {
            (false, r)
        } else if let Some(r) = uri.strip_prefix("tor://") {
            (false, r)
        } else {
            return Err(ComponentError::NotTor(uri.to_string()));
        };
        let (authority, path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, "/"),
        };
        let (hostport, _userinfo) = match authority.rsplit_once('@') {
            Some((u, h)) => (h, Some(u)),
            None => (authority, None),
        };
        let (host, port) = match hostport.rsplit_once(':') {
            Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) => {
                (h.to_string(), p.parse().unwrap_or(if tls { 443 } else { 80 }))
            }
            _ => (hostport.to_string(), if tls { 443 } else { 80 }),
        };
        let host = host.to_ascii_lowercase();
        if !Self::is_onion_host(&host) {
            return Err(ComponentError::InvalidOnion(host));
        }
        let body = host.trim_end_matches(".onion");
        let is_v3 = Self::valid_v3_onion(body);
        Ok(TorSource { onion: host, is_v3, port, path: path.to_string(), tls })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const V3: &str = "duckduckgogg42xjoc72x3sjasowoarfbgcmvfimaftt6twagswzczad.onion";

    #[test]
    fn recognizes_onion_host() {
        assert!(TorMethod::is_onion_host("x.onion"));
        assert!(TorMethod::is_onion_host("A.ONION"));
        assert!(!TorMethod::is_onion_host("example.com"));
    }

    #[test]
    fn v3_validation() {
        assert!(TorMethod::valid_v3_onion(&V3.trim_end_matches(".onion")));
        assert!(!TorMethod::valid_v3_onion("short"));
        assert!(!TorMethod::valid_v3_onion(&"a".repeat(55)));
        assert!(!TorMethod::valid_v3_onion(&format!("{}0", "a".repeat(55)))); // 0 not in base32
    }

    #[test]
    fn parses_tor_uri() {
        let s = TorMethod::parse(&format!("tor://{}/debian", V3)).unwrap();
        assert_eq!(s.onion, V3);
        assert!(!s.tls);
        assert_eq!(s.port, 80);
        assert_eq!(s.path, "/debian");
        assert!(s.is_v3);
    }

    #[test]
    fn parses_https_onion() {
        let s = TorMethod::parse(&format!("https://{}:8443/x", V3)).unwrap();
        assert!(s.tls);
        assert_eq!(s.port, 8443);
    }

    #[test]
    fn rejects_non_onion() {
        assert!(matches!(TorMethod::parse("https://example.com/x"), Err(ComponentError::InvalidOnion(_))));
        assert!(matches!(TorMethod::parse("ftp://x.onion/"), Err(ComponentError::NotTor(_))));
    }
}
