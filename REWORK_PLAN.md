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
