# TODO

## Phase 1: MVP hardening

- [ ] Handle connection drops gracefully (reconnect loop)
- [ ] Log packet stats periodically (packets sent/received, bytes, drops)
- [ ] Handle oversized packets (>1200 bytes) — fragment or send via stream fallback
- [ ] Signal handling (ctrl+c cleanup — remove TUN device, close connection)
- [ ] Error messages when not running as root/sudo
- [ ] Test across macOS ↔ Linux, Linux ↔ Linux, macOS ↔ macOS

## Phase 2: Multi-peer mesh (single network)

- [ ] Creator acts as initial coordinator — assigns IPs sequentially (100.64.0.1, .2, .3, ...)
- [ ] Accept multiple incoming connections (not just one)
- [ ] Full mesh: each peer connects to all other peers
- [ ] Routing table: `HashMap<Ipv4Addr, Connection>`
- [ ] Control channel (bidirectional stream) for peer list exchange, IP assignment
- [ ] Coordinator broadcasts updated peer list when a new peer joins
- [ ] Replicate peer list to all members (hybrid model — any peer holds full state)
- [ ] If creator goes offline, existing peers stay connected and can invite new ones
- [ ] Any peer can share the network ID to invite others
- [ ] Handle peer disconnect — remove from routing table, notify others

## Phase 3: Multi-network support

- [ ] Persistent config file (`~/.config/pitopi/networks.toml`) storing network memberships
- [ ] Each network identified by creator's EndpointId + network name
- [ ] Each network gets its own TUN device, subnet, ALPN, peer list
- [ ] Subnets: 100.64.1.0/24, 100.64.2.0/24, etc.
- [ ] ALPN per network: `pitopi/net/<network-hash>`
- [ ] Single shared iroh Endpoint across all networks
- [ ] Network isolation — traffic from one network never crosses to another
- [ ] CLI: `pitopi create --name work`, `pitopi join <ticket>`, `pitopi list`, `pitopi leave <name>`
- [ ] Daemon mode: `pitopi up` to connect to all saved networks

## Phase 4: UX polish

- [ ] Room codes / short names instead of raw endpoint IDs
- [ ] Status display (connected peers, latency, direct vs relay)
- [ ] `pitopi status` — show active networks, peers, connection quality
- [ ] LAN game auto-discovery (mDNS proxy over virtual network)
- [ ] Graceful reconnection on network changes (wifi switch, sleep/wake)
- [ ] Graceful disconnect handling
- [ ] Systemd service file for Linux servers
- [ ] launchd plist for macOS

## Phase 5: Daemon & Network Extension

- [ ] `pitopid` daemon — long-running background process managing all networks
- [ ] `pitopi` CLI talks to daemon via Unix socket / gRPC
- [ ] Daemon auto-starts on boot (launchd on macOS, systemd on Linux)
- [ ] macOS Network Extension — use NEPacketTunnelProvider for unprivileged operation (no sudo)
- [ ] macOS System Extension distribution (standalone, outside App Store)
- [ ] App Store distribution variant (Network Extension required by sandbox)
- [ ] Graceful fallback: use Network Extension if available, direct utun if running as root
- [ ] Menu bar app (macOS) / system tray (Linux) for status and network management

## Phase 6: Social discovery & auth

- [ ] Discord OAuth login — authenticate users via Discord
- [ ] Map Discord identity to EndpointId (store on a lightweight coordination server)
- [ ] Discover peers by shared Discord servers — see who's online in your servers
- [ ] Create/join networks scoped to a Discord server or role
- [ ] "Play together" flow: pick a Discord server → see members with pitopi → create a network → they get notified
- [ ] Discord bot companion — `/pitopi create` slash command in Discord, shares join link in channel
- [ ] Slack OAuth — discover coworkers, create work networks scoped to a Slack workspace
- [ ] Slack bot companion — `/pitopi join` slash command, share network in a channel
- [ ] Support other social logins later (Steam, GitHub) for different communities
- [ ] Generic model: any social provider maps groups/servers/workspaces → network discovery

## Phase 7: ACLs & resource control

- [ ] Distinguish between organizations (Discord server, Slack workspace) and users within them
- [ ] ACL policy engine — define rules like `user:alice can access server:gamehost on port 25565`
- [ ] Role-based access: map Discord roles / Slack user groups to ACL groups
- [ ] Admin controls: org admins can grant/revoke access per user, per resource, per port
- [ ] Resource tagging — peers can advertise services (e.g. "minecraft", "ssh") with ports
- [ ] Default policies: deny-all, allow-same-org, allow-all
- [ ] Policy format inspired by Tailscale ACLs (JSON/TOML, human-readable)
- [ ] Packet filtering at the forwarding layer — enforce ACLs before writing to TUN
- [ ] Audit log — who connected to what, when

## Ideas / Future

- [ ] Bandwidth throttling per peer
- [ ] Split tunneling — only route specific subnets through pitopi
- [ ] DNS over the virtual network
- [ ] Windows support (Wintun driver)
- [ ] Mobile (iOS/Android) via Network Extension / VpnService
