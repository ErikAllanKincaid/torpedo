# Pitopi

P2P mesh VPN powered by [iroh](https://iroh.computer). Connects peers by cryptographic identity (EndpointId), not IP address. Users create and join virtual networks with assigned IPs in the 100.64.0.0/10 (CGNAT) range.

## Build & Run

```bash
cargo -q build
cargo -q check
sudo cargo -q run -- create        # create a network
sudo cargo -q run -- join <id>     # join with an EndpointId
```

Requires `sudo` — TUN devices need elevated privileges.

### Cross-compile & deploy

```bash
just cross                   # build for x86_64 Linux
just deploy <ip>             # cross-build + rsync + install to server
```

## Architecture

```
App (Minecraft, etc.) → TUN device (100.64.0.x) → pitopi → iroh QUIC datagrams → peer
```

- `src/main.rs` — CLI entry point (clap), `create` and `join` subcommands
- `src/identity.rs` — persistent keypair stored at `~/.config/pitopi/secret_key`
- `src/transport.rs` — iroh endpoint setup, peer connection/acceptance
- `src/tun.rs` — TUN device creation with virtual IP and destination, async packet I/O
- `src/forward.rs` — bidirectional packet forwarding between TUN and iroh datagrams

## Key Dependencies

- `iroh` — P2P QUIC transport with NAT traversal and relay fallback
- `tun` — cross-platform TUN device (macOS utun, Linux /dev/net/tun)
- `tokio` — async runtime
- `clap` — CLI parsing
- `dirs` — platform config directory resolution

## Conventions

- Use `cargo -q` for all cargo commands
- Use `tracing` for logging (INFO level by default, no env filter)
- ALPN protocol: `b"pitopi/net/0"`
- Virtual IPs: 100.64.0.0/10 range (CGNAT, same as Tailscale)
- TUN MTU: 1200 (fits within QUIC datagram limits)
- Identity persists to `~/.config/pitopi/secret_key` — same EndpointId across restarts
- macOS TUN requires destination address (point-to-point interface)
