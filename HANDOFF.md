# Handoff — 2026-07-20 session

Read this first, then `REWORK_PLAN.md` (the canonical, living plan — this
session updated it, don't re-derive its structure from scratch). This doc is
a snapshot; `REWORK_PLAN.md` is the thing that stays current.

## State of the working tree

Nothing from this session is committed. `git status` shows modified files
across `crates/config`, `crates/ui/web`, `crates/ui/display`, `crates/lua`,
`crates/pwnghost-rs`, `Cargo.toml`, `REWORK_PLAN.md`, and the
`tools/rebase-jayofelony/` pipeline (build.sh, Dockerfile.crosscompile,
overlay/). All of it builds and `cargo test --workspace --lib` passes
(verified as the last action of this session). Review and commit in logical
chunks rather than one giant commit — the changes span genuinely unrelated
concerns (SD-corruption fixes, bullseye64 board support, BT pairing,
WebUI config editor).

## What actually got done this session (don't redo)

1. **Diagnosed the SD-card corruption bug** on reference jayofelony
   2.8.9: a crash-loop → torn ext4 write → kernel panic → forced power-cycle
   cascade, not a hardware defect. Full root cause is in Claude's own project
   memory (`sd_corruption_root_cause` if you're a Claude Code session with
   memory access) and echoed in code comments at each fix site.
2. **Fixed the corresponding gaps in pwnghost-rs itself**: `panic=abort` →
   `panic=unwind` + a panic hook (`Cargo.toml`, `main.rs`); atomic config
   writes (temp+fsync+rename, `crates/config/src/lib.rs::save_config`);
   config validation before persisting a webcfg patch (`crates/ui/web/src/
   api.rs::update_config`); e-ink display calls wrapped in a 5s timeout +
   `WatchdogSec=45` added to both systemd units (the display's SPI/GPIO
   busy-poll was previously unbounded and could hang the whole main loop
   with no recovery).
3. **Added bullseye64 as a Pi Zero 2W-only build option**
   (`tools/rebase-jayofelony/build.sh`, `BASE_VERSION=bullseye64`) — from
   `jayofelony/pwnagotchi-bullseye` v2.6.4 (NOT `jayofelony/pwnagotchi`'s own
   "-64bit" v2.8.9 asset, which is actually Bookworm-based despite the
   version-number match — verified by mounting both directly). Bullseye32
   stays the default, proven on both boards. Real hardware boot validation
   of bullseye64 is still pending — nobody has flashed and booted it yet.
4. **Wired Bluetooth PAN tethering end-to-end** — `bt-agent.service`/
   `bt-pan@.service`/scripts existed only in the unused `pi-gen` pipeline;
   ported into `tools/rebase-jayofelony/overlay` (the pipeline actually
   being tested). `bt_tether.lua` now starts the real `bt-pan@<mac>.service`
   unit instead of a bare `bluetoothctl connect` that never brought up an
   actual network interface.
5. **Fixed a shell-injection-shaped pattern** in `pwncrack.lua` (unescaped
   `handshake_path`/`wordlist` in a hashcat shell command).
6. **Built and verified (live, in a real browser) a config editor** on top
   of the existing deep-merge API: edit JSON → save → deep-merge → validate
   → atomic write to disk, round-tripped and confirmed on a running server.
   This is a *first cut* (one raw-JSON textarea) — see priority list below,
   it needs to become real per-section forms.
7. **Full-repo audit + `REWORK_PLAN.md` update** (this session's second
   half): found and spec'd AngryOxide residue, two real mood-cascade
   correctness bugs, a plugin `enabled` config flag that's currently
   ignored entirely, an unverified/likely-broken wpa-sec upload format, and
   researched the PiSugar S hardware constraints. All written into
   `REWORK_PLAN.md`'s Workstream A-cleanup, C, and new Workstream F —
   read those sections for full detail before starting work, don't
   re-research from scratch.

## Priority order for the next session

Per `REWORK_PLAN.md`'s "Current priority" note, in order:

1. **Workstream A-cleanup** (low effort, do first): rename stale
   "AngryOxide" strings (WebUI header + hint text in
   `crates/ui/web/templates/index.html:154-160`, a `warn!` log string, the
   `HealingAction::RestartAo` enum variant), delete the orphaned
   `crates/angryoxide/` crate, remove the angryoxide binary from the
   `pi-gen` image build. Fix the two mood-cascade bugs (`Motivated`
   short-circuiting past negative-mood checks; `Lonely`'s threshold never
   actually firing due to check order) and unify the two parallel
   face-lookup tables (`crates/agent/src/faces.rs` vs `personality.rs`'s
   `FaceConfig`) against real upstream pwnagotchi source, line-for-line —
   the user's bar is "identical to the pwnagotchi one," not "close enough."
2. **WebUI: full feature parity with the original, but better-looking.**
   Explicit user direction: *"WebUI has all the features that matter from
   the original but improved and better looking."* Concretely, per
   `REWORK_PLAN.md` Workstream C:
   - Structured per-section config forms (real inputs/toggles/dropdowns,
     not a raw JSON textarea — the current one is a placeholder, not the
     target).
   - A plugins page: list all ~19 built-ins, enabled/disabled toggle that
     **actually gates loading** (currently `PluginManager::
     load_builtin_plugins`, `crates/agent/src/plugins.rs:125`, ignores the
     config flag entirely and loads everything unconditionally — fix this
     as part of the page, not separately, or the toggle would lie).
   - Cracked-passwords: **both** a wpa-sec potfile view (not built at all
     yet, `GET /api/wpa-sec/cracked`) and the existing local-hashcat table
     (`pwncrack.lua` + `/api/cracked`) — user confirmed wanting both, not a
     replacement. Also fix `wpa_sec.lua`'s upload (it sends the `.hc22000`
     hash file instead of a raw pcap — its own comment at
     `wpa_sec.lua:60-64` flags this as unverified/likely-wrong).
   - Design bar: "improved and better looking," not just functional parity.
     Worth a real design pass (the `ui-ux-pro-max`/`web-design-guidelines`
     skills were available this session but not invoked for this — do that
     before building the plugins/config-forms UI, not after).
   - Live e-ink view and the deep-merge config API itself are already done
     — build on top, don't rebuild.
