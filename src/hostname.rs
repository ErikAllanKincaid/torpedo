//! Hostname generation, validation, and collision handling for Magic DNS.

use rand::RngExt;

use crate::network_name::NOUNS_B;

pub fn generate_hostname() -> String {
    let mut rng = rand::rng();
    NOUNS_B[rng.random_range(0..NOUNS_B.len())].to_string()
}

pub fn is_valid_hostname(name: &str) -> bool {
    if name.is_empty() || name.len() > 63 {
        return false;
    }
    if name.starts_with('-') || name.ends_with('-') {
        return false;
    }
    name.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

pub fn resolve_collision(desired: &str, taken: &[&str]) -> String {
    if !taken.contains(&desired) {
        return desired.to_string();
    }
    for i in 2u32.. {
        let candidate = format!("{desired}{i}");
        if !taken.contains(&candidate.as_str()) {
            return candidate;
        }
    }
    unreachable!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_hostname_is_valid() {
        for _ in 0..100 {
            let h = generate_hostname();
            assert!(is_valid_hostname(&h), "invalid: {h}");
        }
    }

    #[test]
    fn valid_hostnames() {
        assert!(is_valid_hostname("alice"));
        assert!(is_valid_hostname("my-host"));
        assert!(is_valid_hostname("host2"));
        assert!(is_valid_hostname("a"));
    }

    #[test]
    fn invalid_hostnames() {
        assert!(!is_valid_hostname(""));
        assert!(!is_valid_hostname("-start"));
        assert!(!is_valid_hostname("end-"));
        assert!(!is_valid_hostname("UPPER"));
        assert!(!is_valid_hostname("has space"));
        assert!(!is_valid_hostname("has.dot"));
        let long = "a".repeat(64);
        assert!(!is_valid_hostname(&long));
    }

    #[test]
    fn collision_no_conflict() {
        assert_eq!(resolve_collision("alice", &["bob"]), "alice");
    }

    #[test]
    fn collision_appends_number() {
        assert_eq!(resolve_collision("alice", &["alice"]), "alice2");
        assert_eq!(resolve_collision("alice", &["alice", "alice2"]), "alice3");
    }
}
