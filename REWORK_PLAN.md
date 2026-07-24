# PWNGHOST-RS Rework Plan

Derived from a four-part source audit of the real implementations
(jayofelony/pwnagotchi `noai`, evilsocket/pwnagotchi, MrBumChinz/oxigotchi)
against our current code. Every claim below is backed by file:line evidence
gathered in that audit.

## Objective

Make PWNGHOST-RS actually behave like a pwnagotchi on the real target
hardware (Pi Zero W / Zero 2W, internal Broadcom nexmon chip): perceive the
RF environment, decide and execute real attacks, capture handshakes
reliably, render a face and UI that match the original, and expose a web UI
with a live display view, safe config editing, plugin management, and
wpa-sec.

Success = on real hardware: APs appear in the UI with real counts,
handshakes get captured, the face looks like real pwnagotchi's (smooth TTF,
not blocky), the web UI shows the live e-ink frame, and the agent's
decisions actually drive the radio.

## What this rework is (and isn't) — the Rust value proposition

Real pwnagotchi = a **Python** agent/AI/web/plugin stack + **Go** bettercap +
**Go** pwngrid. bettercap has always been the capture engine. This project's
value was never in replacing bettercap — it is in replacing the **Python
agent layer** (the heaviest, most RAM-hungry, most crash-prone part) with a
lean Rust daemon, and in hardening the image against SD-card wear.

Switching capture from AngryOxide to bettercap therefore does **not** undo
the rework. After the switch:

- Real pwnagotchi: Python agent (heavy) + Go bettercap
- PWNGHOST-RS:    Rust agent (~120 MB, no GC) + Go bettercap

The Rust advantages are all in the agent layer we keep, and are preserved:

1. **Performance** — measured ~120 MB RSS on-device vs a much heavier CPython
   agent+AI+plugins; no GC pauses; fast start; the `spawn_blocking` display
   fix keeps radio/agent/UI from stalling each other. Untouched by the
   capture backend.
2. **SD-card failure prevention** — image/OS-level, independent of capture:
   zram-backed logs (writes to compressed RAM, not the card), `/tmp` as
   tmpfs, the hardened systemd unit (ReadWritePaths etc.), `e2fsck` on build.
   Fully preserved.
3. **Crash resilience** — the multi-layer healer (restart engine →
   power-cycle GPIO → safe mode → USB lifeline) + recovery persistence.
   Preserved; it simply restarts bettercap instead of AO.
4. **Single static binary** — no pip/venv dependency drift; atomic updates.

**Alternative considered:** keep capture in pure Rust via a native libpcap
sniffer + injector (the oxigotchi approach) — more Rust, no bettercap
dependency, but we would own all the nexmon monitor-mode/injection quirks
ourselves (high risk on this FullMAC chip). bettercap is the pragmatic,
proven choice for reliable capture on this exact hardware and matches real
pwnagotchi. This plan assumes bettercap; the native-Rust radio path is a
documented fork point if maximal-Rust ownership is preferred over capture
reliability.

## The single fact that reframes everything

Our agent's "brain" (personality, mood, epoch, RL, targeting) computes on
**empty inputs** and its **outputs are discarded**:

- `agent.aps` is only ever populated by `update_aps()`, whose sole callers
  are test code (`crates/agent/src/lib.rs:339`; verified). So `find_target()`
  always returns `None`, `aps_count()` is always 0, RL features are all-zero,
  and every epoch reads as "blind."
- `AgentAction::Deauth`/`Associate` are no-ops (`crates/pwnghost-rs/src/main.rs:686-692`,
  just an `info!` log). `Hop` is "informational; AO manages hopping."
- Root cause: AngryOxide is spawned once with static CLI args and exposes no
  control channel and no observable AP list (`crates/angryoxide/src/spawn.rs:30-101`;
  `crates/agent/src/lib.rs:369-373` admits "AO exposes no such data").

Separately, AngryOxide **cannot even capture** on this hardware: it manages
monitor mode itself via netlink (mac80211 SoftMAC assumption), but the Pi's
internal Broadcom chip is FullMAC where monitor mode exists only through the
nexmon firmware patch. The live signature `Frames: 0 | ERs: ~190 |
NetworkDown | os error 132 (ERFKILL)` is that incompatibility — the radio
never delivers a single frame. Real pwnagotchi avoids this by building
`wlan0mon` via `monstart` and pointing bettercap at an already-monitor
interface; bettercap never manages monitor mode itself.

**Both problems have the same fix.** bettercap gives us a live AP/client
list (eyes), a REST control channel (real actions), and reliable capture on
this exact chip. That is why Workstream A is the keystone: it resurrects the
whole decision loop and makes attacks real at the same time.

## Workstream A — Switch capture backend from AngryOxide to bettercap (KEYSTONE)

**Why first:** fixes capture, gives the agent real perception, and makes its
decisions real — all at once. The base jayofelony image is already built
around bettercap (`monstart`/`monstop`, `wlan0mon`, the bettercap service),
so this works *with* the platform instead of against it.

Tasks:
- A1. New `bettercap` client crate (replace the `angryoxide` process/parse
  layer): HTTP Basic REST client to `http://127.0.0.1:8081/api/session` +
  `/api/events` websocket consumer. Mirrors `pwnagotchi/bettercap.py`.
  - Acceptance: can issue `session`, `set`, and `run` commands and receive
    parsed events; unit-tested against recorded bettercap JSON.
- A2. Interface setup: `main.iface = wlan0mon`, invoke `monstart` at startup
  (already on the image). Remove AO's `--disable-deauth`/band/gpsd arg model.
  - Acceptance: service starts, `wlan0mon` exists, bettercap `wifi.recon on`
    succeeds.
- A3. Agent perception: poll bettercap's AP+client list
  (`get_access_points_by_channel` equivalent) each loop and feed `update_aps()`
  with real data.
  - Acceptance: `aps_count()` > 0 with APs nearby; web dashboard AP count is
    real; RL/mood features non-zero.
- A4. Real actions: wire `AgentAction::Hop/Deauth/Associate` to
  `wifi.recon.channel` / `wifi.deauth <mac>` / `wifi.assoc <mac>`, gated by
  `max_interactions`/whitelist/already-have-handshake like `agent.py`.
  Rate-limit deauth (nexmon injection can crash firmware under load).
  - Acceptance: logs show real deauth/assoc issued against real targets;
    firmware survives.
- A5. Handshake capture: consume `wifi.client.handshake` events → existing
  `crates/agent/src/capture.rs` validate + move-to-final (unchanged).
  - Acceptance: real handshakes land in `/etc/pwnghost/handshakes`.
- A6. Retire/park the `angryoxide` crate and its dead action machinery.

Effort: High. Impact: Critical — nothing else matters if it can't pwn.

## Workstream A-cleanup — AngryOxide residue + face/personality parity fixes

**Status (2026-07-20): DONE.** All of AC1-AC6 landed and verified (workspace
builds, `cargo test --workspace --lib` passes, `cargo clippy --workspace
--all-targets` clean). Summary of what was done:
- AC1: WebUI "Live activity (AngryOxide)" header + hint strings made
  backend-neutral (`index.html`).
- AC2: `crates/angryoxide/` deleted (was orphaned; not a workspace member).
- AC3: `HealingAction::RestartAo` -> `RestartCapture`, `HealingLayer::
  AoBackoff` -> `CaptureBackoff`, `max_ao_backoff_attempts` /
  `ao_backoff_max_duration` renamed; the `warn!` journal string is now
  "Soft-resetting capture backend".
- AC4: AngryOxide download/install removed from `pi-gen/stage4/00-install-
  artifacts/00-run.sh`; `ANGRYOXIDE_ARCH`/`ANGRYOXIDE_VERSION` removed from
  `pi-gen/config`; both `bootlog.sh` copies now check `bettercap` instead of
  `angryoxide`; now-false build comments corrected.
- AC5: `Personality::compute_mood` fixed -- negative blind-epoch cascade now
  evaluated BEFORE peers (ordered angry>lonely>sad>bored so `Lonely` is
  reachable), and a present peer applies the real pwnagotchi support-network
  override (Grateful *instead of* the negative mood) rather than short-
  circuiting to Motivated. Regression tests added.
- AC6: face tables unified to a single source of truth,
  `pwncore::Mood::face()` (one upstream-verified face per mood, from
  faces.py). `agent::faces::face_for_mood` and `ui/display::face_for_mood`
  now delegate; the fabricated multi-variant `FaceConfig` (agent + config
  schema + defaults.toml + migrate) and `Mood::faces()`/`random_face()` were
  removed. `[personality.faces]` is gone from config (extra keys in old
  TOMLs are ignored -- no `deny_unknown_fields`).

