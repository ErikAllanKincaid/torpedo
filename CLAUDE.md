# Pitopi

P2P mesh VPN powered by [iroh](https://iroh.computer). Connects peers by cryptographic identity (EndpointId), not IP address. Users create and join virtual networks with assigned IPs in the 100.64.0.0/10 (CGNAT) range.

## Build & Run

```bash
cargo -q build
cargo -q check
cargo -q test
cargo -q clippy
```

### Running

```bash
# Start the daemon (required first — owns TUN device and iroh endpoint)
sudo cargo -q run -- daemon

# In another terminal: create/join/manage networks (talks to daemon via IPC)
cargo -q run -- create                      # generates a three-word network name
cargo -q run -- join gentle-amber-fox       # join by three-word name via DHT lookup
cargo -q run -- leave gentle-amber-fox
cargo -q run -- nuke gentle-amber-fox       # publish empty membership + leave
cargo -q run -- status              # live peer info from daemon
cargo -q run -- down                # shut down the daemon

# Standalone (no daemon needed)
cargo -q run -- list                # show saved networks from config

# System service
sudo cargo -q run -- install-service
sudo cargo -q run -- uninstall-service

# Shell completions
cargo -q run -- completions bash > /etc/bash_completion.d/pitopi
```

Only `daemon` (and its alias `up`) requires `sudo`. All other commands run unprivileged via IPC.

### Cross-compile & deploy

```bash
just cross                   # build for x86_64 Linux
just deploy <ip>             # cross-build + install + create group + start daemon service
```

## Architecture

```
App (Minecraft, etc.) → TUN device (100.64.x.x) → pitopi → iroh QUIC datagrams → peer
```

### Modules

- `src/main.rs` — thin CLI client (clap), IPC client functions, `spawn_path_logger`, service install/uninstall; `pitopi create` (no --name, daemon generates name), `pitopi join <three-word-name>`, `pitopi nuke <name>`
- `src/daemon.rs` — daemon process: DaemonState (shared endpoint + TUN + PeerTable), NetworkHandle per active network, IPC server over Unix socket, coordinator accept loop, joiner mesh logic, reconnect loop, DHT publishers (membership + seed list), membership poller, three-word name generation, `nuke_network()`, `restore_coordinator_network()`
- `src/network_name.rs` — three-word name generation: adjective-noun-noun word lists embedded at compile time, `generate_name()` (random selection via rand), `is_valid_name()` for validation
- `src/ipc.rs` — IPC protocol types (IpcRequest, IpcResponse, NetworkStatus, PeerStatus), length-prefixed JSON wire helpers, socket path (`/var/run/pitopi/pitopi.sock`), client connect helper; `IpcRequest::Create` has no `name` field, `IpcRequest::Join` takes `name: String`, `IpcRequest::Nuke { name, force }`
- `src/identity.rs` — persistent Ed25519 keypair at `~/.config/pitopi/secret_key`
- `src/membership.rs` — IdentityProvider trait, FNV-1a IP derivation, MemberList, ApprovedList, GroupMode, MembershipPolicy, canonical msgpack serialization + blake3 hashing (MembershipData); `MembershipData` now includes `network_secret: [u8; 32]` and `membership_signing_key: [u8; 32]`; `canonical_membership_bytes_with_secrets()`
- `src/transport.rs` — iroh endpoint setup, per-network ALPN, connect/accept
- `src/tun.rs` — TUN device creation with /10 netmask, split into TunReader/TunWriter for lock-free I/O
- `src/forward.rs` — multi-peer forwarding: TUN → routing table → correct peer connection, DisconnectEvent notification on peer drop
- `src/dht.rs` — three pkarr record types: directory record (human name → network keys, `derive_directory_key`, `directory_dht_id`, `encode/decode_directory_record`, `publish/resolve_directory`), seed list record (network secret → online peer endpoints, `encode/decode_seed_list_record`, `publish/resolve_seed_list`), membership record (coordinator key → blob hash, existing logic)
- `src/control.rs` — control protocol: Welcome, MemberApproved, JoinApproved, JoinDenied, MemberSync, MeshHello, MeshWelcome, ReconnectRequest, AdvertiseServices
- `src/peers.rs` — PeerTable (routing by dest IP), PeerEntry with Connection + endpoint_id + network name, remove_by_network for teardown
- `src/config.rs` — persistent network config at `~/.config/pitopi/networks.toml` (members + approved list); `NetworkConfig` has `network_pkarr_pubkey: Option<String>` and `membership_dht_pubkey: Option<String>` instead of `coordinator_id`
- `src/acl.rs` — ACL policy engine: default policies, per-rule src/dst/port matching, packet filtering (not yet wired in)
- `src/audit.rs` — append-only audit log at `~/.config/pitopi/audit.log` (not yet wired in)
- `src/stats.rs` — packet/byte counters with periodic logging
- `src/shutdown.rs` — SIGINT/SIGTERM handling via CancellationToken

### Key flows

**Create (coordinator):** generates three-word name (adjective-noun-noun via `network_name::generate_name()`) → generates random `network_secret` ([u8; 32]) → derives `membership_signing_key` from coordinator secret key + network name → publishes directory record (name → network keys), seed list record (network secret → online peers), and membership record (coordinator key → blob hash) to pkarr → spawns DHT publishers and membership poller → listens for connections → on new peer: checks policy, checks IP collision, broadcasts MemberApproved to mesh, sends Welcome with member+approved lists+DHT IDs, promotes to member, broadcasts MemberSync with DHT IDs, notifies publishers.

**Join:** looks up three-word name via directory DHT → resolves seed list (network secret → online peer endpoints) and membership hash in parallel → fetches membership blob from any reachable seed peer via iroh-blobs → verifies blake3 hash → connects to coordinator or mesh peer → receives Welcome (member list + approved list) → joiner checks own IP for collision → creates TUN device → connects to each existing peer with MeshHello → spawns per-peer datagram readers → runs mesh forwarding loop.

**Nuke:** publishes empty membership record + empty seed list to pkarr (announcing the network is gone) → leaves the network (tears down connections, removes from config).

**Gatekeeper model:** coordinator approves identities and broadcasts MemberApproved. Any peer can then welcome an approved identity when it connects. The coordinator doesn't need to be online when the approved peer actually joins.

**DHT membership (three-record model):** Three pkarr record types enable coordinator-free joins:

1. **Directory record** (`derive_directory_key` from blake3 of network name): maps the human-readable three-word name → `{network_secret, membership_signing_key}`. Any peer can look up a network by name.

2. **Seed list record** (derived from `network_secret`): maps the network secret → list of online peer `EndpointId`s. Updated by `spawn_seed_list_publisher()` every 300s. Joiners use this to find online peers to fetch the membership blob from.

3. **Membership record** (derived from coordinator's secret key + network name via `blake3::derive_key`): stores a blake3 hash of canonical membership data (msgpack-serialized, sorted by identity). Joiners resolve the hash then fetch the full blob from any seed peer via iroh-blobs, verifying the hash before trusting the data.

`MembershipData` includes `network_secret` and `membership_signing_key` fields so all peers can republish seed list records. A background `spawn_membership_poller()` checks the membership hash every 60s and reconciles any changes (new members approved while a peer was offline).

**Reconnection:** per-peer reader detects connection drop → sends DisconnectEvent on mpsc channel → coordinator side removes dead peer from PeerTable (peers reconnect to it); joiner side removes dead peer and spawns reconnect task with exponential backoff (1s–30s) → on success, sends MeshHello, adds new connection to PeerTable, spawns fresh peer reader. Packets to the peer drop silently during the gap.

**Mesh forwarding:** TUN read loop extracts dest IP from IPv4 header bytes 16-19, looks up PeerTable, sends datagram on correct connection. Per-peer reader tasks write incoming datagrams to a shared TUN writer channel.

**Network isolation:** each network gets its own ALPN (`pitopi/net/<name>`). A single shared iroh Endpoint accepts connections for all networks, filtering by ALPN on accept. Single TUN device with /10 netmask shared across networks.

**Daemon/IPC:** `pitopi daemon` starts a long-lived root process that owns the iroh Endpoint, TUN device, and PeerTable. CLI commands (`create`, `join`, `leave`, `nuke`, `status`, `down`) connect via Unix socket IPC (`/var/run/pitopi/pitopi.sock`) using the same length-prefixed JSON wire format as `control.rs`. The daemon uses `Endpoint::set_alpns()` to dynamically add/remove network ALPNs at runtime. Each active network gets a `NetworkHandle` with a child `CancellationToken` for clean teardown on leave. `create` generates a three-word name automatically; `join` accepts a three-word name and resolves it via the directory DHT; `nuke` publishes empty records before leaving.

## Key Dependencies

- `iroh` — P2P QUIC transport with NAT traversal and relay fallback
- `iroh-blobs` — content-addressed blob transfer for membership data exchange (FsStore, BlobsProtocol)
- `iroh-dns` — pkarr `SignedPacket` for DHT membership records
- `blake3` — key derivation for per-network DHT signing keys, membership data hashing
- `rand` — random three-word network name generation (`network_name::generate_name()`)
- `tun` — cross-platform TUN device (macOS utun, Linux /dev/net/tun)
- `tokio` — async runtime
- `clap` + `clap_complete` — CLI parsing and shell completions
- `rmp-serde` — msgpack serialization for canonical membership data (compact, deterministic)
- `serde` + `serde_json` + `toml` — serialization for control messages and config
- `dirs` — platform config directory resolution

## Conventions

- Use `cargo -q` for all cargo commands
- Use `tracing` for logging (INFO level by default, configurable via `RUST_LOG` env var)
- ALPN per network: `pitopi/net/<name>` (e.g., `pitopi/net/gaming`)
- Virtual IPs: 100.64.0.0/10 CGNAT range — FNV-1a hash of identity, 22-bit host space
- TUN MTU: 1200 (fits within QUIC datagram limits)
- Identity persists to `~/.config/pitopi/secret_key` — same EndpointId across restarts
- Config persists to `~/.config/pitopi/networks.toml`
- macOS TUN requires destination address (point-to-point interface)
- Control messages: length-prefixed JSON (4-byte BE length + JSON body) over QUIC bidirectional streams
- Three-word names: adjective-noun-noun format (e.g., `gentle-amber-fox`), generated by `network_name::generate_name()` at create time; used as the human-friendly network identifier for joining via DHT lookup; replaces room codes entirely
- Use split/sink patterns for I/O — never share I/O resources (TUN, sockets, streams) behind a Mutex. Always split into separate read/write halves for concurrent access
- Avoid Mutex wherever possible — prefer channels (mpsc), split I/O, atomics, or RwLock (only for fast non-async state)
- Always update docs (CLAUDE.md, docs/book.md, README.md) after finishing a feature or significant change
