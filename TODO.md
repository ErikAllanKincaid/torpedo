# TODO

## Phase 2: Multi-peer mesh (single network)

- [ ] Creator acts as initial coordinator — assigns IPs sequentially (100.64.0.1, .2, .3, ...)
- [ ] Accept multiple incoming connections (not just one)
- [ ] Full mesh: each peer connects to all other peers
- [ ] Routing table: `HashMap<Ipv4Addr, Connection>`
- [ ] Coordinator broadcasts updated peer list when a new peer joins
- [ ] Replicate peer list to all members (hybrid model — any peer holds full state)
- [ ] If creator goes offline, existing peers stay connected and can invite new ones
- [ ] Any peer can share the network ID to invite others

## Phase 3: Multi-network support

- [ ] Persistent config file (`~/.config/pitopi/networks.toml`) storing network memberships
- [ ] Each network identified by creator's EndpointId + network name
- [ ] Each network gets its own TUN device, subnet, ALPN, peer list
- [ ] Subnets: 100.64.1.0/24, 100.64.2.0/24, etc.
- [ ] ALPN per network: `pitopi/net/<network-hash>`
- [ ] Single shared iroh Endpoint across all networks
- [ ] CLI: `pitopi create --name work`, `pitopi join <ticket>`, `pitopi list`, `pitopi leave <name>`

## Phase 4: UX polish

- [ ] Room codes / short names instead of raw endpoint IDs
- [ ] Status display (connected peers, latency, direct vs relay)
- [ ] LAN game auto-discovery (mDNS proxy over virtual network)
- [ ] Graceful reconnection on network changes
- [ ] Graceful disconnect handling
