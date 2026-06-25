# Custom relay, discovery-DNS, and DNS-upstream configuration

Date: 2026-06-25
Status: design approved, pending spec review

## Goal

Let users optionally point rayfish at custom infrastructure, including the
rayfish-operated relay and DNS-discovery servers, instead of (or alongside)
iroh's n0 defaults. Defaults stay n0 — existing installs see no behavior change
until they opt in.

Deployed rayfish infra this targets:

- relay: `http://relay.iroh.rayfish.xyz:3340`
- discovery DNS: `http://dns.iroh.rayfish.xyz:8080`

## Scope

Three independent global settings:

1. `relay` — iroh transport relay (NAT-traversal fallback).
2. `discovery-dns` — iroh's DNS/pkarr discovery server (resolve + publish).
3. `dns-upstreams` — Magic DNS upstream forwarders for non-`.ray` queries.

All three are global by nature: there is one shared iroh `Endpoint` and one
Magic DNS resolver across every network, so per-network overrides do not apply.

Each setting is independent and supports an `augment` (default) or `replace`
mode. Each value is a comma list of **preset keywords** (`rayfish`, `n0`) or
literal URLs/IPs. Presets resolve at use time so a release can change a preset's
URL without rewriting user config.

Out of scope (YAGNI): per-network overrides; live re-bind of the endpoint for
relay/discovery changes (restart instead); a single bundled "provider" switch
(settings are independent); disabling relay entirely.

## iroh integration (verified against iroh 1.0.0)

This codebase pins **iroh 1.0.0** and binds with `Endpoint::builder(presets::N0)`
in `transport.rs::bind_endpoint()`. The API verified from the unpacked crate
source (NOT the older `.discovery()` / `DnsResolver::new(url)` shape, which does
not exist in 1.0.0):

- Builder takes a preset: `Endpoint::builder(presets::N0)`. The `N0` preset
  already stacks `PkarrPublisher::n0_dns()` + `DnsAddressLookup::n0_dns()` and
  sets `relay_mode(default_relay_mode())`.
- Relay override: `.relay_mode(RelayMode::custom(urls: impl IntoIterator<Item =
  RelayUrl>))`. The last `relay_mode` call wins, so this overrides the preset.
- Discovery override: `.address_lookup(PkarrPublisher::builder(url))` and
  `.address_lookup(PkarrResolver::builder(url))`. `address_lookup` **stacks**
  (additive); `.clear_address_lookup()` drops all currently-registered services.
  There is no `.discovery()` method in 1.0.0.
- n0 default relay URLs (for augment): `RelayMode::Default.relay_map()
  .urls::<Vec<RelayUrl>>()`.

We keep `Endpoint::builder(presets::N0)` as the base and override only when a
setting is configured; with everything unset, the bind path is byte-for-byte
today's behavior.

Mode semantics:

- `relay` replace: `.relay_mode(RelayMode::custom(custom_urls))`. `relay`
  augment: `.relay_mode(RelayMode::custom(custom_urls ++
  RelayMode::Default.relay_map().urls()))`.
