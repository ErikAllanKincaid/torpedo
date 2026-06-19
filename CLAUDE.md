# Pitopi

P2P mesh VPN powered by [iroh](https://iroh.computer). Connects peers by cryptographic identity, not IP address. Users create and join virtual networks with assigned IPs in the 100.64.0.0/10 (CGNAT) range.

## Build & Run

```bash
cargo -q build
cargo -q check
sudo cargo -q run -- create        # create a network
sudo cargo -q run -- join <id>     # join with an EndpointId
```

Requires `sudo` — TUN devices need elevated privileges.

## Architecture

- `src/main.rs` — CLI entry point (clap), `create` and `join` subcommands
- `src/transport.rs` — iroh endpoint setup, peer connection/acceptance
- `src/tun.rs` — TUN device creation and async packet I/O
- `src/forward.rs` — bidirectional packet forwarding between TUN and iroh datagrams

## Key Dependencies

- `iroh` — P2P QUIC transport with NAT traversal
- `tun2` — cross-platform TUN device
- `tokio` — async runtime
- `clap` — CLI parsing

## Conventions

- Use `cargo -q` for all cargo commands
- ALPN protocol: `b"pitopi/net/0"`
- Virtual IPs: 100.64.0.0/10 range (same as Tailscale)
- TUN MTU: 1200 (fits within QUIC datagram limits)