Remaining note for a future session (NOT part of A-cleanup, needs user
sign-off per Boundaries): `pi-gen/stage5/00-install-pwnghost/00-run.sh:209`
and `:255` still describe the image setting the capture interface to
`wlan0` (AngryOxide-era) while the runtime now uses `wlan0mon`. That's the
"interface model" change flagged as *Ask first* -- left untouched here.

---

**Original spec (kept for reference):** Found during a full-repo audit
triggered by the user noticing stale "AngryOxide" text in the WebUI while
testing Workstream C's live view. Workstream A (the capture-backend switch)
is functionally done, but left real residue behind, and a separate audit of
the face/personality code (triggered by "the face should be identical to the
pwnagotchi one") found real correctness gaps against that parity goal.

Tasks:
- AC1. Rename `<h2>Live activity (AngryOxide)</h2>` and its "waiting for
  AngryOxide…" strings in `crates/ui/web/templates/index.html:154-160` to
  something backend-neutral -- the panel is fed by real `StatusLine` events
  already, the label is just stale.
- AC2. Delete `crates/angryoxide/` -- confirmed fully orphaned (not a
  workspace member in root `Cargo.toml`, nothing depends on it).
- AC3. Rename `HealingAction::RestartAo` (`crates/agent/src/healing.rs:34`)
  to something that reflects what it actually does now (resets bettercap's
  wifi module via REST, not AngryOxide) -- and fix the matching `warn!` log
  string in `crates/agent/src/lib.rs`/`crates/pwnghost-rs/src/main.rs:795`
  that a user would see in `journalctl` output.
- AC4. Remove the angryoxide binary download/install from
  `pi-gen/stage4/00-install-artifacts/00-run.sh:49-75` and the presence
  check in `pi-gen/stage5/.../bootlog.sh:39-40` -- nothing spawns it anymore.
- AC5. **Personality/mood correctness, against the "identical to real
  pwnagotchi" goal**: `Personality::compute_mood()`
  (`crates/agent/src/personality.rs:328-368`) checks `Motivated` (peers
  nearby) *before* the blind-epoch negative-mood cascade, short-circuiting
  past Bored/Sad/Angry/Lonely entirely when peers are present -- real
  pwnagotchi's order is the reverse (already flagged as a gap in
  `SPEC.md:106-114`, not yet fixed). Separately, `lonely_num_epochs` (150)
  sits between `sad_num_epochs` (100) and `angry_num_epochs` (200) but the
  cascade checks worst-first (Angry → Sad → Lonely → Bored), so `Lonely`
  almost never actually fires in practice -- a latent ordering bug, not a
  config problem. Fix both to match real pwnagotchi's actual precedence.
- AC6. Unify the two parallel face-lookup systems: `crates/agent/src/faces.rs`
  (hardcoded, one kaomoji per mood, claims exact parity with upstream
  `faces.py`) vs `crates/agent/src/personality.rs`'s `FaceConfig` (config-
  driven, randomized among variants per mood, `personality.rs:395-426`).
  Having two sources of truth risks drifting from "identical to pwnagotchi"
  over time as one gets edited without the other. Decide which is
  authoritative (likely: config-driven variants seeded FROM the exact
  upstream strings, not a second hardcoded copy) and remove the other.
- Acceptance: no "AngryOxide" string reachable from a user-facing surface
  (WebUI, journal logs); `crates/angryoxide/` gone; mood cascade order and
  face lookup verified against upstream pwnagotchi source line-for-line, not
  just "looks about right."

Effort: Low (AC1-4) / Medium (AC5-6, needs upstream source diffing).
Impact: Medium (correctness/polish, not new capability) but directly
requested ("identical to the pwnagotchi one").

## Workstream B — Real TTF face/text rendering

**Status (2026-07-20): done.** `crates/ui/display/src/ttf.rs` uses `fontdue`
+ bundled `DejaVuSansMono-Bold.ttf`/`DejaVuSansMono.ttf`, matching real
pwnagotchi's `fonts.Huge` (35pt) exactly. Verified present this session; not
re-verified pixel-for-pixel against a real hardware screenshot.

**Why:** real pwnagotchi draws the face as **DejaVuSansMono-Bold at 35pt**
via PIL/FreeType (`ui/fonts.py:4,38`, `hw/waveshare2in13_V4.py::layout`
= `fonts.setup(10,9,10,35,25,9)`, `ui/components.py::Text.draw`). Ours blits
a 16px GNU Unifont bitmap upscaled 2× nearest-neighbor
(`crates/ui/display/src/layout.rs:521-580`, `fonts.rs:12-14`) — the blocky
look the user rightly rejected.

Tasks:
- B1. Add `fontdue` (pure-Rust software rasterizer, ARMv6-friendly); bundle
  `DejaVuSansMono-Bold.ttf` + `DejaVuSansMono.ttf` via `include_bytes!`.
- B2. New TTF text drawer in `crates/ui/display`: rasterize each glyph at the
  target px (face 35, body ~10), advance by real metrics, threshold coverage
  >127 into the 1bpp framebuffer. Glyph cache keyed by `(char, px)`.
- B3. Keep the Unifont atlas ONLY as a fallback for codepoints DejaVu lacks
  (e.g. `⚆` U+2686), rasterized to the same target height.
- B4. Delete `face_scale` + nearest-neighbor `blit_glyph` upscaling.
- B5. Refresh cadence tweaks to match "interactive fps=1": blink the name
  cursor every tick (not every 2 — `main.rs` `display_tick % 2`), and raise or
  idle-gate `FULL_REFRESH_EVERY=50` (`driver.rs:318`) so there's no periodic
  full-screen flash the original doesn't have. Keep the mandatory first-frame
  full refresh.
- Acceptance: face renders smooth and matches a real pwnagotchi screenshot at
  the same positions; body text legible; cursor blinks at 1Hz.

Effort: Medium. Impact: High (this is "make it look right").

## Workstream C — Web UI to match the original

**Why:** real pwnagotchi serves the **actual rendered e-ink frame as a PNG at
`/ui`, polled every 1s** (`ui/web/handler.py::ui()` → `send_file(frame_path)`,
`index.html` re-fetches `/ui?<ts>` each second). Ours has no live display
view, a config editor that silently wipes sections, no plugins page, and no
wpa-sec surface.

- **C1 (live view): done, verified this session** -- `/ui` serves the real
  framebuffer as PNG, `index.html` polls it, confirmed showing the same
  frame as the panel in a real browser test.
- **C2 (safe config editor): done** -- `GET /api/config` returns the whole
  `PwnConfig`; `POST /api/config` deep-merges via `apply_config_patch`
  (`crates/config/src/lib.rs`); a save/revert UI was added and verified
  end-to-end (edit → save → deep-merge → validate → atomic write to disk).
- **C2-follow-up (structured config editor): done, 2026-07-20.** Full
  redesign: tabbed single-page UI (Dashboard/Config/Plugins/Cracked),
  elevated terminal-green identity (per user design-direction choice).
  Config renders real per-section forms generated from the live JSON shape
  (toggles for bools, number inputs, nested groups) instead of one raw-JSON
  textarea; the raw-JSON editor is kept as a collapsed "advanced" escape
  hatch, not the primary path. Verified live in a real browser.
