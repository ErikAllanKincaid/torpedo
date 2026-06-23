//! Declarative deployment spec for `ray apply` (Phase B of the trusted-networks plan).
//!
//! The spec is a read-only description of the *intended* trusted-network state:
//! which networks should exist (and be trusted), and the suggested firewall
//! rules for each. `ray apply` reconciles the live state against it — creating
//! missing networks and publishing suggestions — but never joins or mutates
//! membership directly (B3 only reports the membership gap and offers to mint
//! hostname-bound invites).
//!
//! The spec reuses [`ray_proto::policy::SuggestedFirewall`] verbatim, so the
//! wire/blob shape and the authoring shape are identical: an admin authors the
//! exact rules a node will materialize, keyed by hostname, before any host has
//! joined. Loading is format-agnostic via the [`config`] crate — YAML, TOML, or
//! JSON, detected by file extension. Output (`--dry-run`, `--example`) is YAML.
//!
//! Firewall model: if a subject has an `allows` list, only those peers pass
//! (a network-scoped catch-all deny is appended); a subject with only `denies`
//! is a blacklist (rest allowed); an empty subject is fully open. There is no
//! `default` field — the mode is inferred from which list is non-empty.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use ray_proto::policy::SuggestedFirewall;
use serde::{Deserialize, Serialize};

/// One network's intended state in a deploy spec.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkSpec {
    /// Trusted network (coordinator may suggest firewall rules). Currently
    /// always `true` in practice — untrusted networks have nothing to apply —
    /// but kept explicit so the spec is self-describing and a future
    /// trustless field can be added without a format change.
    #[serde(default)]
    pub trusted: bool,
    /// Subject hostname → its suggested rules. Empty means "no suggestions"
    /// (useful with `--prune` to clear an existing set).
    #[serde(default, skip_serializing_if = "SuggestedFirewall::is_empty")]
    pub firewall: SuggestedFirewall,
}

/// The full deploy spec: network name → intended state. A [`BTreeMap`] gives a
/// canonical (sorted) serialization, so two admins authoring the same intent
/// produce byte-identical files.
pub type DeploySpec = BTreeMap<String, NetworkSpec>;

/// Load a deploy spec from a file. Format is auto-detected from the file
/// extension by the [`config`] crate (`.yaml`/`.yml` → YAML, `.toml` → TOML,
/// `.json` → JSON). A top-level `networks:` wrapper is accepted (and
/// unwrapped); a flat map (network names at the top level) is also accepted.
/// Unknown fields error.
pub fn load(path: &Path) -> Result<DeploySpec> {
    let cfg = config::Config::builder()
        .add_source(config::File::from(path.to_path_buf()))
        .build()
        .with_context(|| format!("parsing spec {}", path.display()))?;
    deserialize_spec(cfg)
}

/// Try the canonical `networks:` wrapper form, then fall back to a flat map.
fn deserialize_spec(cfg: config::Config) -> Result<DeploySpec> {
    // config-rs represents YAML `null` (e.g. an empty `beta:` subject) as
    // `ValueKind::Nil`. serde can't turn a present-but-Nil value into a struct
    // — field-level `#[serde(default)]` only fires for *absent* keys — so an
    // empty subject would error ("invalid type: null, expected struct") and
    // make the wrapper path fall through to the flat-map fallback, producing a
    // confusing "unknown field" error. Normalize Nil → empty Table first: in
    // this spec a null always means "default/empty" (an open subject).
    let mut value: config::Value = cfg.try_deserialize().context("reading config tree")?;
    normalize_nil(&mut value);
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Wrapper {
        networks: DeploySpec,
    }
    if let Ok(w) = value.clone().try_deserialize::<Wrapper>() {
        return Ok(w.networks);
    }
    value
        .try_deserialize::<DeploySpec>()
        .context("expected a `networks:` table or a flat map of network names")
}

