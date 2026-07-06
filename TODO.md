# torpedo — TODO

Tracking for deferred work on the fork. See `DESIGN.md` for decisions and
`spec/design_spec.py` for the requirement set.

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

### macOS / Android branches
- Still assume `100.64.0.0/10` and carry the old identity on paths the Linux
  fork never executes: macOS `route_peer_range` (`src/tun.rs`), Android
  `VpnService` (`android/`).
- Decision for later: (a) adapt them if we want multi-platform, or (b) rip them
  out for a clean Linux-only fork.
- Leaning (a)/keep-as-is for now: keeping them is a small, rebase-friendly diff
  (same logic as neutralize-not-delete for self-update). Rip out only if torpedo
  becomes permanently Linux-only.

## Upcoming (active agenda)

- [ ] Documentation: write `README.md` (torpedo-focused; "fork of rayfish,
      MPL-2.0, changes the overlay subnet") + a TLDR getting-started section.
      Resolves the pending `README.md` delete.
- [ ] Testing: build a distributable binary for the other test machines
      (`cargo build --release`, or `just cross` for a portable build). Decide
      static vs dynamic.
- [ ] Push `master` to `origin` (github.com/ErikAllanKincaid/torpedo) when ready.
- [ ] Manual Phase-7 live test: `torpedo up` + `create --subnet`/`config set
      subnet` on two machines, confirm mesh + Tailscale coexistence.

## Notes
- Relay / discovery-DNS `rayfish` presets are **kept on purpose** (upstream
  infra, default is n0; honest for a fork). Do not rename — protected by CON-001.
- Self-update is neutralized (`SELF_UPDATE_ENABLED = false`); do NOT enable it —
  `REPO_SLUG` still points at upstream rayfish. Guarded by CON-006.