- **C3 (plugins page): done, 2026-07-20.** `GET /api/plugins` (name/enabled/
  options), `POST /api/plugins/<name>/toggle`, `POST /api/plugins/<name>/
  options` (per-plugin settings like wpa_sec's `api_key`/`api_url`,
  pwncrack's `wordlist`) all through the same safe deep-merge path. **The
  blocking bug is fixed**: `PluginManager::load_builtin_plugins` /
  `load_plugins` now actually check `config.plugins[name].enabled` before
  loading (`crates/agent/src/plugins.rs`) -- previously every built-in and
  every user plugin loaded unconditionally regardless of the flag. Verified
  live: toggling `pwncrack` off persisted and the WebUI reflected it after a
  server restart.
- **C4 (cracked passwords, both sources): done, 2026-07-20.**
  - C4a: `GET /api/wpa-sec/cracked` parses the wpa-sec potfile format
    (bssid:clientmac:ssid:password, first-3-colons-only split since
    passwords can contain `:`). `wpa_sec.lua` rewritten to upload the raw
    `.pcapng` (via a new `handshake_pcap_path` Lua global -- required a
    small Workstream-D-flavored plugin-host extension, `PluginManager::
    on_handshake` now passes both hash and pcap paths) instead of the
    `.hc22000` hash file, and to periodically download the account's
    potfile to disk for the UI to read.
  - C4b: `pwncrack.lua` now records duration/attack-mode/wordlist/hash-type
    per crack; `CrackedPassword` gained those fields (optional, so old
    `<bssid>.json` files still deserialize). The UI merges both sources
    client-side, deduped by normalized BSSID, each row tagged with its
    source pill(s).
  - **Real bug caught by the test written for this** (not shipped):
    `PluginConfig::options` is `#[serde(flatten)]`, so a naive patch nesting
    new option keys under a literal `"options"` key silently failed to
    merge -- caught by `test_update_plugin_options_merges_not_replaces`
    before it reached the browser, fixed by flattening the patch shape to
    match.
- Acceptance: all met -- live frame visible; editing config never loses
  sections; plugins toggle AND actually gate what loads (verified);
  wpa-sec key settable and cracked results shown from both sources, deduped
  (verified).

Effort: Medium (C3) / Medium-High (C4, C2-follow-up). Impact: High --
directly requested. **Status: Workstream C fully done, 2026-07-20.**

## Workstream D — Foundations: identity + plugin host API + hooks

Tasks:
- **D1: done, 2026-07-20.** `agent::identity::Identity` -- ed25519 (pure
  Rust via `ed25519-dalek`, no C/asm, same ARMv6 cross-compile rationale as
  the `fontdue` choice) keypair, generated once and persisted to
  `/var/lib/pwnghost/identity.key` (atomic temp+fsync+rename+dir-fsync
  write, mirroring `config::save_config`'s crash-safety discipline), with a
  stable `sha256(pubkey)` hex fingerprint. Wired into `main.rs` startup:
  logs the fingerprint on boot. Not yet consumed by grid/mesh (Workstream
  E2 is still an open, undecided integration) -- this is the identity
  primitive for that to build on later, not the integration itself.
  6 unit tests, including a same-seed-same-fingerprint determinism check
  and a stable-across-reloads check (the actual D1 acceptance bar).
- D2. Plugin host API: inject a real API table into each Lua plugin (`http`
  client, `config` object, `log` bridge, `ui` handle) instead of forcing
  `curl`/`io.popen` + hand-parsing `config.toml` (see `grid.lua`,
  `wpa_sec.lua`).
- D3. More plugin hooks: add `on_ui_update`, `on_webhook`,
  `on_internet_available`, and peer/mood/attack hooks (real pwnagotchi fires
  ~20; we fire 3 — `plugins.rs:164-212`). `on_webhook` needs web-route wiring
  (ties into Workstream C).
- Acceptance: a plugin can draw to the UI, serve a web route, and upload only
  when online; identity fingerprint stable across reboots.

Effort: Medium. Impact: High (unlocks the plugin ecosystem; prerequisite for
mesh).

## Workstream E — Revive the AI in Rust (differentiator) + decide mesh

**Decision made:** the AI is **kept and made real, in Rust** — not dropped.
This is a deliberate differentiator. Real pwnagotchi's `noai` branch *removed*
the RL because the Python implementation was too heavy/problematic on a Pi
Zero. Reimplementing it in Rust — where the compute and memory cost are
actually feasible on this hardware — is a core reason this project exists:
"pwnagotchi with a working brain, because it's not Python."

- E1. **Real RL in Rust.** Our `rl-agent` crate (A2C + bandit policy) already
  exists but is vestigial: features come from the always-empty `aps`/`peers`
  (`lib.rs:223-250` → all-zero histograms), reward is a flat `+1.0`/`-0.2`
  (`lib.rs:401/108`), and any Deauth/Associate it picks is discarded
  (`main.rs:686`). Phase 1 (bettercap) fixes all three preconditions at once:
  real observations, real reward signal (handshakes/APs/deauths actually
  happen), and actions that actually execute. Then:
  - Feed real AP/client/peer histograms + epoch stats into the feature vector.
  - Replace the flat reward with evilsocket's weighted reward (formula already
    transcribed in `SPEC.md`): reward handshakes/new-APs/associations, penalize
    missed epochs/reboots.
  - Let the policy's chosen Deauth/Associate/Hop actually drive bettercap.
  - Persist the learned policy across reboots (recovery layer already exists).
  - **Dependency: requires Phase 1.** Until the agent perceives and acts, the
    AI has nothing real to learn from. This is why the AI revival ships with
    or right after Phase 1, not before.
  - Effort: Medium (the crate exists; it needs real wiring, not a rewrite).
    Impact: High — it's the project's headline differentiator.
- E2. **Mesh/grid decision.** Our current mesh (`mesh.rs`, `grid.lua`)
  transmits/receives nothing and uses a non-pwnagotchi OUI. Options: (a) real
  opwngrid interop (needs a pwngrid-peer equivalent + D1 identity — High),
  (b) an honest custom mesh once we have a frame TX/RX path, or (c) drop it.
  **Decide, don't leave it a stub.** Lower priority than the AI revival.

Effort: Medium (E1) / High (E2a) or Low (E2c). Impact: High (E1).

## Workstream F — PiSugar S custom button (hardware)

**Status (2026-07-20): researched, not started.** User has the PiSugar S
battery module (not S Plus / 2 / 3). Confirmed via PiSugar's own docs: this
model reports **no battery percentage over I2C at all** (a hardware
limitation of this specific model, not something fixable in software) --
its only user-controllable input is a physical button.

Key hardware constraint: the button shares **GPIO3, which is also the
Raspberry Pi's I2C1 SCL pin**. PiSugar's own docs are explicit that the
board's "auto power-on" switch must be set OFF for the pin to behave as a
plain momentary-low button input without interfering with I2C1 (with it ON,
SCL is held low during external power presence for wake-on-power, which
would corrupt any real I2C1 traffic). This project doesn't currently use
I2C1 for anything else on this hardware, so the tradeoff should be safe, but
it's a physical switch on the board the user has to set, not something this
project's software can change.

This project already has a `gpio_buttons` Lua plugin (seen loading in a
local test run this session: `"gpioget not found on PATH, disabling
(no-op)"` -- i.e. it already shells out to `gpioget`, the modern libgpiod
CLI, which is the right tool here instead of legacy `/sys/class/gpio`
sysfs polling PiSugar's own reference script
(`pisugar-power-manager-rs/scripts/PiSugarSButtonActive.sh`) uses). The
low-effort path is extending/configuring this existing plugin for GPIO3
rather than building new GPIO infrastructure.

Tasks:
- **F1: done, 2026-07-20.** `gpio_buttons.lua` fully rewritten: configurable
  `[plugins.gpio_buttons] pin` (default 3, the PiSugar S button) and
  `long_press_secs` (default 3). Detection redesigned from the old
  once-per-epoch `gpioget` poll (which could miss short presses entirely
  between epochs -- 15s apart by default -- and couldn't measure hold
  duration at all) to a background watcher spawned once from `on_ready`:
  blocks on `gpiomon` waiting for the press (real interrupt-driven wait via
  the kernel gpio-cdev event fd, near-zero CPU while idle), then sleeps
  `long_press_secs` and re-checks the raw level to classify short vs long.
  Flag syntax targets libgpiod 1.6.x (`gpiomon`/`gpioget`'s CLI on this
  project's actual base image, bullseye32) -- **unverified on real
  hardware**, no Linux/GPIO access in this dev environment; confirm
  `gpiomon --help` matches on first real boot per this plan's own "never
  claim hardware-verified without on-device confirmation" rule.
- **F2: answered by user, 2026-07-20.** Short-press: **no binding** (see
  below -- the natural "cycle a display info screen" answer was rejected
  once it surfaced that it required new display infrastructure this
  project deliberately doesn't have; see `layout.rs`'s own doc comment
  against inventing screens/rows real pwnagotchi doesn't have). Long-press
  (held ≥ `long_press_secs`): **safe shutdown** -- the watcher itself calls
  `systemctl poweroff` (falling back to bare `poweroff`) directly, no
  round-trip through the agent's epoch loop needed since there's no other
  action to coordinate.
- Acceptance: met, pending real-hardware confirmation of the exact
  `gpiomon`/`gpioget` flag syntax. Documented instruction for the user to
  set the auto-power-on switch OFF lives in `gpio_buttons.lua`'s doc
  comment and `defaults.toml`'s `[plugins.gpio_buttons]` comment (the
  latter is reference-only, see the bug note below).

**Side discovery while wiring this up, fixed in the same pass**:
`crates/config/src/defaults.toml` is **not actually loaded anywhere** --
`config::load_config` builds its baseline entirely from `Serialized::
defaults(PwnConfig::default())` (schema.rs's Rust-side `Default` impls),
never parses the TOML file. That file's `[plugins.*].enabled` values (13
plugins documented as off-by-default, including the upload plugins
`wpa_sec`/`wigle`/`ohcapi` and every optional-hardware plugin) had silently
drifted out of sync with `schema.rs::default_plugins()`, which set **every
plugin to `enabled: true` unconditionally** -- meaning every fresh install
shipped upload-to-third-party-service plugins active with no credential set,
and every optional-hardware plugin (bt_tether/gps/memtemp/pisugarx/ups_lite/
webgpsmap, plus the new gpio_buttons) polling for hardware most installs
don't have. Fixed: `default_plugins()` now matches the documented intent (8
safe-by-default, 13 opt-in); `defaults.toml`'s header comment rewritten to
stop claiming it's loaded and explain it's hand-synced reference
documentation only. Two regression tests added
(`test_default_plugins_opt_in_ones_are_off`,
`test_default_gpio_buttons_targets_pisugar_s_button`).

Effort: Low-Medium (mostly plugin config, not new infra) + the unplanned
defaults.toml/default_plugins() fix (small, contained). Impact: Low-Medium
(hardware feature) + Medium (the defaults bug -- privacy-relevant, affects
every fresh install). **Status: done, 2026-07-20**, pending on-device
verification of the exact gpiomon/gpioget flags.

## Workstream G — Comprehensive real-hardware validation audit (2026-07-21)

Triggered by a real-hardware session that found and fixed a chain of live bugs
on a flashed Pi Zero 2W (wifi capturing zero real frames despite monitor mode
"succeeding," XP/level never moving despite real APs being found, the face
frozen on one expression, the WebUI's live-activity feed permanently stuck on
its placeholder). Once the wifi capture bug (see below) was fixed and real
data started flowing end-to-end for the first time, a full comparison audit
against jayofelony/pwnagotchi source was run across every subsystem to find
what else silently doesn't work now that real inputs actually exist.

### The root bug that unblocked everything else
`tools/rebase-jayofelony/overlay/usr/bin/monstart` used `iw dev wlan0 set
type monitor` (an in-place interface-type change via cfg80211's
`change_virtual_intf` path) instead of the real jayofelony `pwnlib`'s
`iw phy <phy> interface add wlan0mon type monitor` (creates a new virtual
interface via `add_virtual_intf`). On this nexmon-patched FullMAC chip
(BCM43430/43436), only the interface-add path actually triggers the
firmware-level monitor/promiscuous setup — the in-place type change just
relabels the interface at the kernel level with no firmware effect. Result:
interface existed, monitor mode reported success, channel hopping worked,
but `root tcpdump -i wlan0mon` showed exactly 0 packets, always, on every
board, across every earlier "fix" attempt (wifi regdomain, `iw` package,
etc. — all real but none of them were the actual cause). Fixed by rewriting
`monstart`/`monstop` to match `pwnlib`'s proven approach. Confirmed on real
hardware after the fix: 14 real APs, ~20k frames/3min via `wlan_keepalive`,
real client probe-request logging in bettercap.

### A recurring systemic pattern: implemented, tested, never wired up
Found **four separate times** across independently-audited subsystems this
session — worth calling out as a pattern, not four coincidences:
1. `Agent::add_or_update_ap` (new-AP detection) — existed, unit-tested,
   marked `#[allow(dead_code)]`, never called. `update_aps` did a blind
   list-replace instead. Result: `reward_new_ap` XP never fired no matter
   how many real APs were found. **Fixed.**
2. `WebSocketManager::broadcast_activity` — fully implemented, never called
   from anywhere. Result: WebUI's "Live activity" panel permanently showed
   its placeholder text regardless of real epochs/captures happening.
   **Fixed** (wired to new-AP-detected and handshake-captured events).
3. `pwncore::AccessPoint::is_target` (whitelist/blacklist filter) — fully
   implemented, unit-tested, never called. `Agent::find_target`
   (`crates/agent/src/lib.rs:306-322`), the sole target-selection function
   for both the heuristic and RL action-selection paths, filters only by
   channel/handshake-state/RSSI — never consults `main.whitelist`. **Not yet
   fixed.** Real security/safety consequence: an SSID/BSSID a user
   explicitly whitelists (e.g. their own home network) still gets
   deauthed/associated.
4. `PersonalityConfig::frame_padding`/`frame_padding_min_bytes` — flows
   through config into `Personality`, consumed nowhere. The one place it
   would logically apply (`MeshManager::build_mesh_ie`, this project's own
   identity-advertisement frame builder) has no padding logic at all.

**Process recommendation:** before calling a feature done, grep for the
implementing function's call sites outside its own test module. All four
instances above would have been caught by that one check.

### High severity (real behavioral/security gaps)

- **Whitelist unwired** (above) — `crates/agent/src/lib.rs:306-322`. Fix:
  call `is_target` in `find_target`.
- **Mesh/grid is non-functional by architecture, not just "unconfirmed."**
  Confirmed by call-site inspection: `MeshManager::build_mesh_ie` is never
  called from anywhere that would transmit it (no TX path into a real
  beacon/probe), and `update_peer` is never called with real data (no RX
  path parsing incoming vendor IEs) — `bettercap`'s REST API doesn't expose
  raw 802.11 management frames at all, only a processed AP/client list.
  Real pwnagotchi's actual mesh transport lives entirely inside a separate
  Go `pwngrid-peer` daemon (RSA identity, JWT enrollment, raw frame TX/RX)
  that this project's own `build.sh` deliberately strips and doesn't
  reimplement (already the documented Workstream E decision — this confirms
  the severity is "zero capability," not "partial"). Separately, `grid.lua`'s
  session-report target `api.opwngrid.com` is likely fabricated outright —
  real opwngrid is `api.opwngrid.xyz`, and the `/api/v1/report` path matches
  no real pwngrid-peer endpoint either. No action recommended beyond keeping
  Workstream E's framing (real interop vs. honest custom mesh vs. drop) —
  this just sharpens the "why."
- **`ups_lite.lua` targets the wrong I2C hardware entirely.** Real
  `ups_lite.py` (marbasec v1.3.0, the actual upstream plugin) talks to a
  **CW2015** fuel-gauge chip at I2C address **0x62**. This project's plugin
  targets **MAX17040 at 0x36** — a different chip at a different address.
  Our own code comment claiming 0x36/MAX17040 is "the original pwnagotchi
  ups_lite plugin" target is factually wrong. Will never respond on real UPS
  Lite hardware; permanent "battery read failed" logging is the only
  possible outcome as shipped.
- **`pwncrack.lua` implements a different feature under the real plugin's
  name.** Real `pwncrack.py` uploads pcaps to a remote `pwncrack.org`
  service and does no local cracking. This project's version does local
  hashcat dictionary attacks — a real, useful feature, but not what
  "pwncrack" means upstream; the in-file comment claiming parity is wrong.
  Not a bug in what it does, but mislabeled — worth a rename or a corrected
  comment so nobody expects remote-pwncrack.org behavior from it.
- **`[plugins.pwnstore_ui]` and `[plugins.webgpsmap]` are configurable but
  entirely unimplemented** — no `.lua` file, no entry in
  `PluginManager::BUILTINS` (`crates/agent/src/plugins.rs:91-111`). Enabling
  either from the WebUI's plugin toggle is a silent no-op.
- **WebUI `auth = true` is a complete no-op.** No middleware, no Basic-auth
  check exists anywhere in `crates/ui/web/src/{server,api}.rs`. Every route
  — including the new `/api/reboot` — is open regardless of the config
  setting. `auth = false` as a *default* correctly matches upstream; the gap
  is that the *on* setting does nothing, which is worse than an honest
  missing feature since it looks configured. Related: no CSRF protection on
  any state-changing route either (real pwnagotchi wraps its whole app in
  `CSRFProtect`).
- **`config::schema` fields using bare `#[serde(default)]` silently take the
  field type's default (false/0), not the struct's custom `Default` impl**
  (true/34/etc. as appropriate). This is the exact mechanism that produced
  the already-fixed `deauth=false`/`associate=false` deployed-config bug —
  but the underlying schema landmine is still live, so it will recur on any
  future config that omits those keys (a partial hand-edit, a config
  generated by an older schema version, etc.). Needs an explicit
  `default = "fn_name"` on every field whose correct default isn't its
  type's zero-value, not just the two fields already caught by hand.
- **`personality.min_rssi` default is `-80`; real pwnagotchi's is `-200`**
  (effectively unfiltered). `-80` silently drops distant/weak real targets
  from the moment bettercap's `set wifi.rssi.min` is issued.
- **`main.mon_max_blind_epochs` default is `5`; real is `50`.**
  `next_epoch`-equivalent logic restarts the whole agent once blind epochs
  reach this threshold — `5` is 10x more trigger-happy than upstream,
  meaningful restart-loop risk in an ordinary dead wifi zone.
- **`personality.max_misses_for_recon` is missing from the schema
  entirely.** Backs real pwnagotchi's `is_stale()` guard, which no-ops
  associate/deauth/channel-hop once a target's gone stale. No equivalent
  exists in this project at all.

### Medium severity

- `personality.bored_num_epochs`/`sad_num_epochs` defaults (`50`/`100`) vs
  real (`15`/`25`) — much less frequent mood swings than upstream.
- `personality.max_interactions` default `10` vs real `3` — lingers 3x
  longer on unproductive targets; also, like `frame_padding` above,
  `max_interactions`/`throttle` aren't actually consulted by `find_target`
  either, so the config value doesn't matter yet regardless of default.
- `personality.angry_num_epochs`/`lonely_num_epochs` are fabricated config
  surface — real pwnagotchi derives "angry"/"lonely" from a bond-encounter
  factor and `is_stale()`, not separate epoch-count thresholds.
- `personality.ap_ttl`/`sta_ttl` missing — real pwnagotchi sends these to
  bettercap to control seen-AP/station pruning; without them bettercap uses
  its own internal defaults, which may not match intent.
- `personality.throttle` (single field) vs real's `throttle_a`/`throttle_d`
  (separate post-association/post-deauth pause) — collapsed into one field
  whose consumer wasn't found at all in a schema.rs search; units/intent
  unclear.
- `bettercap.silence` (both deployed copies) is missing 6 of the 13 real
  event names (`ble.device.service.discovered`,
  `ble.device.characteristic.discovered`, `ble.device.disconnected`,
  `ble.device.connected`, `ble.connection.timeout`, `wifi.client.probe`) —
  more journal/log noise, not a capture-rate issue.
- `Agent::find_target` picks the *first* matching AP, not best-RSSI or
  least-recently-attacked, and has no cross-epoch backoff — an unproductive
  target gets retried every single epoch indefinitely. Real `automata.py`
  tracks per-BSSID interaction history to back off.
- WebUI plugins page only lists plugins with an explicit `[plugins.x]` config
  section (via a hardcoded client-side description map), not everything
  discovered on disk with real metadata/version/upgrade action like
  upstream's `plugins.html`. Also missing: a shutdown button and a
  manual/auto operating-mode toggle (both present in real pwnagotchi's UI;
  no "manual mode" concept exists in this project at all currently).
- RL reward shaping has 2 terms (handshake +1.0, blind-epoch −0.2) vs real
  pwnagotchi's 7-term blend (engagement, channel diversity, missed
  interactions, sustained mood, etc.) — the bandit can't distinguish
  "productive but risky" from "safe but idle" the way upstream's shaping
  can. Reasonable given the simpler bandit target (see below), but the
  shallowest part of the reward design.