3. **Workstream D** (plugin host API, more hooks) as needed to support C4's
   wpa-sec plugin config going through the same safe-merge path as
   everything else — not a hard blocker, but do it if C4 needs it rather
   than bolting on a special case.
4. **Workstream F (PiSugar S button)** — blocked on one open product
   decision: what should short-press vs. long-press actually *do*? Ask the
   user before implementing (info-screen cycle vs. safe shutdown are the
   common patterns for this kind of hardware button). The technical path is
   otherwise scoped: extend the existing `gpio_buttons` Lua plugin
   (already uses `gpioget`, the right modern tool) for GPIO3, and the user
   needs to physically set the PiSugar S board's auto-power-on switch OFF
   (GPIO3 is shared with I2C1 SCL — not software-fixable).
5. **Workstream E (RL/mesh)** — deliberately deprioritized, not the current
   focus. Don't pick this up unless explicitly asked.

## Non-obvious things worth knowing (save yourself the rediscovery)

- **Local dev/testing**: `cargo run -p pwnghost-rs --bin pwnghost-rs --
  --config <path>/config.toml` runs fine on Windows without any real
  hardware — radio/display/bettercap all degrade gracefully to no-ops with
  warnings, and the WebUI comes up on `:8080` and is fully testable in a
  browser. Verified working this session, including the config
  save/round-trip. One gotcha: the `webcfg` **Lua plugin** independently
  hardcodes `/etc/pwnghost/config.toml` for its own read (unrelated to the
  real `--config` arg, which the actual Rust web layer does use correctly
  via `AppState.config_path`) — don't confuse the two if you see "webcfg:
  ... not readable" in logs, it's not evidence the real config path is
  broken.
- **Docker images already built locally**: `pwnghost-builder`,
  `pwnghost-crosscompile-bullseye`, `pwnghost-rebase-jayofelony` — reuse
  these rather than rebuilding from scratch (rebuilding
  `Dockerfile.crosscompile` is needed once, though, since it now targets
  three Rust triples instead of two — armhf×2 + the new
  `aarch64-unknown-linux-gnu`).
- **Base images already downloaded** in `tools/rebase-jayofelony/`:
  `pwnagotchi-2.8.9-32bit.img.xz`, `pwnagotchi-2.9.5.3-32bit.img.xz`,
  `pwnagotchi-rpi-bullseye-2.6.4-arm64.img.xz` (the new bullseye64 base,
  checksum already pinned in `build.sh`).
- **Verify base-image claims by direct mount, not by version number or
  filename** — this session wasted a full download on a wrong assumption
  (jayofelony's own "-64bit" v2.8.9 asset looked like the bullseye64
  answer by version-number match; it's actually Bookworm). The repo's own
  `tools/rebase-jayofelony/README.md` already documents this exact lesson
  from an earlier session about nexmon presence — it keeps coming up,
  take it seriously.
- **Stay scoped to what's actually being asked** — this session sprawled
  into re-explaining Bookworm/2.9.5.x history when the user only wanted
  bullseye32+64 addressed; got corrected for it. Don't re-litigate settled
  context.

## Open questions for the user (don't guess)

- PiSugar S button: short-press / long-press bindings (Workstream F, task
  F2).
- Whether to actually flash and boot-test bullseye64 on real Pi Zero 2W
  hardware before investing further in it, or proceed assuming it'll work
  based on the mounted-image verification already done.