/// Recursively replace `ValueKind::Nil` with an empty `Table` so a null
/// (YAML `key:` with no value) deserializes as a default struct.
fn normalize_nil(v: &mut config::Value) {
    use config::ValueKind;
    match &mut v.kind {
        ValueKind::Nil => {
            v.kind = ValueKind::Table(config::Map::new());
        }
        ValueKind::Table(t) => {
            for (_k, child) in t.iter_mut() {
                normalize_nil(child);
            }
        }
        ValueKind::Array(a) => {
            for child in a.iter_mut() {
                normalize_nil(child);
            }
        }
        _ => {}
    }
}

/// Serialize a spec to YAML (sorted, stable, canonical). Used by `ray apply
/// --dry-run` to echo the normalized intent.
pub fn to_yaml(spec: &DeploySpec) -> Result<String> {
    serde_yml::to_string(spec).context("serializing spec to YAML")
}

/// The example spec printed by `ray apply --example` (YAML).
pub const EXAMPLE_SPEC: &str = r#"# Rayfish deploy spec. See `ray apply --help`.
# Top level is a `networks:` map; keys are network names.
# Save as e.g. deploy.yaml and run: ray apply deploy.yaml
# Format is detected by extension (yaml/toml/json).

networks:
  gaming:
    trusted: true
    firewall:
      # alice has an allow-list ⇒ only listed peers pass, rest denied.
      alice:
        allows:
          bob: "tcp:22"
        denies:
          eve: "icmp"
      # bob's allow-list uses comma-separated proto:ports tokens.
      bob:
        allows:
          alice: "tcp:9000,tcp:8123"
      # An empty subject is fully open (no rules materialized).
      carol: {}
"#;