- Real pwnagotchi's RL trains *personality parameters* (recon_time,
  min_rssi, etc.) which a heuristic layer then executes; this project's RL
  picks the tactical action (hop/deauth/associate/wait) directly every
  epoch, collapsing the two-layer design into one. Confirmed **not** a
  wiring bug — the full path (`select_action_rl` → `AgentAction` → real
  bettercap calls) is genuinely connected end to end, reward feedback
  included. A documented architectural simplification, not a defect.

### Low severity / confirmed non-issues (listed so they don't get
re-investigated)

- Neither project ships a pretrained RL model — both cold-start from zero
  on first boot. The task brief that assumed otherwise was wrong; verified
  against real pwnagotchi's own `ai/__init__.py::load()`.
- The bandit-vs-actor-critic gap (this project has both implemented but
  only uses the LSTM actor-critic if a checkpoint file exists on disk,
  otherwise falls back to a simpler bandit) is candidly documented in-code
  already and is an acceptable, working simplification.
- `wpa_sec.lua`'s upload format — previously flagged in an old code comment
  as possibly wrong — **is actually correct**, confirmed byte-for-byte
  against real `wpa_sec.py` (`files={'file': ...}` + `key=` cookie). The
  stale comment should be removed; missing pieces are whitelist filtering
  and internet-availability gating, not the wire format.
