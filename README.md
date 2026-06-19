# Pitopi

A peer-to-peer mesh VPN that lets you create private virtual networks without any infrastructure. Built on [iroh](https://iroh.computer), it connects peers by cryptographic identity — not IP addresses — so you never need to deal with port forwarding, dynamic DNS, or firewall rules.

## Why?

You want to play Minecraft with friends, but nobody wants to set up port forwarding or pay for a hosted server. With Pitopi, one person creates a network, shares a code, and everyone joins. Each player gets a virtual IP and the game thinks you're all on the same LAN.

But it's not just for games. Pitopi gives you a private, encrypted network between any set of devices — work machines, home servers, cloud instances — without trusting a third party.

## How it works

1. **Create a network** — one peer starts a network and gets a unique identity (an Ed25519 public key)
2. **Share the ID** — send the ID to friends via any channel (chat, email, carrier pigeon)
3. **Join** — peers connect using just the ID. iroh handles NAT traversal, hole-punching, and encrypted transport automatically
4. **Use it** — every peer gets a virtual IP (100.64.0.x). Any app that uses TCP/UDP just works

Under the hood, Pitopi creates a TUN device on each machine, captures IP packets, and tunnels them through iroh's QUIC-based P2P connections. If direct connections aren't possible (~10% of cases), traffic falls back to encrypted relay servers.

## Usage

```bash
# Create a network
sudo pitopi create
# > Network created!
# > Your virtual IP: 100.64.0.1
# > Share this node ID with your peer:
# >   <endpoint-id>

# Join a network (on another machine)
sudo pitopi join <endpoint-id>
# > Connected! Your virtual IP: 100.64.0.2
# > Tunnel active.

# Now you can ping each other
ping 100.64.0.1   # from the joiner
ping 100.64.0.2   # from the creator
```

Requires `sudo` because TUN devices need elevated privileges.

## Roadmap

- [x] Point-to-point tunnel between two peers
- [ ] Multi-peer full mesh (3+ peers in one network)
- [ ] Multiple simultaneous networks (work + gaming, isolated from each other)
- [ ] Persistent network config
- [ ] Room codes / short invite links
- [ ] LAN game auto-discovery (mDNS proxy)

## Building

```bash
cargo build
```

Requires Rust 2024 edition.