/// Union of every hostname mentioned in the spec's `firewall:` blocks — both
/// subjects (`self`) and peer hostnames in `allows`/`denies`. This is the set
/// of hosts the spec expects to exist; B3 diffs it against the joined hosts.
pub fn expected_hosts(spec: &DeploySpec) -> Vec<String> {
    let mut set: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for net in spec.values() {
        for (subject, rules) in &net.firewall {
            set.insert(subject.clone());
            for peer in rules.allows.keys() {
                set.insert(peer.clone());
            }
            for peer in rules.denies.keys() {
                set.insert(peer.clone());
            }
        }
    }
    set.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ray_proto::policy::HostSuggestions;

    /// Parse a spec from YAML text. The top level may be a `networks:` table
    /// (wrapper form) or a flat map whose keys are network names. Unknown
    /// fields are rejected so a typo'd key surfaces as an error instead of
    /// being silently dropped with defaults.
    fn parse(text: &str) -> Result<DeploySpec> {
        let cfg = config::Config::builder()
            .add_source(config::File::from_str(text, config::FileFormat::Yaml))
            .build()
            .context("building config")?;
        deserialize_spec(cfg)
    }

    #[test]
    fn parse_yaml_wrapper() {
        let yaml = r#"
networks:
  gaming:
    trusted: true
    firewall:
      alice:
        allows:
          bob: "tcp:22"
"#;
        let spec = parse(yaml).unwrap();
        assert_eq!(spec.len(), 1);
        let g = spec.get("gaming").unwrap();
        assert!(g.trusted);
        let alice = g.firewall.get("alice").unwrap();
        assert_eq!(alice.allows.get("bob").map(|s| s.as_str()), Some("tcp:22"));
    }

    #[test]
    fn parse_yaml_flat() {
        let yaml = r#"
gaming:
  trusted: true
"#;
        let spec = parse(yaml).unwrap();
        assert!(spec["gaming"].trusted);
        assert!(spec["gaming"].firewall.is_empty());
    }

    #[test]
    fn parse_yaml_null_subject_is_open() {
        // A subject written as `beta:` (YAML null) means "empty / fully open".
        // Must deserialize to a default HostSuggestions, not error.
        let yaml = r#"
networks:
  net1:
    trusted: true
    firewall:
      beta:
      gamma:
"#;
        let spec = parse(yaml).unwrap();
        let g = spec.get("net1").unwrap();
        assert!(g.trusted);
        assert_eq!(g.firewall.len(), 2);
        assert!(g.firewall.get("beta").unwrap().allows.is_empty());
        assert!(g.firewall.get("gamma").unwrap().allows.is_empty());
    }

    #[test]
    fn load_toml_wrapper() {
        // `load` auto-detects format by extension. Write a .toml temp file
        // to exercise the TOML path (config-rs handles it via the `toml` feature).
        let dir = std::env::temp_dir().join(format!("rayfish-apply-toml-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("spec.toml");
        std::fs::write(
            &path,
            r#"
[networks.gaming]
trusted = true

[networks.gaming.firewall.alice]
[networks.gaming.firewall.alice.allows]
bob = "tcp:22"
"#,
        )
        .unwrap();
        let spec = load(&path).unwrap();
        let g = spec.get("gaming").unwrap();
        assert!(g.trusted);
        assert_eq!(
            g.firewall
                .get("alice")
                .unwrap()
                .allows
                .get("bob")
                .map(|s| s.as_str()),
            Some("tcp:22")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn roundtrip_yaml_is_stable_and_sorted() {
        let mut spec = DeploySpec::new();
        let mut fw = SuggestedFirewall::new();
        fw.insert(
            "alice".to_string(),
            HostSuggestions {
                allows: [("bob".to_string(), "tcp:22".to_string())].into(),
                denies: [].into(),
            },
        );
        spec.insert(
            "gaming".to_string(),
            NetworkSpec {
                trusted: true,
                firewall: fw,
            },
        );
        spec.insert(
            "admin".to_string(),
            NetworkSpec {
                trusted: true,
                firewall: SuggestedFirewall::new(),
            },
        );
        let s1 = to_yaml(&spec).unwrap();
        let s2 = to_yaml(&parse(&s1).unwrap()).unwrap();
        assert_eq!(
            s1, s2,
            "roundtrip must be byte-identical (sorted canonical)"
        );
        // admin (empty firewall, omitted) sorts before gaming; both present.
        let admin_idx = s1.find("admin:").unwrap();
        let gaming_idx = s1.find("gaming:").unwrap();
        assert!(admin_idx < gaming_idx);
    }

    #[test]
    fn empty_firewall_omits_field() {
        let yaml = r#"
networks:
  gaming:
    trusted: true
"#;
        let spec = parse(yaml).unwrap();
        // Round-trips without emitting `firewall: {}`.
        let out = to_yaml(&spec).unwrap();
        assert!(!out.contains("firewall"));
    }

    #[test]
    fn expected_hosts_collects_subjects_and_peers() {
        let mut spec = DeploySpec::new();
        let mut fw = SuggestedFirewall::new();
        fw.insert(
            "alice".to_string(),
            HostSuggestions {
                allows: [("bob".to_string(), "tcp:22".to_string())].into(),
                denies: [("carol".to_string(), "icmp".to_string())].into(),
            },
        );
        spec.insert(
            "gaming".to_string(),
            NetworkSpec {
                trusted: true,
                firewall: fw,
            },
        );
        let hosts = expected_hosts(&spec);
        assert_eq!(
            hosts,
            vec!["alice".to_string(), "bob".to_string(), "carol".to_string()]
        );
    }

    #[test]
    fn unknown_field_errors() {
        // `bogus` is not a valid NetworkSpec field.
        let yaml = r#"
networks:
  gaming:
    trusted = true
"#;
        assert!(parse(yaml).is_err());
    }

    #[test]
    fn invalid_yaml_errors() {
        assert!(parse("key: [unclosed").is_err());
    }

    #[test]
    fn example_spec_parses() {
        // The constant printed by `ray apply --example` must round-trip.
        let spec = parse(EXAMPLE_SPEC).expect("EXAMPLE_SPEC must parse");
        let g = spec.get("gaming").unwrap();
        assert!(g.trusted);
        assert_eq!(g.firewall.len(), 3);
        let alice = g.firewall.get("alice").unwrap();
        assert_eq!(alice.allows.get("bob").map(|s| s.as_str()), Some("tcp:22"));
        // carol is an empty subject → fully open.
        assert!(g.firewall.get("carol").unwrap().allows.is_empty());
    }
}