- `ohcapi.lua`'s upload payload shape matches the real V2 API exactly.
- Structured per-section config forms in the WebUI **exceed** upstream —
  real pwnagotchi has no built-in web config editor at all (only via a
  third-party plugin); this project's auto-generated forms are a genuine
  improvement, not a gap.
- The cracked-passwords/wpa-sec-potfile WebUI tab also exceeds upstream,
  which has no core equivalent.
- `pisugarx.lua`'s charging-bit detection matches real PiSugar3 code
  exactly; only the battery-percent formula differs (register-read vs
  upstream's voltage-curve derivation) — plausible per the PiSugar
  datasheet, not confirmed wrong.
- Most other plugins (`auto_backup`, `cache`, `fix_services`, `logtail`,
  `memtemp`, `session_stats`, `webcfg`, `wigle`) are honestly-scoped,
  simplified-but-functional ports, several with their own in-file comments
  already disclosing the simplification. Not urgent.

### Suggested priority (highest-impact first)
1. ~~Wire the whitelist into `find_target`~~ — **done.** Also had to correct
   `AccessPoint::is_target`'s own exclude-vs-allow-scope direction, which
   was backwards from real pwnagotchi (caught by a failing regression test
   that named the intended behavior directly).
2. ~~Fix the `#[serde(default)]` landmine at the type level~~ — **done**
   for `deauth`/`associate`/`personality.position_y`/`faces.position_y`;
   also corrected `faces.png`'s own struct-level default to `false`.
3. ~~Fix `ups_lite.lua`'s I2C target~~ — **done** (MAX17040@0x36 →
   CW2015@0x62). Byte-order for the word read is carried over from the
   pisugarx plugin's pattern, not yet confirmed on real UPS Lite hardware
   — flagged in-file.
4. ~~Fix `min_rssi`/`mon_max_blind_epochs` defaults~~ — **done**, all four
   copies (schema.rs, migrate.rs, defaults.toml, both deployed
   overlay/pi-gen configs).
5. Decide and act on `auth=true` being a no-op: either implement real
   Basic-auth middleware or remove the setting so it stops looking
   configured when it isn't.
6. Implement or remove `pwnstore_ui`/`webgpsmap` config sections (silent
   no-ops currently).
7. Everything else in High/Medium above, roughly in listed order, as time
   allows — none of it is currently blocking.

**Also fixed this round, found live on hardware rather than from the
audit:** a channel-hop absorbing-state bug in `rl-agent`'s bandit
exploration policy. `ap_histogram` only reflects channels the agent has
ever visited; the explore branch weighted its random channel pick by that
histogram with no floor, so once the agent happened to land on one channel
and see APs only there (nowhere else surveyed yet to compare against),
every subsequent explore roll landed right back on that same channel —
forever. Confirmed on real hardware: 15+ consecutive "Hopping to channel 1"
log lines with zero variation over a 3-minute session. Fixed with a
uniform exploration floor (`EXPLORE_FLOOR` in `rl-agent/src/policy.rs`) so
every channel always has some nonzero chance of being sampled regardless of
prior history.

### Round 2 — deeper audit: voice, filesystem, CLI/mode, systemd units, utilities

A second wave of 5 parallel audits, each starting from the real
jayofelony/pwnagotchi repo's actual file tree (not guessed paths) to cover
what Round 1 didn't touch.

**High severity:**