- `discovery-dns` replace: `.clear_address_lookup()` then
  `.address_lookup(PkarrPublisher::builder(url))
  .address_lookup(PkarrResolver::builder(url))`. augment: skip the clear (the
  custom services stack on top of N0's).
- The `dht.rs` `PkarrRelayClient` (currently `https://dns.iroh.link/pkarr`,
  `PKARR_RELAY_URL`) is a single client with one URL: it follows the first
  configured `discovery-dns` URL when set, else the n0 default. So setting a
  `discovery-dns` server also points network/contact-record publish+resolve at
  it. (Augment does not split publishing across two pkarr relays — one client,
  one URL; documented nuance.)

`dns-upstreams` is unrelated to the endpoint. It is applied at the Magic DNS
resolver: `set_upstreams(servers: Vec<Ipv4Addr>)` at `daemon.rs:3154`, fed from
`captured_upstreams() -> Vec<Ipv4Addr>` at `daemon.rs:3149`. Final list =
`replace ? custom : custom ++ captured`. IPv4 only (the resolver's upstream type),
no ports.

## Config schema (`config.rs` → `settings.toml`)

New optional global settings on `AppConfig` / `Settings`. A small shared type:

```rust
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ServerOverride {
    #[serde(default)]
    pub servers: Vec<String>, // preset keyword or literal URL/IP, as typed
    #[serde(default)]
    pub replace: bool,        // false = augment defaults (default)
}
```

Serialized form:

```toml
[relay]
servers = ["rayfish"]
replace = false

[discovery_dns]
servers = ["rayfish"]
replace = false

[dns_upstreams]
servers = ["1.1.1.1", "8.8.8.8"]
replace = false
```

An omitted or empty `servers` list = unset = today's behavior exactly. `serde`
defaults ensure pre-existing `settings.toml` files load as "unset". Persisted
via the existing `config::save_settings()` atomic write path.

Preset table (constants in the resolving modules):

- `relay`: `rayfish` → `http://relay.iroh.rayfish.xyz:3340`, `n0` → iroh default.
- `discovery-dns`: `rayfish` → `http://dns.iroh.rayfish.xyz:8080`, `n0` → iroh default.
- `dns-upstreams`: no presets; literal IPs only.

## CLI surface (`main.rs`) — client-side config write, no IPC

Follows the established `cmd_mdns` pattern: the CLI reads/writes `settings.toml`
directly via `config::load()` / `config::save_settings()` and prints "restart
the daemon to apply." No new IPC messages. This matches `ray mdns on|off` and
`ray status` (both work by reading/writing config directly). On Linux the config
tree is under `/etc/rayfish` root-owned, so a write naturally requires root/sudo
— same as `ray mdns`; reads are world-readable for non-secret settings.

```
ray config                      # list all settings (alias: ray config get)
ray config get <key>            # one key
ray config set <key> <value> [--replace]   # value = comma list
ray config unset <key>          # revert key to default
```

- Keys: `relay`, `discovery-dns`, `dns-upstreams`.
- Default mode is **augment**; `--replace` opts into replacement. `--replace`
  help text notes the connectivity risk (a bad custom server with no fallback
  can isolate the node).
- Validation at set time: `relay` / `discovery-dns` entries must be a known
  preset keyword (`rayfish` / `n0`) or an `http`/`https` URL (`url::Url::parse`
  + scheme check); `dns-upstreams` entries must parse as `Ipv4Addr`; unknown
  preset keywords rejected with a clear message.
- All three print `run 'sudo ray restart' for changes to take effect.` (same as
  `cmd_mdns`). The daemon applies relay/discovery at endpoint bind and
  dns-upstreams at activate, both on next start.
- `--json` honored for `get` (machine-readable key/value list).

Short aliases follow existing conventions (`get`→`ls`/`show` where it fits,
`unset`→`rm`); finalized in the plan to stay unique within the subcommand enum.

## How the daemon consumes the settings (no IPC)

`build_daemon` already does `config::load()` and calls
`create_endpoint_with_alpns`. Thread `app_config.relay` and
`app_config.discovery_dns` into that call so `bind_endpoint` applies the relay /
address-lookup overrides. The `dht.rs` pkarr client reads the resolved
`discovery-dns` URL (via a small accessor) instead of the hardcoded constant.
For `dns-upstreams`, the `activate()` path (`daemon.rs:3149-3154`) merges the
configured upstreams with `captured_upstreams()` before `set_upstreams`.

## Resolution module

Pure helpers on `config.rs` (or a small `src/serverconfig.rs`) turn a
`ServerOverride` into validated primitives, keeping `config` iroh-light (only
`url::Url` / `std::net::Ipv4Addr`, no iroh types):

- `relay_urls(&ServerOverride) -> Result<Vec<String>>` — resolve presets
  (`rayfish`/`n0`) to URL strings + validate; empty when unset.
- `discovery_urls(&ServerOverride) -> Result<Vec<String>>` — same for the
  discovery server(s).
- `resolve_upstreams(&ServerOverride, captured: Vec<Ipv4Addr>) -> Vec<Ipv4Addr>`
  — `replace ? custom : custom ++ captured`.

The iroh-typed conversion lives at the call site: `transport.rs` parses relay
strings to `RelayUrl` and discovery strings to `url::Url` and applies the
builder methods; `daemon.rs` merges upstreams. This keeps the iroh API surface
in `transport.rs` where it already lives.

## Testing

Unit:

- Preset resolution (`rayfish`/`n0`/unknown) for relay and discovery-dns.
- Entry parsing/validation: good and bad URLs, good and bad IPv4s,
  unknown preset rejected.
- Mode list-building for all three (augment vs replace), including dns-upstreams
  augment merging with a sample captured list.
- `settings.toml` round-trip with serde defaults: an old file (no new sections)
  loads as fully unset.

Manual (all require `sudo ray restart` to take effect):

- `ray config set dns-upstreams 1.1.1.1` + restart, then resolve a non-`.ray`
  name.
- `ray config set relay rayfish` + restart, confirm the endpoint binds against
  the rayfish relay; `ray config set discovery-dns rayfish` + restart, confirm
  publish/resolve via `dns.iroh.rayfish.xyz`.
- `ray config unset relay` + restart restores n0.

## Resolved during planning (was: open items)

1. Relay augment uses `RelayMode::custom(custom ++
   RelayMode::Default.relay_map().urls::<Vec<RelayUrl>>())`; replace uses
   `RelayMode::custom(custom)`. Verified in iroh 1.0.0.
2. One `discovery-dns` URL drives both the endpoint `address_lookup`
   (PkarrPublisher + PkarrResolver) and the `dht.rs` `PkarrRelayClient`. The
   deployed `dns.iroh.rayfish.xyz` serves both roles.
3. iroh 1.0.0 has no `.discovery()` / `DnsResolver::new(url)`; discovery is
   `.address_lookup(...)` which stacks. Augment = stack on N0; replace =
   `.clear_address_lookup()` then add custom.
4. `bind_endpoint()` keeps `Endpoint::builder(presets::N0)` and conditionally
   chains `.relay_mode(...)` / `.clear_address_lookup()` / `.address_lookup(...)`
   only when a setting is configured — no separate "explicit builder" branch.

## Deliberate change from the brainstorm

The brainstorm sketched an IPC-based `ray config` with live DNS apply. Planning
against the codebase showed the `cmd_mdns` precedent (client-side config write +
"restart to apply") is simpler and consistent, and relay/discovery need a
restart regardless. So `ray config` writes config directly and all three
settings apply on daemon restart. No new IPC surface; one fewer moving part.
