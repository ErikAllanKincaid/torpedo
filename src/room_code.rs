use anyhow::{Context, Result};
use iroh::EndpointId;

pub fn encode(id: &EndpointId) -> String {
    let z32 = id.to_z32();
    let mut result = String::with_capacity(z32.len() + z32.len() / 4);
    for (i, ch) in z32.chars().enumerate() {
        if i > 0 && i % 4 == 0 {
            result.push('-');
        }
        result.push(ch);
    }
    result
}

pub fn decode(code: &str) -> Result<EndpointId> {
    let stripped: String = code.chars().filter(|c| *c != '-').collect();
    EndpointId::from_z32(&stripped).context("invalid room code")
}

pub fn parse_node_id(input: &str) -> Result<EndpointId> {
    if let Ok(id) = input.parse::<EndpointId>() {
        return Ok(id);
    }
    decode(input).context("could not parse as EndpointId or room code")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let key = iroh::SecretKey::generate();
        let id = key.public();
        let code = encode(&id);
        let decoded = decode(&code).unwrap();
        assert_eq!(id, decoded);
    }

    #[test]
    fn format_has_dashes() {
        let key = iroh::SecretKey::generate();
        let id = key.public();
        let code = encode(&id);
        assert!(code.contains('-'));
        let parts: Vec<&str> = code.split('-').collect();
        for part in &parts[..parts.len() - 1] {
            assert_eq!(part.len(), 4);
        }
    }

    #[test]
    fn parse_accepts_both_formats() {
        let key = iroh::SecretKey::generate();
        let id = key.public();

        let raw = id.to_string();
        assert_eq!(parse_node_id(&raw).unwrap(), id);

        let code = encode(&id);
        assert_eq!(parse_node_id(&code).unwrap(), id);
    }

    #[test]
    fn invalid_code_errors() {
        assert!(decode("not-a-valid-code!!!").is_err());
        assert!(decode("aaaa-bbbb").is_err());
    }
}