- ~~**No real "voice" (status-text) system exists.**~~ — **done.** Ported
  real `voice.py`'s phrase pools onto `Mood::voice_lines()`/
  `Mood::voice_line()` (`crates/pwncore/src/lib.rs`), each a
  `random`-picked pool per mood matching the real per-mood phrasing
  (Bored/Sad/Angry/Excited/Grateful/Lonely/Awake/Sleep/Look*/Motivated/
  Demotivated/Broken). `Agent::current_phrase` re-rolls only on a mood
  *transition* (mirrors real `automata.py` calling `Voice.on_X()` once per
  `set_X()` event, not every render tick — confirmed by reading
  `automata.py` directly), plus a direct celebratory override on handshake
  capture. Wired into both consumers that used to only have the old fixed
  `Personality::get_phrase` (now deleted): the e-ink display's status line
  (`main.rs`'s display-refresh tick) *and* the WebUI, which previously
  rendered the raw `Mood` enum name — `SessionResponse`/`StatusResponse`/
  `AppState` gained a `phrase` field, `LiveUpdate::Session`/`MoodChange`
  now carry it over the websocket, and `index.html` renders it under the
  face/mood card.

**Medium severity:**

- **No manual/auto mode fork exists** — confirmed a real, deliberate scope
  gap (not vestigial): real pwnagotchi's `--manual` flag is a complete
  behavioral fork (`do_manual_mode()` vs `do_auto_mode()` in the real
  CLI) that disables *all* autonomous recon/hop/deauth/associate, letting
  a human drive bettercap directly — a genuine safety/consent valve for
  demos, legal-compliance testing, or CTF use. This project has exactly
  one code path (always autonomous) and no way to disable offensive
  behavior short of stopping the whole service. `crates/ui/display/src/
  layout.rs:345-354` hardcodes the display's mode field to the literal
  string `"AUTO"` for exactly this reason. Also missing, lower-stakes CLI
  polish: `--wizard`, `--donate`, `--check-update`, `--clear`,
  `--print-config`, `-U/--user-config` (all one-shot utility branches in
  real pwnagotchi, not core-loop behavior).
- **`remove_whitelisted`-equivalent missing**: real pwnagotchi filters
  whitelisted SSIDs out of handshake lists before upload/display; no
  equivalent filtering was found anywhere in this project's capture/
  upload pipeline. Distinct from the `find_target` whitelist fix already
  made this session (that gates *targeting*; this would gate what gets
  *uploaded/shown* for an AP the agent observes passively, e.g. from a
  neighbor's traffic, without ever attacking it).
- **`total_unique_handshakes`-equivalent is architecturally different, not
  just missing**: real pwnagotchi recomputes its handshake count by
  globbing actual `.pcap` files on disk every time (always ground-truth).
  This project tracks a persisted in-memory counter
  (`agent::recovery::RecoveryState.total_handshakes`) that can drift from
  the real file count if files are deleted/added externally, or if
  recovery state is corrupted/lost. Worth a disk-rescan fallback or
  periodic reconciliation if this ever proves to matter in practice.
- **`iface_channels`-equivalent (query the adapter's actual supported
  channels) missing**: not found in `crates/radio` or `crates/bettercap`.
  If channel-hop validity is assumed/hardcoded (13 fixed channels, see
  `Agent::next_channel`) rather than queried from the real adapter, that
  could matter on hardware with a different supported channel set (e.g.
  5GHz-capable chips, or regulatory-restricted channels) — worth a direct
  check, not yet confirmed as a live bug.
- **Confirms a Round 1 finding with more detail**: no
  `on_internet_available`-equivalent gate exists anywhere in this
  project, and (new this round) it was also not found in real
  pwnagotchi's own `utils.py`/`agent.py`/`automata.py` directly — it must
  live elsewhere upstream (not yet located). Corroborates the plugin
  audit's finding that `wpa_sec.lua`/`ohcapi.lua` fire synchronously on
  handshake/epoch with no internet-availability gating.
- **`rsync-zram.timer`'s 60s cadence ignores `FsConfig`'s per-mount `sync`
  field** (log=60s vs data=3600s in config) — a single shared timer
  syncs both every 60s regardless, working against the `data` mount's
  intended lower-wear cadence. A 5th instance of the "config field exists,
  isn't actually consulted" pattern found this session (after
  `add_or_update_ap`, `broadcast_activity`, the whitelist, and
  `frame_padding`).
- **`pwnghost-rs.service`'s hardening doesn't match its own comment's
  claim**: the unit's comment says it deliberately mirrors real
  pwnagotchi's plain, unrestricted-root, `Restart=always` units, but the
  actual directives keep `NoNewPrivileges=yes`, an explicit
  `CapabilityBoundingSet=` (5 named caps, vs real pwnagotchi's
  unrestricted root/all-capabilities), and `Restart=on-failure` (not
  `always`). Likely intentional hardening from an earlier session, not an
  oversight, but the comment and the directives now disagree — needs a
  deliberate decision (keep hardened and fix the comment, or actually
  match upstream) rather than staying silently inconsistent.

**Low severity / confirmed non-issues:**

- Filesystem/SD-protection (zram-backed logs/data) is a faithful analog
  of real pwnagotchi's `fs.py` — same protected paths, same size/config
  intent, and this project's shutdown-flush is *more* robust than
  upstream's (a systemd `system-shutdown` hook covers external
  reboot/poweroff/watchdog paths that real pwnagotchi's app-internal
  `fs.mounts` flush loop would miss entirely).
- Systemd units for `wlan_keepalive`, `wifi-country`, `bootlog`,
  `safe-shutdown`, `bt-agent`/`bt-pan@` have no upstream equivalent at
  all (confirmed via full recursive tree search) — these are intentional
  custom additions, not parity gaps.
- `led`/`blink` boot-diagnostic helpers (real `utils.py`) are genuinely
  absent here — low-medium severity, cheap to port if headless boot
  diagnostics via the Pi's status LED ever matter.
- Most of real `utils.py`'s other functions (`DottedTomlEncoder`,
  `merge_config`, `download_file`, `unzip`, `md5`, `secs_to_hhmmss`,
  `WifiInfo`/`extract_from_pcap`) are Python-only plumbing or covered by
  an architecturally-reasonable substitute already (bettercap's own
  session data instead of re-parsing pcaps directly) — not gaps.

### Round 3 — deep audit: plugin host/runtime, bettercap client handling

Two more parallel, deeper-scoped audits, each specifically targeting one
subsystem the earlier rounds only surveyed at a shallow level.

**Plugin host/runtime:**

- **Hook surface is much smaller than real pwnagotchi's.** Only three
  hooks exist here (`on_ready`, `on_epoch`, `on_handshake`) vs. real
  pwnagotchi's much larger set (`on_loaded`, `on_config_changed`,
  `on_ui_setup`, `on_ui_update`, `on_wifi_update`, `on_association`,
  `on_deauthentication`, `on_peer_detected`/`on_peer_lost`,
  `on_internet_available`, `on_unread_inbox`, every mood-transition hook,
  `on_bcap_<tag>`). Not fixed this round — a larger scope than the two
  concrete bugs below, tracked here for a future pass.
- **Plugin execution is synchronous/blocking**, unlike real pwnagotchi's
  `PluginEventQueue` (each plugin gets its own thread/queue so one slow or
  hung plugin can't stall the main loop). Also not fixed this round —
  architectural, not a quick patch.
- ~~**`on_epoch` always received a freshly-zeroed `EpochState`, never the
  epoch that just finished.**~~ — **done.** Root cause:
  `EpochTracker::advance()` (`crates/agent/src/epoch.rs`) replaces
  `self.current` with the *next* epoch's zeroed state before returning,
  and the just-finished epoch's real counts (handshakes, deauths,
  associations, APs seen) only survive in `self.history.back()`. But
  `main.rs`'s `on_epoch` call site read `agent.epoch_tracker.current`
  directly — so every plugin's `on_epoch` hook saw all-zero data, every
  time, since real pwnagotchi's own `Automata.next_epoch()` has the exact
  same ordering problem and works around it with `self._epoch.epoch - 1`
  plus `self._epoch.data()` captured *before* `self._epoch.next()`. Fixed
  by adding `EpochTracker::last_completed() -> Option<&EpochState>`
  (returns `history.back()`) and changing the call site to pass
  `agent.total_epochs().saturating_sub(1)` and `last_completed()` instead
  — mirrors real pwnagotchi's own `epoch - 1` adjustment for the same
  underlying reason.

**Bettercap client handling:**

