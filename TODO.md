# torpedo — TODO

Tracking for deferred work on the fork. See `DESIGN.md` for decisions,
`AGENTS.md` for agent guidance, and `spec/design_spec.py` for the requirement set.

## Upcoming (active agenda)

- [ ] **PING-001 — `torpedo ping` (identity-based control-channel probe) got zero
      replies AORUS -> xps-17-9720, while raw ICMP over the TUN and `torpedo
      ping` to node-c both worked fine.** Found 2026-07-09, same live 3-machine
      session as ADMIN-001, right after that saga. `torpedo ping xps-17-9720 -c
      3` from AORUS: 3 sent, 0 received, 100% loss ("no reply ... timeout").
      Contrast: `ping -c 3 10.88.80.165` (raw ICMP, same pair) had just
      succeeded minutes earlier (0% loss); `torpedo ping node-c -c 3` from
      AORUS succeeded cleanly (0% loss, via relay, ~59ms avg) in the same
      breath. So this is the inverse of the usual data-plane-vs-control-plane
      split seen elsewhere (e.g. SUBNET-014, where raw ping failed but the
      identity-based path worked) — here the **data plane works and the
      control-channel `ControlMsg::Ping`/`Pong` round trip does not**, and only
      for the one pair (AORUS<->xps) that had just been through ADMIN-001's
      restart/promotion/manual-accept churn today; node-c's connection (never
      restarted mid-session) was unaffected.
      **CONFIRMED real and reproducible, and root cause narrowed to a specific
      node, not a specific link.** Retested `torpedo ping xps-17-9720 -c 5`
      from AORUS — 5 sent, 0 received, 100% loss again; raw `ping -c 3
      10.88.80.165` immediately after was still 0% loss. Reverse direction
      (`torpedo ping aorus -c 5` from xps) — also 100% loss; raw `ping -c 3
      10.88.68.29` from xps — 0% loss. **Then the key test: `torpedo ping
      node-c -c 5` run from xps** — also 5 sent, 0 received, 100% loss, while
      `ping -c 3 10.88.134.27` from xps (same target) was 0% loss. But
      `torpedo ping node-c` from AORUS succeeds cleanly (0% loss, ~51ms avg via
      relay), and node-c's link was never restarted all session.
      **This rules out "the AORUS<->xps link specifically."** Full 3x3 matrix
      completed (node-c->aorus and node-c->xps run directly by the user on
      node-c): every `torpedo ping` call that has **xps on either end** fails
      (AORUS<->xps both directions, xps->node-c, node-c->xps — all 100% loss),
      while every call between the two nodes that never restarted (AORUS,
      node-c, both directions) succeeds at 0% loss. Raw ICMP is unaffected in
      every case tested (data plane is fine everywhere). This is a clean,
      complete confirmation of a **per-node defect on xps**, not a per-link
      one.
      The one thing that singles xps out: it is the only node in this session
      that went through `restore_coordinator_network` (the coordinator-restore
      boot path, taken because it persisted a `network_secret_key` after
      today's `AdminGrant`) rather than the plain member-reconnect/fresh-join
      path AORUS and node-c used. Strongest lead: compare whatever
      `restore_coordinator_network` / `spawn_coordinator_background_tasks`
      wires up for a restored coordinator against what a normal member join
      (`join_mesh_shared` / `spawn_member_control_listener`) wires up for
      `ControlMsg::Ping`/`Pong` handling (the `pending_pongs` correlation map,
      and whatever registers a coordinator's own outbound-ping/inbound-pong
      handling) — the coordinator-restore path likely never sets up (or
      resets) this piece for coordinator-role connections, breaking ping in
      both directions for every peer xps talks to, while leaving the
      independently-wired data-plane/forwarding path untouched. Should be
      reproducible without the full ADMIN-001 scenario: create a network,
      promote a member to co-coordinator, restart it, then `torpedo ping` in
      both directions against any peer.

      **ROOT CAUSE VERIFIED (2026-07-09, code read, not yet fixed).** Every peer
      connection gets a data-plane reader (`forward::spawn_peer_reader`, which
      pulls datagrams -> TUN, so raw IP forwarding always works). But the
      *control* reader (the task that does `conn.accept_bi()` and handles
      `ControlMsg::*`, incl. replying `Pong` to a `Ping` and firing the
      `pending_pongs` oneshot on a `Pong`) is attached to only ONE connection
      per node: the **primary coordinator<->member control link**.
      - Member side: `spawn_member_control_listener` (`join.rs:217`) is attached
        only to `initial_conn` — the single coordinator that welcomed this node
        on join.
      - Coordinator side: `spawn_coordinator_control_reader` is attached in the
        accept path (`accept.rs:87` `handle_known_member_reconnect`, `:586`
        `spawn_admitted_member_tasks`) for each admitted member.
      EVERY other peer link gets `forward::spawn_peer_reader` **only**, no
      control reader — verified at all five such sites:
      `register_mesh_peer` (`join.rs:370`, member dials roster peers on join),
      `spawn_reconnect_loop` (`join.rs:933`, ANY peer re-dial after a drop),
      `MemberAcceptState::register_peer` (`accept.rs:631`, a member accepting an
      inbound peer), and the coordinator-dials-members reconnect
      (`create_join.rs:1329`). `torpedo ping` (`diagnostics.rs:363`) sends its
      `Ping` over `peers.lookup_v4(ip).conn` — whatever connection the PeerTable
      holds for that IP — and needs a control reader on BOTH ends of THAT
      connection. So ping only works over the primary coordinator<->member link.
      Two distinct consequences, both observed:
      1. **member<->member / secondary-coordinator links never had a control
         reader** (roster-peer links). Explains xps<->node-c failing:
         node-c's primary link is to AORUS, so its link to xps is a roster-peer
         link. Also explains why 2-node tests always passed (the only link IS the
         primary link).
      2. **reconnect drops the control reader even on the primary link.**
         `spawn_reconnect_loop` re-dials a dropped peer and re-attaches only
         `forward::spawn_peer_reader` (`join.rs:933`) — never re-attaches
         `spawn_member_control_listener`. So after ANY disconnect/reconnect
         (peer restart, `torpedo restart`, network blip) the re-dialing side
         permanently loses control-message handling on that link until the
         daemon fully restarts into a fresh accept. This is what broke
         AORUS<->xps after xps's restart, and would break even a plain 2-node
         mesh's ping after a single reconnect. **NOT xps- or coordinator-
         specific** — the ADMIN-001 scenario just happened to force the
         reconnect that exposed it.
      **The user's instinct is correct — ping is only the symptom I tested.**
      Any peer-to-peer `ControlMsg` sent over a reader-less link is silently
      dropped: `Ping`/`Pong` today, and `MeshHello` (hostname rename) or
      `AdminGrant` if either is ever delivered peer-to-peer over a non-primary
      or post-reconnect link rather than via the coordinator + signed blob. The
      core protocol survives today only because it funnels all authoritative
      state through the coordinator link + pkarr blob; anything assuming direct
      peer-to-peer control messaging is affected.
      **PROPOSED FIX.** Guarantee exactly one `accept_bi` consumer per
      connection that always handles at least `Ping`/`Pong`. Cleanest: a minimal
      `spawn_peer_ping_responder(conn, pending_pongs, token)` (an `accept_bi`
      loop handling only `Ping`->`respond_pong` and `Pong`->fire
      `pending_pongs`), spawned at exactly the five sites above that today spawn
      `forward::spawn_peer_reader` WITHOUT any control reader. Safe from
      stream-steal precisely because those links currently have zero `accept_bi`
      consumer; must NOT be added to the primary-link sites (`join.rs:217`,
      `accept.rs:87/586`) which already run a full reader. Main plumbing cost:
      thread the `protocol_router.pending_pongs` Arc into `register_mesh_peer`,
      `spawn_reconnect_loop`, `MemberAcceptState::register_peer`, and the
      `create_join.rs:1329` path (none receive it today). Longer-term
      alternative: unify into one role-aware control reader per connection so
      there is never a link without one. Either way add a `spec/design_spec.py`
      requirement (e.g. PING-001) + keep `reconcile.py` green + a
      TESTING.md stage that pings across a reconnect and across a
      non-primary-coordinator pair.

- [ ] **ADMIN-001 — CRITICAL: a newly-promoted co-coordinator can't serve the
      group blob it advertises, breaking the failover story.** Found 2026-07-09
      live-testing a 3-machine/2-coordinator topology (AORUS + xps-17-9720 +
      node-c, `testnet`). Sequence: AORUS creates the network,
      xps joins, AORUS runs `torpedo admin <net> add <xps-short-id>` to promote
      xps to co-coordinator, AORUS then goes offline (`sudo torpedo stop`), and
      node-c tries to join through an invite minted from xps. Join fails:
      `could not fetch group blob from any peer`. node-c's log shows it reached
      xps fine (`connected to peer ... alpn=/iroh-bytes/4`) but the fetch itself
      failed (`blob fetch failed: io: stream reset by peer: error 3`) — xps
      accepted the connection but had nothing to serve.
      **Root cause:** `spawn_member_control_listener`'s `AdminGrant` handler
      (`src/daemon/mesh/join.rs:707-714`) calls `s.refresh_snapshot()` and starts
      the lazy DHT publisher (which announces the *new* blob hash), but never
      writes the corresponding bytes into the local `blob_store` — unlike every
      other coordinator-sealing path (`seal_and_publish`, `create_join.rs:56-58`,
      which does `refresh_snapshot()` then
      `blob_store.blobs().add_slice(&snap.msgpack_bytes).await`). So a freshly
      promoted co-coordinator truthfully publishes a hash in the DHT record that
      its own `iroh_blobs` store has no bytes for. `spawn_member_control_listener`
      doesn't even take `blob_store` as a parameter today, though it's already in
      scope at the call site (`join_mesh_shared`, from `MeshCtx`) — plumbing it
      through is straightforward.
      **`sudo torpedo restart` workaround tried live — partially fixes it, and
      exposes a second, compounding defect.** Restarting the co-coordinator (xps)
      does trigger `restore_coordinator_network` -> `seal_and_publish`, which
      correctly populates `blob_store` this time (confirmed: node-c's next join
      attempt got past the blob-fetch stage entirely, a new failure mode). But
      the restart's roster restore hit `could not restore roster from DHT blob
      ... falling back to config` (DHT/seed-peer fetch still unreachable with
      AORUS down) — and the **fallback config roster is stale**: it reflects
      xps's roster from its *original join*, before the `AdminGrant`, because
      that handler (`join.rs:707-714`) never persists `is_coordinator = true`
      into xps's own config-file member entry, only into the in-memory
      `SharedNetworkState` that a restart discards. So the freshly re-sealed
      blob xps re-publishes after restart still shows *only AORUS* as
      coordinator-flagged.
      **Consequence, confirmed live:** node-c's retried join fetched the blob
      fine, then failed at the dial stage — `no coordinator admitted the join
      (tried 1): coordinator offline: failed to connect to peer`, dialing
      `<endpoint-a>` (AORUS, still offline at that point) and never attempting
      `<endpoint-b>` (xps, the actual invite minter, reachable the whole time).
      `coordinator_dial_order` evidently builds its candidate list from the
      blob's `is_coordinator` flags (or the invite-pinned-minter priority
      doesn't override it) — either way, once xps's own flag is wrong in the
      republished roster, new joins can't reach it even though it holds the key
      and is willing to serve.
      **Net effect:** the two defects compound. Fresh promotion breaks blob
      serving outright (fix #1). Recovering via restart fixes blob serving but
      re-exposes a stale roster that excludes the co-coordinator from the dial
      list entirely (fix #2) whenever the DHT/seed-peer roster fetch can't reach
      the original coordinator to get the authoritative version. Both must be
      fixed together for the "survives any single coordinator offline" claim in
      AGENTS.md to actually hold.
      **Fix:**
      1. Add `blob_store: FsStore` to `spawn_member_control_listener`'s params
         (pass `blob_store.clone()` from `join_mesh_shared`, already in scope),
         and after `s.refresh_snapshot()` in the `AdminGrant` arm, write
         `blob_store.blobs().add_slice(&bytes).await` the same way
         `seal_and_publish` does — the `RwLockWriteGuard` must be dropped before
         the `.await`.
      2. In the same `AdminGrant` handler, persist `is_coordinator = true` into
         the config-file member entry for `my_identity_c` (not just the
         in-memory `SharedNetworkState`), so a later restart's config-fallback
         roster carries it forward even when the DHT/seed-peer fetch fails.
      Add a `spec/design_spec.py` requirement + keep `reconcile.py` green.
      **Live-test workaround that unblocks a demo/test today without the code
      fix:** bring the original coordinator back online (`sudo torpedo start`)
      so it's a valid, correctly-flagged dial target again; defer the actual
      failover proof until this is fixed in code.

- [ ] **CONN-001 — relay stickiness after a network change (investigate).** Found
      2026-07-08 moving xps-17 LAN -> hotspot -> LAN. Once the endpoint's address
      set churns and iroh falls to `relay`, it does NOT re-probe the now-available
      direct path: stayed on relay (~60ms) for ~7 min back on the LAN; `torpedo
      down`/`up` did not help; only `sudo torpedo restart` restored direct (~5ms).
      Cross-NAT ingress itself worked (via relay on the hotspot). Likely upstream
      iroh path-management behavior. Options: trigger a re-dial / path re-probe on
      a detected network change, or (acceptable stopgap) document "run `torpedo
      restart` after changing networks". Also seen: iroh resolves relay hostnames
      via system DNS, so a transition DNS hiccup briefly stalls relay too.

- [ ] **DNS-003 — CRITICAL, TOP PRIORITY: mutual DNS forwarding loop with
      Tailscale on tier-5 hosts.** Found 2026-07-08 live testing on xps-17-9720.
      torpedo's `DirectResolvConf` takeover rewrites `/etc/resolv.conf` to its
      own magic resolver; `tailscaled` also watches that file for its own
      upstream and adopts torpedo's IP, creating a forever query loop between
      the two (`torpedo -> Tailscale -> torpedo -> ...`). Breaks ALL system DNS
      (not just `.ray`), including torpedo's own pkarr discovery (so `torpedo
      join` fails with a misleading "Service 'pkarr' failed"). `.ray` names
      resolve fine — the bug is specifically the non-`.ray` upstream-forwarding
      path. Does not reproduce on tier-1 (systemd-resolved) hosts, since
      neither daemon touches `/etc/resolv.conf` there. See
      `spec/design_spec.py`'s `DNS-003` for full diagnosis + evidence.
      **Primary fix landed (DNS-005):** Magic DNS is now opt-in — `magic-dns`
      config (`off|auto|direct`, default `auto`) no longer seizes
      `/etc/resolv.conf` unless `direct`, so the loop cannot occur by default;
      reach peers by mesh IP (`torpedo status`). Verified live on xps + Tailscale
      (default `auto`: resolv.conf untouched, join succeeds, no loop).
      **`DNS-004` also landed** (100.64/10 loop-breaker + real-upstream recovery
      from Tailscale's pre-takeover backup) as the safety net for the opt-in
      `magic-dns direct` path. NM D-Bus recovery source deferred. Still to do:
      live-verify the `direct` path on xps + Tailscale.

- [x] Documentation: `README.md` (torpedo-focused fork intro + background/further
      reading + image) and `AGENTS.md` (canonical agent guide; `CLAUDE.md` symlink).
- [ ] Testing: build a distributable binary for the other test machines
      (`cargo build --release` → `target/release/torpedo`, or `just cross` for a
      portable build). Decide static (musl) vs dynamic (glibc >= 2.39 on target).
- [ ] Push `master` to `origin` (github.com/ErikAllanKincaid/torpedo) when ready.
- [ ] Manual Phase-7 live test: `torpedo up` + `create --subnet` / `config set
      subnet` on two machines, confirm mesh + Tailscale coexistence.
- [ ] Optional guardrail: add `RENAME-010` (build-tooling identity) + a
      `reconcile.py` check that greps `justfile`/`contrib/` for stale
      `ray`/`rayfish` tokens, so the justfile fix can not silently regress.
      (The justfile is not Rust, so CON-007 does not cover it.)
- [x] **RENAME-011 — user-facing CLI hint strings still say `ray`** (found in
      Phase-7 testing: `torpedo create` prints `ray join …` / `ray up`). Done:
      41 live/reachable sites swept `ray <subcommand>` -> `torpedo <subcommand>`
      across `src/main.rs`, `src/apply.rs` (incl. `EXAMPLE_SPEC`),
      `src/onepassword.rs`, `src/cli/{status,network,invite,pair,connect,alias,
      service,files,firewall}.rs`, `src/daemon/mod.rs`, `src/daemon/mesh/
      {runtime,create_join,files,firewall}.rs`, plus the dormant `APP_NAME`
      constant in `src/lib.rs`. Since this is pre-release WIP with no real
      backups to break, the 1Password item **title** default was also renamed
      `Rayfish Identity` -> `Torpedo Identity` (no back-compat lookup needed).
      See `spec/design_spec.py`'s `RENAME-011` for the full include/exclude list
      (the `.ray` Magic-DNS TLD and the internal `rayfish` crate name are
      correctly excluded). No `reconcile.py` guard added (see that class's
      docstring for why a token-count gate would false-fail on the
      deliberately-untouched comments and dead `cli/update.rs` code).
- [ ] **DNS-001-fix — warning not delivered in the real flow** (found in Phase-7 on
      tier-5 xps): the daemon auto-activates at startup, so the DNS takeover + warning
      happen there (log only) and the interactive `sudo torpedo up` short-circuits with
      `already up`, never populating the warnings channel. Fix: persist the active DNS
      mode + warning on the daemon (set in `DnsManager::configure`); return it from `up`
      even on the already-active path. Pairs with DNS-002.
- [ ] **DNS-002 — surface active DNS mode in `torpedo status`** (now necessary, not
      optional): the daemon exposes its DNS backend / takeover state; `status` (and
      `--json`) show it, covering the non-interactive (reboot / auto-activate) path where
      `up` prints nothing. Add `dns_mode` to `StatusResponse`; CLI renders it.
- [x] **Doc fix — resolver IP is subnet-derived** (`10.88.100.53` on the default subnet,
      not `100.100.100.53`): correct AGENTS.md, TESTING.md, and any prose that hardcodes
      `100.100.100.53`. Done (DOC-001): AGENTS.md's 4 hardcoded-literal mentions reworded
      to describe the subnet-derived formula; DESIGN.md/TESTING.md's mentions were
      already correctly historical and needed no change.
- [ ] **Investigate — resolv.conf re-assert storm** (3x within ~1s at startup on xps):
      confirm it always settles; if some hosts sustain the trample fight, damp the
      re-assert loop or widen the quiet-window guard.
- [x] **CRITICAL — `create --subnet` corrupts the data plane** (Phase-7): it sets the
      network roster/blob to the requested subnet but leaves the node's TUN/config
      subnet at default, so roster (`10.99.x`) and TUN (`10.88.x`) diverge and NO IP
      forwarding works between nodes (raw `ping` fails both ways; only `torpedo ping`,
      which is identity-based, works). The `--subnet` flag does only the roster half of
      what `config set subnet` + restart does. Fix: make `create --subnet` set the node
      subnet (rebuild the TUN live, or require/trigger a restart), or reject `--subnet`
      when it differs from the node's current subnet with a clear "run `config set
      subnet <cidr>` + restart first" message. See `create_join.rs` create path +
      `set_node_subnet` + `blob_subnet`.
      Done (SUBNET-014): took the "warn honestly, require restart" option, not the
      "apply live" one. `membership::subnet_change_warning` (unit-tested) is called
      from all three create/join sites in `create_join.rs`, carried on the `Created`/
      `Joined` IPC messages, and printed by the CLI as `⚠ subnet … takes effect after
      sudo torpedo restart`. The silent mismatch-until-restart window is gone; the
      mismatch itself (until you actually restart) still exists by design.
- [x] **Doc — audit AGENTS.md invite/CLI against the real binary**: `torpedo invite`
      has no `--hostname`/`--expires`/`--qr`/`--reusable`/`list`/`revoke` (usage is just
      `invite <NETWORK>`), yet AGENTS.md (inherited from upstream) documents them. Sweep
      AGENTS.md for other commands/flags the fork's binary does not actually implement.
      Done (DOC-001): the original diagnosis was wrong, not the binary — all those flags
      exist, they belong to an explicit `create` subcommand clap requires spelled out
      before it will parse subcommand flags. AGENTS.md/TESTING.md corrected to show
      `create` explicitly rather than trimmed, since nothing was actually missing.
- [ ] **TOR-001 — `--tor` is endpoint-wide, not per-network.** Found 2026-07-08
      auditing `src/transport.rs`. `bootstrap.rs:171-174` computes one bool — "does
      *any* saved network want Tor" — and builds the single shared iroh endpoint
      accordingly; `bind_endpoint` then adds `TorCustomTransport` **alongside**
      relay/direct (`transport.rs:134-151`), never in place of them. So enabling
      `--tor` on one network adds the Tor path to the daemon's endpoint globally —
      every other network on that same daemon rides the same Tor-capable endpoint
      too. There is no way to run "network A over Tor only, network B direct-only"
      on one node, and no way to force Tor-exclusive (no relay/direct fallback) for
      a hardened case. May be intentional (AGENTS.md already documents "adds Tor
      transport alongside relay"), but the per-network `--tor` flag on
      `create`/`join` implies isolation it doesn't deliver. Options:
      (a) leave as-is, just correct the CLI help text so `--tor` reads as a
      daemon-wide opt-in rather than a per-network toggle;
      (b) make it a true per-network mode by dialing/accepting that network's
      ALPN only over the Tor transport (bigger change — iroh's transport
      selection is endpoint-wide, not per-connection-aware of which ALPN a
      connect is for, so this needs upstream-level support or a per-network
      endpoint, which conflicts with the "single shared endpoint" architecture);
      (c) add a `torpedo config set transport tor-only` global kill-switch that
      drops relay/direct entirely once Tor is enabled, for the fully-anonymous
      single-purpose-node case.
- [ ] **TOR-002 — enabling `--tor` on a running daemon silently has no effect
      until restart.** Found 2026-07-08, same audit. `create_network_inner`
      (`create_join.rs:233`) never touches the endpoint; the endpoint is only
      built once at daemon bootstrap (`bootstrap.rs`). Unlike the subnet-mismatch
      case a few lines below it (which explicitly errors with "run `torpedo
      config set subnet <cidr>` and restart"), there is no equivalent warning
      here — `torpedo create --tor` (or `join --tor`) on a live daemon with no
      prior Tor network succeeds and reports success, but the connection never
      actually uses Tor until `sudo torpedo restart`. Options:
      (a) cheapest/likely right: detect the mismatch (`--tor` requested but the
      running endpoint has no Tor transport) and return a warning in the
      `Created`/`Joined` response, same pattern as the subnet case;
      (b) rebuild the endpoint live when the first Tor network is added (bigger
      change — endpoint rebuild mid-run affects every already-connected peer on
      every network, need to re-home all live connections);
      (c) document the restart requirement in AGENTS.md/CLI help and leave the
      silent gap (weakest option, easy to miss since it fails open with no
      error).

- [ ] **MINIMAL-001 — feature-gate unneeded satellites for a mesh-only build.**
      Goal: a lean torpedo that provides only the identity-based mesh overlay
      (TUN + subnet + firewall + DNS), using stock host `sshd` over the overlay
      instead of the embedded mesh SSH. **Key fact: mesh SSH is already
      runtime-off by default** (`ssh_enabled=false`, the `:22<->:30022` userspace
      NAT inactive, admits nobody until `torpedo firewall ssh on` + an `ssh_allow`
      entry). So this is not about "turning it off" — it is off. It is about
      dropping the code, the `russh` dependency, and the attack surface from the
      binary. **Approach: Cargo feature gates (default-on), NOT deletion** —
      `cargo build --no-default-features` yields the minimal core, each satellite
      stays one flag away, upstream code is preserved (matches the fork's
      spec-first + KEEP-ON-PURPOSE philosophy). **Start with `ssh`** to prove the
      pattern: highest value because it is redundant with `sshd` and carries the
      most security-sensitive surface (host-key extraction via `sshd -T`,
      privilege drop in `pre_exec`, `cfg(macos)` blocks, a userspace `:22` NAT in
      the packet hot path). Other candidates, in rough order: `files` (use
      scp/rsync over the mesh), `connect` (2-peer friend-request flow), `pair` +
      `onepassword` (multi-device identity + backup), `apply` (YAML
      orchestration); `tor` is already a feature. **Watch-outs:** each satellite
      spans `MeshManager` fields, a `ProtocolRouter` ALPN accept arm, `IpcMessage`
      variants, CLI subcommands, and config keys — all must be gated together, or
      the build breaks. SSH's userspace `:22<->:30022` NAT lives in `forward.rs`
      (the per-packet data path), so gate that carefully to avoid touching the hot
      loop. Add a `spec/design_spec.py` requirement per gate + keep `reconcile.py`
      green. Note: removing mesh SSH drops a Tailscale-parity differentiator, but
      it is unneeded for this personal-use fork.

## macOS rewrite — adapt to torpedo

Decision: adapt to torpedo rather than rip out. Ripping out stays the
alternative only if torpedo becomes permanently Linux-only.

- [ ] Make `route_peer_range` **subnet-agnostic** (`src/tun.rs:286`, `#[cfg(macos)]`):
      it hardcodes `route add -inet 100.64.0.0/10` (+ `-inet6 200::/7`). The Linux
      path already reads the network's configured subnet; the macOS path does not,
      so on macOS the fork routes the wrong /10 and ignores `--subnet`.
- [ ] Audit `route_self_loopback` and any other `#[cfg(target_os = "macos")]`
      block in `src/tun.rs` for hardcoded `100.64` / stale identity.
- [ ] Identity: launchd label already `com.torpedo.vpn` (RENAME-008); confirm no
      other `rayfish` host artifacts remain on macOS-only paths.
- [ ] **Must build + test on a real Mac** — cfg(macos) code is not compiled or
      type-checked on this Linux host, so all the above is compiler-unverified.

## Android rewrite — adapt to torpedo

Decision: adapt to torpedo rather than rip out. Ripping out stays the
alternative only if torpedo becomes permanently Linux-only.

- [ ] **Deep-link scheme mismatch (broken):** `AndroidManifest.xml` registers
      `android:scheme="rayfish"` but the Rust side is now `torpedo://` (RENAME-007).
      Android deep links do not work until the manifest is updated to `torpedo`.
- [ ] Kotlin identity rename `rayfish` -> `torpedo`: package `xyz.rayfish.android`,
      `RayfishApp` / `RayfishTheme` / `RayfishVpnService.kt`, thread `rayfish-node-stop`,
      and the JVM package dir `android/app/src/main/java/xyz/rayfish/...`.
- [ ] `ray-mobile` crate (`lib.rs`, `android_tun.rs`, `diag.rs`): make the
      `VpnService` TUN setup **subnet-agnostic** (drop `100.64` assumptions). Decide
      whether to rename the crate/artifact (`ray-mobile` / `libray_mobile`) or keep
      it as an internal name like the `rayfish` library crate.
- [ ] Build prerequisites: `cargo-ndk`, the Android rust targets, JDK 17
      (`just apk`). Verify `just build`/`just apk` after the fixes.

## Deferred (decision made: not now)

### Adapt the multi-node test harnesses to torpedo
- `tests/e2e/*`, `tests/bench/*`, `tests/lib/common.sh` are upstream shell-based
  harnesses. They are **not** part of `cargo test` / `reconcile.py`, so they do
  not gate the build.
- They hardcode the old identity (`ray`/`rayfish`, `/etc/rayfish`) and the old
  `100.64.x.x` range, so they fail against torpedo as-is.
- Downstream of the "prepare a distributable binary" task (they run the binary).
- When adapting: make them **subnet-agnostic** — read each node's real assigned
  IP from `torpedo status` (e.g. `common.sh` `own_ip`) instead of assuming
  `100.64`, so they never rot on a default-subnet change again.
- Priority: after the fork is proven via the manual Phase-7 two-machine test;
  worthwhile for automated multi-node regression coverage of a P2P VPN.

## Notes
- Relay / discovery-DNS `rayfish` presets are **kept on purpose** (upstream
  infra, default is n0; honest for a fork). Do not rename — protected by CON-001.
- Self-update is neutralized (`SELF_UPDATE_ENABLED = false`); do NOT enable it —
  `REPO_SLUG` still points at upstream rayfish. Guarded by CON-006.
- Internal Cargo library crate name `rayfish` (`use rayfish::…`, `info,rayfish=debug`)
  is kept on purpose — renaming it is churn with no user-visible benefit.
