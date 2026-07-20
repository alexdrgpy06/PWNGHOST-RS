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

## Workstream B — Real TTF face/text rendering

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

Tasks:
- C1. Live view: encode the existing display framebuffer
  (`crates/ui/display/src/lib.rs:22`) to PNG; serve `GET /ui`; add `<img>` +
  1s cache-busting poll (or WS "frame updated" ping) to `index.html`.
  - Acceptance: browser shows the same frame as the panel, updating ~1s.
- C2. Safe config editor: fix GET to return the whole `PwnConfig` (currently
  omits `bettercap`/`fs`/`oxigotchi`/`plugins` — `api.rs:39-53`); change POST
  to a **deep-merge patch** instead of whole-object replace (currently a
  data-loss trap — `api.rs:55-76`). Add a `merge_json` helper + tests proving
  an omitted `[plugins]` section doesn't wipe an existing `api_key`.
- C3. Plugins page: `GET /api/plugins` (name/enabled/builtin), `POST
  /api/plugins/<name>/toggle` writing `config.plugins[name].enabled` via the
  safe-merge path; UI section with toggles.
- C4. wpa-sec surface: config keys editable (via C2/C3) + a cracked-passwords
  view (`GET /api/wpa-sec/cracked` reading the potfile) rendered as a table.
- Acceptance: live frame visible; editing config never loses sections;
  plugins toggle; wpa-sec key settable and cracked results shown.

Effort: Medium. Impact: High.

## Workstream D — Foundations: identity + plugin host API + hooks

Tasks:
- D1. Persistent identity: generate+persist an ed25519 (or RSA) keypair under
  `/var/lib/pwnghost`, derive a `sha256(pubkey)` fingerprint. Real pwnagotchi
  uses this for grid identity and stable naming (`identity.py`). We have none.
  Cheap, self-contained early win.
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

### Phase 1 — Capture that works + agent that perceives (Workstream A)
The keystone. bettercap replaces AngryOxide: reliable capture on the nexmon
chip, real AP/client observations feeding the agent, real deauth/assoc/hop.
- Gate: real APs in UI, real handshakes captured, deauth issued and firmware
  survives, RSS still ~unchanged (bettercap is a separate Go process exactly
  as in real pwnagotchi; the Rust agent stays lean), SD-card guardrails
  intact. Deauth rate-limited to protect nexmon firmware.

### Phase 2 — Face/display fidelity (Workstream B)
Real TTF rendering (fontdue + DejaVuSansMono-Bold @35pt, Unifont fallback),
cursor blink at 1 Hz, no periodic full-refresh flash.
- Gate: face matches a real pwnagotchi screenshot; glyph cache keeps per-tick
  CPU negligible; no RSS regression; runs entirely in the existing 1 s
  display tick (no new loop).
- Independent of Phase 1 — can be built in parallel.

### Phase 3 — Web UI to match the original (Workstream C)
Live e-ink PNG at `/ui`, safe deep-merge config editor, plugins page,
wpa-sec surface.
- Gate: live frame visible in browser; config edits never drop sections
  (regression test); plugin toggles persist; PNG encode is cheap (1bpp
  250×122, encoded on the existing display tick). Depends on Phase 2 for the
  rendered frame.

### Phase 4 — Foundations (Workstream D)
Persistent identity keypair; real plugin host API (http/config/log/ui);
missing plugin hooks (on_ui_update, on_webhook, on_internet_available, peer/
mood/attack). Unlocks the plugin ecosystem and is the prerequisite for mesh.
- Gate: identity fingerprint stable across reboots; a plugin can draw to UI +
  serve a web route + upload only when online; no RSS regression.

### Phase 5 — Mesh + RL decisions (Workstream E)
Explicit decisions once Phase 1 gives real inputs: real opwngrid interop vs
honest custom mesh vs drop; RL-with-real-inputs vs drop-to-match-`noai`.
- Gate: whatever is kept is functional on real inputs; whatever is dropped is
  removed cleanly (no half-alive stubs).

**Dependencies:** 1 and 2 are independent (parallelizable). 3 depends on 2.
4 is mostly independent but its `on_webhook` ties into 3. 5 depends on 1.

**Suggested sequence:** Phase 1 + Phase 2 in parallel (capture + look, the
two things the user directly called out) → Phase 3 → Phase 4 → Phase 5.