- **No websocket `/api/events` consumption** — bettercap perception is
  polling-only (`wifi_session()` every 3s in `main.rs`'s
  `bettercap_poll_interval`). Moderate severity, modest practical impact
  (3s is fast enough for this project's cadence); not fixed this round.
- **Asymmetric retry**: only the startup bootstrap sequence retries
  (10x/2s-flat); every subsequent `run_command` call (channel hop, deauth,
  associate, the Healer's soft-reset) is fire-once with no retry. Not
  fixed this round — a deliberate design decision either way, flagged for
  a future call.
- ~~**`ureq::Error::Status`'s `Display` doesn't capture the response
  body**~~ — **done.** bettercap puts its actual failure reason (e.g.
  "interface not in monitor mode") in the HTTP response body on non-2xx
  responses; ureq's `Error::Status(code, Response)` variant carries that
  body but its `Display`/`with_context` path never reads it, so only a
  generic "unexpected status code" ever reached the logs. Fixed in
  `crates/bettercap/src/client.rs`'s `run_command`/`wifi_session`: both
  now match `Err(ureq::Error::Status(code, resp))` explicitly and read
  `resp.into_string()` into the bailed error message. Also fixed in the
  same pass: `ApiResponse.message` was missing `#[serde(rename = "msg")]`
  — bettercap's Go struct (`api_rest_controller.go::APIResponse`) tags
  that field `json:"msg"`, so without the rename this silently stayed
  empty on every real response regardless of the body-capture fix above.
- ~~**`Healer::report_crash()`/`report_alive()` were fully implemented but
  never called from any real path**~~ — **done** (6th instance of the
  "config/feature exists, never wired up" pattern this session, after
  `add_or_update_ap`, `broadcast_activity`, the whitelist, and
  `frame_padding`/`rsync-zram.timer`'s cadence, both still open below).
  With nothing ever calling `report_crash`, the crash-window inside
  `Healer` never had anything in it, so `decide()` could never escalate
  past `HealingLayer::FwWatchdog` — the entire 6-layer self-healing state
  machine (soft-reset → GPIO power-cycle → safe mode → USB lifeline) was
  inert on real hardware no matter how badly bettercap was failing. Wired
  into `main.rs`'s existing `bettercap_poll_interval` tick (the natural
  "is the capture engine alive" heartbeat, already polling `wifi_session()`
  every 3s): `agent.report_alive()` on success, `agent.report_crash()` on
  either a bettercap-returned error or a panicked/joined blocking task.

### Round 4 — closing gaps that actually block/limit capture success

Direct read of real pwnagotchi's `pwnagotchi/agent.py` (fetched via `gh api
.../contents/pwnagotchi/agent.py`, not guessed) to check this project's
core recon/deauth/associate decision loop against the real thing, since
"the agent captures handshakes reliably" is the actual point of all of the
above.

- ~~**`find_target` could hand a clientless AP to the deauth branch,
  wasting that epoch's one deauth slot on a command that can't
  succeed.**~~ — **done.** Confirmed directly from bettercap's own Go
  source (`modules/wifi/wifi_deauth.go`'s `startDeauth`): `wifi.deauth
  <BSSID>` collects every currently-known client of that AP and deauths
  each one (so passing an AP's own MAC, as this project already did, is
  correct and even more efficient than real pwnagotchi's own per-station
  loop) -- but if the AP has *zero* detected clients, that same code path
  returns a hard error ("doesn't have detected clients") instead of a
  no-op. `find_target` never checked for that, so it could repeatedly
  offer a clientless AP as the deauth target while a client-bearing AP
  later in `self.aps` went untouched. Fixed: `find_target` now takes a
  `requires_clients: bool` (true for deauth, false for associate --
  associate needs no client, a PMKID can be pulled from the AP directly),
  and the heuristic/RL action-selection paths look for deauth and
  associate targets independently instead of sharing one lookup.
- ~~**`max_interactions` was fully configurable (schema, migrate,
  defaults.toml) but never consulted anywhere** — a 7th instance of the
  "config exists, never wired up" pattern.~~ — **done.** Real
  pwnagotchi's `Agent._history`/`_should_interact` caps how many times the
  same BSSID/station can be re-targeted before it's skipped, so one
  stubborn AP that never yields a handshake (WPA3-only, deauth-resistant
  client, wrong RSSI reading, etc.) can't monopolize every future
  deauth/associate slot forever. Added `Agent::interaction_history` (a
  per-BSSID counter), consulted by `find_target` and incremented in
  `tick()` whenever a `Deauth`/`Associate` action is actually chosen.
- ~~**Bettercap bootstrap never applied `wifi.rssi.min`**~~ — **done**
  (small, matches real pwnagotchi's own `_reset_wifi_settings()`): both
  the startup bootstrap and the Healer's soft-reset re-bootstrap now `set
  wifi.rssi.min` to `personality.min_rssi` so bettercap itself stops
  reporting APs weaker than the configured floor, instead of only
  filtering them out after the fact in `find_target`.
- ~~**`BanditPolicy`'s explore branch could only ever produce a
  `HopChannel` action, never `Deauth`/`Associate`/`Wait`.**~~ — **done,
  and a genuinely serious finding**: fetched real pwnagotchi's actual
  gameplay loop (`bin/pwnagotchi`'s `do_auto_mode`, not guessed) to check
  this project's decision loop against it, and traced `Agent::select_action`
  through to confirm which policy actually drives it. `agent.rl_agent` is
  *always* populated (`main.rs` calls `rl_agent::init_agent` unconditionally
  at startup), and `select_action_rl` is tried before the heuristic
  fallback -- so on any device without a trained model on disk (i.e. every
  device, since none ships), `BanditPolicy` is the *sole* decision-maker,
  not a fallback in name only. Its explore branch (taken with probability
  `epsilon`, which *starts at 1.0* -- pure exploration) added a nonzero
  floor to every channel's weight (this session's earlier absorbing-state
  fix, above) before checking `total > 0.0` to decide whether to hop or
  fall through to a uniform pick over the whole action space -- but the
  floor made that check unconditionally true, so it *always* took the
  "pick a channel to hop to" path and could never fall through to
  Deauth/Associate/Wait. Net effect: a freshly-flashed device (or any
  device whose bandit state didn't survive a reboot) would channel-hop
  forever and never attempt a single deauth or association -- this bug
  would fully explain poor/zero capture rates independent of every fix
  already made this session (monitor mode, deauth/associate defaults,
  whitelist, client-requirement, max_interactions, etc. -- all necessary
  but insufficient if the policy driving the whole loop never asks for the
  action in the first place). Fixed: explore now first picks *which kind*
  of action to try uniformly over the full action space, and only re-rolls
  with the histogram-weighted channel logic if that pick landed in the
  "hop" family -- preserving the earlier absorbing-state fix while
  actually letting Deauth/Associate/Wait be explored (~3/16 of explore
  rolls, given `action_dim=16`). Regression test added
  (`test_bandit_explore_can_still_pick_deauth_or_associate`) alongside the
  existing absorbing-state test to guard both properties simultaneously.
- **Not done, lower priority / bigger scope**: real pwnagotchi's
  `ap_ttl`/`sta_ttl` personality settings (`set wifi.ap.ttl`/`wifi.sta.ttl`
  on bettercap, controlling how long it remembers stale APs/stations) have
  no equivalent config field or wiring here at all -- bettercap's own stock
  TTL defaults apply instead. Also confirmed but out of scope for this
  round: real pwnagotchi's `recon()` sets a *list* of channels (or clears
  to let bettercap free-hop across everything) and lets bettercap's own
  internal hop timer drive channel-switching, whereas this project issues
  one `wifi.recon.channel <N>` command per hop from its own single-channel
  schedule -- a different but not incorrect architecture (this project's
  own `Agent::next_channel`/`calc_hop_time` already drives the same
  end result), not something this round changed.

## Boundaries

- **Always:** run `cargo test --workspace` before deploying; verify the
  deployed binary checksum on-device; keep the crash-healing/firmware-recovery
  layer intact (it's a genuine strength); rate-limit deauth on nexmon.
- **Ask first:** dropping the RL crate or the mesh subsystem; adding an
  external USB adapter requirement; any change that alters the flashed image's
  interface model (`wlan0` vs `wlan0mon`).
- **Never:** claim a fix is hardware-verified without on-device confirmation;
  ship a config-save path that can silently wipe sections; enable heavy
  injection that risks bricking firmware without a rate limit.

## Phased delivery

Each phase is independently shippable (builds, tests, flashes, and is
verifiable on hardware) and must **not regress** the two non-negotiables:
performance (~120 MB RSS, no runtime stalls) and SD-card safety (zram logs,
tmpfs, hardened unit). Those are checked at every phase gate, not assumed.

### Phase 0 — Baseline guardrails (carry-forward, verify only)
Already in the image; this phase just confirms they survive the rework.
- zram-backed logs, `/tmp` tmpfs, hardened `pwnghost-rs.service`,
  `e2fsck`-on-build, the crash-healer + recovery persistence.
- Gate: on-device `systemctl` + `free` + log-location checks confirm all
  present after each later phase's deploy.

### Phase 1 — Capture that works + agent that perceives (Workstream A) — ✅ DONE
The keystone. bettercap replaces AngryOxide: reliable capture on the nexmon
chip, real AP/client observations feeding the agent, real deauth/assoc/hop.
- Gate: real APs in UI, real handshakes captured, deauth issued and firmware
  survives, RSS still ~unchanged (bettercap is a separate Go process exactly
  as in real pwnagotchi; the Rust agent stays lean), SD-card guardrails
  intact. Deauth rate-limited to protect nexmon firmware.
- **Residue found 2026-07-20, not yet cleaned up — see Phase 1.5 below.**

### Phase 1.5 — AngryOxide residue + face/personality parity fixes (Workstream A-cleanup) — ✅ DONE
Rename stale "AngryOxide" strings (WebUI + logs), delete the orphaned
`crates/angryoxide/`, rename `HealingAction::RestartAo`, remove the unused
angryoxide binary from the pi-gen image, fix two real mood-cascade
correctness bugs, and unify the two parallel face-lookup tables.
- Gate: no "AngryOxide" string reachable from any user-facing surface;
  mood-cascade order and face strings verified against upstream pwnagotchi
  source line-for-line.
- Independent of everything else — can be done any time, low risk.

### Phase 2 — Face/display fidelity (Workstream B) — ✅ DONE
Real TTF rendering (fontdue + DejaVuSansMono-Bold @35pt, Unifont fallback),
cursor blink at 1 Hz, no periodic full-refresh flash.
- Gate: face matches a real pwnagotchi screenshot; glyph cache keeps per-tick
  CPU negligible; no RSS regression; runs entirely in the existing 1 s
  display tick (no new loop).
- Independent of Phase 1 — can be built in parallel.

### Phase 3 — Web UI to match the original (Workstream C) — ✅ DONE
C1 (live e-ink PNG), C2 (safe deep-merge config editor), C2-follow-up
(structured per-section forms, tabbed redesign), C3 (plugins page with a
real gating fix), and C4 (wpa-sec potfile view + fixed pcap upload,
alongside deduped local hashcat) are all done and verified in a real
browser, 2026-07-20.
- Gate: met -- plugin toggles persist AND actually gate what loads; wpa-sec
  key settable and cracked results shown from both sources, deduped; config
  editor has real per-section forms.

### Phase 4 — Foundations (Workstream D)
Persistent identity keypair; real plugin host API (http/config/log/ui);
missing plugin hooks (on_ui_update, on_webhook, on_internet_available, peer/
mood/attack). Unlocks the plugin ecosystem and is the prerequisite for mesh
AND for C4's wpa-sec plugin config to go through the same safe-merge path
as everything else.
- Gate: identity fingerprint stable across reboots; a plugin can draw to UI +
  serve a web route + upload only when online; no RSS regression.

### Phase 5 — Mesh + RL decisions (Workstream E)
Explicit decisions once Phase 1 gives real inputs: real opwngrid interop vs
honest custom mesh vs drop; RL-with-real-inputs vs drop-to-match-`noai`.
- Gate: whatever is kept is functional on real inputs; whatever is dropped is
  removed cleanly (no half-alive stubs).
- Lower priority than Phase 3/4 right now — not the current focus.

### Phase 6 — PiSugar S custom button (Workstream F) — ✅ DONE
`gpio_buttons.lua` extended for GPIO3 with a real background-watcher
detection design (not new Rust infra). Bindings decided: short-press
unbound (rejected the "cycle info screen" idea once it required new
display infrastructure this project deliberately avoids), long-press =
safe shutdown.
- Gate: met -- button press detection design is sound; user still needs to
  physically set the board's auto-power-on switch OFF (documented, not
  software-fixable) and this session couldn't verify the exact
  `gpiomon`/`gpioget` flag syntax on real hardware (no Linux/GPIO access in
  this dev environment) -- confirm on first real boot.
- Independent of everything else — standalone hardware feature.

**Dependencies:** 1 and 2 are independent (parallelizable), both done. 1.5 is
independent, low-risk, done. 3 needed only a small, targeted Workstream-D
touch (threading `handshake_pcap_path` through `PluginManager::on_handshake`
for C4a) rather than the full D2/D3 host-API rebuild -- done without it. 6 is
fully independent.

**Status as of 2026-07-20 (this session)**: Phases 1, 1.5, 2, 3, and 6 are
all ✅ done and verified (build + full test suite + live browser checks for
3; build + test for 6, real-hardware GPIO flag confirmation still
outstanding). Also fixed in passing: `default_plugins()` had silently
drifted from `defaults.toml`'s documented intent to enabling every plugin
unconditionally (upload plugins + optional-hardware plugins included) --
see Workstream F's "side discovery" note. Full Workstream D (D1 identity,
D2 real plugin host API, D3 more hooks) remains undone but is no longer
blocking anything -- it's optional depth, not a prerequisite, unless a
future feature needs it. Phase 5 (mesh/RL) remains deliberately
deprioritized, lightly-touch-only per explicit user direction.

**Suggested sequence, updated:** Phases 1/2/3/6 done this session. What's
left -- Phase 4 (plugin host API depth) and Phase 5 (mesh/RL) -- are both
explicitly lower priority; pick either as time allows, not urgent.

### Phase 7 — Real-hardware validation audit (Workstream G) — 🔶 IN PROGRESS
First real-hardware session where Phase 1's wifi capture actually worked
end to end (root cause: `monstart`/`monstop` used the wrong `iw`
interface-creation path for this nexmon FullMAC chip — fixed) surfaced a
chain of live bugs only visible once real data started flowing: dead
new-AP/XP wiring, a frozen mood/face, a dead WebUI activity feed, and
passive-only `deauth`/`associate` defaults (all **fixed** this session). A
follow-up comprehensive audit against jayofelony/pwnagotchi source across
every subsystem (config schema, plugins, WebUI, RL/AI, bettercap/recon
targeting, mesh/grid) found a longer list — see Workstream G above for full
detail and file:line citations.
- **Fixed this session:** `monstart`/`monstop` iw-invocation bug, wifi
  regdomain verification, `iface` config drift (wlan0→wlan0mon, both
  deployed copies), `deauth`/`associate` defaults (false→true, both
  deployed copies), dead `add_or_update_ap`/`update_aps` AP-reward wiring,
  frozen Recon-mode mood (now alternates LookR/LookL), dead
  `broadcast_activity` WebUI wiring, plus infra fixes (overclock config,
  WebUI reboot button, replacement MOTD, GH Actions CI coverage for the
  rebase-jayofelony pipeline).
- **Fixed in a follow-up round, same session:** whitelist now wired into
  `find_target` (also corrected `AccessPoint::is_target`'s own logic, which
  had the exclude-vs-allow-scope direction backwards from real
  pwnagotchi -- both fixed together, regression tests on both); the
  `#[serde(default)]` schema landmine on `deauth`/`associate`/
  `personality.position_y`/`faces.position_y` (explicit default fns now);
  `faces.png`'s own struct-level default corrected to `false` to match
  real pwnagotchi (was the odd one out, not the deployed config);
  `ups_lite.lua`'s I2C target (MAX17040@0x36 → CW2015@0x62, all four
  deployed/schema copies); `min_rssi` (-80→-200) and
  `mon_max_blind_epochs` (5→50) defaults (schema.rs, migrate.rs,
  defaults.toml, both deployed overlay/pi-gen copies). Also found and
  fixed live on hardware during this round, not from the audit: a real
  channel-hop absorbing-state bug in `rl-agent`'s bandit exploration
  policy -- `ap_histogram` only reflects channels ever visited, so once
  the agent landed on one channel and found APs only there, every
  subsequent explore roll landed back on that same channel forever (a
  self-reinforcing trap, not a preference); fixed with a uniform
  exploration floor so every channel always has some chance of being
  sampled.
- **Round 2 audit (voice, filesystem, CLI/mode, systemd units, general
  utilities)** found the single biggest remaining gap: there is
  effectively **no real "voice"/status-text system** at all (one fixed
  caption per mood, no variety, not even wired to the WebUI) — see
  Workstream G's "Round 2" subsection for this plus a manual/auto-mode
  safety-valve gap, a 5th instance of the config-not-wired-up pattern
  (`rsync-zram.timer`'s cadence), and several lower-severity findings.
- **Round 3 audit (plugin host/runtime, bettercap client handling)** found
  two more real bugs (both now fixed — see Workstream G's "Round 3"
  subsection): `on_epoch` always received a freshly-zeroed `EpochState`
  instead of the epoch that just finished, and `Healer::report_crash()`/
  `report_alive()` — a 6th instance of the "implemented, never wired up"
  pattern — meant the entire 6-layer self-healing state machine was inert
  on real hardware. Also fixed in the same pass: `ureq::Error::Status`
  discarding bettercap's actual error-response body from logs, and
  `ApiResponse.message` missing the `#[serde(rename = "msg")]` needed to
  actually deserialize bettercap's real field name. Still open from Round
  3: the smaller plugin hook surface (3 hooks vs. real pwnagotchi's much
  larger set) and synchronous/blocking plugin execution (both
  architectural, not quick patches), plus no bettercap websocket event
  stream and asymmetric command retry (both flagged as deliberate-tradeoff
  candidates, not urgent).
- **Fixed this round (voice system + Round 3 bugs):** `Mood::voice_lines()`/
  `voice_line()` (real `voice.py`-ported phrase pools) and
  `Agent::current_phrase()` (mood-transition-gated, real handshake-capture
  override), wired into both the e-ink display and the WebUI
  (`phrase` field added to `SessionResponse`/`StatusResponse`/`AppState`/
  `LiveUpdate::Session`/`LiveUpdate::MoodChange`, rendered in
  `index.html`); `EpochTracker::last_completed()` fixing `on_epoch`'s
  always-zeroed data; `Healer::report_crash`/`report_alive` wired into
  `main.rs`'s bettercap poll-interval heartbeat; `bettercap::client`'s
  `ureq::Error::Status` body-capture and `ApiResponse`'s `msg` rename.
- **Not yet fixed, highest priority first:** no manual/auto mode fork
  (Medium, Round 2, real safety/consent gap), WebUI `auth=true` no-op
  (needs real middleware or removing the setting), `pwnstore_ui`/
  `webgpsmap` unimplemented-but-configurable plugins, `frame_padding`
  orphaned config (4th instance of the wiring-bug pattern),
  `rsync-zram.timer` ignoring per-mount `sync` cadence (5th instance),
  `pwncrack.lua` mislabeled (implements a different feature than the real
  plugin of that name), the smaller plugin hook surface and
  synchronous/blocking plugin execution (Round 3). Full detail in
  Workstream G.
- Gate: each fix verified against real hardware where the original bug was
  found on real hardware, not just unit tests (this entire workstream
  exists because unit-tested code was still wrong in practice four separate
  times). Voice system and Round 3 fixes verified so far by
  `cargo build`/`test`/`clippy --workspace` only — not yet flashed to a
  real board this round.
- Not independent of Phase 5 in one place: mesh/grid's audit finding
  (non-functional by architecture, confirmed by call-site inspection) is
  now evidence for whichever Phase 5 decision gets made, not a new decision
  point itself.
