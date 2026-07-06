//! Shared `torpedo://` deep-link parser. Platform-agnostic: Android intents,
//! iOS URL handling, and the desktop `torpedo open` subcommand all route through
//! it. The scheme carries this fork's identity (RENAME-007) so a scanned or
//! pasted link is unambiguously torpedo's, never confused with a genuine
//! rayfish `rayfish://` link on the same host.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TorpedoLink {
    Join(String),
    Pair(String),
}

/// Parses `torpedo://<verb>/<code>` where verb is `join` or `pair`. Tolerant of
/// surrounding whitespace and a single trailing slash. The code is taken verbatim
/// (not percent-decoded); invite/pairing codes are bs58 and never contain
/// reserved characters.
pub fn parse_torpedo_uri(s: &str) -> anyhow::Result<TorpedoLink> {
    let s = s.trim();
    let rest = s
        .strip_prefix("torpedo://")
        .ok_or_else(|| anyhow::anyhow!("not a torpedo:// URI"))?;
    let rest = rest.strip_suffix('/').unwrap_or(rest);
    let (verb, code) = rest
        .split_once('/')
        .ok_or_else(|| anyhow::anyhow!("missing code in {s}"))?;
    anyhow::ensure!(!code.is_empty(), "empty code in {s}");
    match verb {
        "join" => Ok(TorpedoLink::Join(code.to_string())),
        "pair" => Ok(TorpedoLink::Pair(code.to_string())),
        other => anyhow::bail!("unknown torpedo verb {other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_join_and_pair() {
        assert_eq!(
            parse_torpedo_uri("torpedo://join/ABC123").unwrap(),
            TorpedoLink::Join("ABC123".into())
        );
        assert_eq!(
            parse_torpedo_uri("torpedo://pair/XYZ789").unwrap(),
            TorpedoLink::Pair("XYZ789".into())
        );
    }

    #[test]
    fn trailing_slash_and_whitespace_tolerated() {
        assert_eq!(
            parse_torpedo_uri(" torpedo://join/CODE/ ").unwrap(),
            TorpedoLink::Join("CODE".into())
        );
    }

    #[test]
    fn rejects_bad_scheme_host_or_missing_code() {
        assert!(parse_torpedo_uri("https://join/x").is_err());
        assert!(parse_torpedo_uri("torpedo://bogus/x").is_err());
        assert!(parse_torpedo_uri("torpedo://join/").is_err());
        assert!(parse_torpedo_uri("torpedo://join").is_err());
    }
}
