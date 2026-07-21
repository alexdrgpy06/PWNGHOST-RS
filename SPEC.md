# Spec: PWNGHOST-RS — Our Own Rust Pwnagotchi

## Objective

Build a **complete, flashable SD card image** for:
- **Raspberry Pi Zero W** (ARMv6, BCM43430/1, 32-bit)
- **Raspberry Pi Zero 2W** (ARMv7/ARMv8, BCM43436B0, 32-bit userland)

That replaces the Python pwnagotchi with a **pure Rust implementation**. This
section describes the *original* design intent (AngryOxide as the WiFi
engine); Phase 1 of the rework (see `REWORK_PLAN.md`, the current, living
plan) switched the capture backend to **bettercap** after discovering
AngryOxide cannot capture at all on this hardware's FullMAC chip -- the
"What We Take from Each Repo" table below is kept as historical
design-provenance record, not a description of current behavior. The image
must:
- Be a drop-in replacement for jayofelony's pwnagotchi (bookworm, 32-bit userland)
- Use **bettercap** (the base jayofelony image's own capture engine) for WiFi
  recon, handshake capture, and deauth/assoc attacks -- see `REWORK_PLAN.md`
  Workstream A for why AngryOxide, the original plan here, was replaced
- Include **CoderFX's BCM43436B0 firmware stability patches** (8 layers) + userspace keepalive daemon
- **Port the A2C RL agent (LSTM + MLP policy)** to Rust for intelligent channel selection and attack decisions
- Support **Waveshare e-ink displays** (SSD1306, 2.13"/2.7"/2.9" V4) + **web UI** (port 8080)
- Use **TOML config** compatible with existing `config.toml` / `defaults.toml`
- Run as a single `pwnghost-rs` systemd service (no Python) alongside
  bettercap as its own systemd unit (the capture engine, per the Phase 1
  pivot noted above -- not spawned/owned by `pwnghost-rs` itself)
- Include **Lua plugin system** (via `mlua`) for extensibility
- Feature **classic pwnagotchi faces** (kaomoji mood system) enhanced with better rendering
- Implement **6-layer self-healing** (firmware watchdog, crash detection, AO watchdog, GPIO power cycle, graceful give-up, USB lifeline)
- Use **tmpfs capture pipeline** (staging → validated `.22000` + `.pcapng`) for SD card longevity
- **Bluetooth mode** with auto tethering/pairing for internet access (WPA-SEC upload, updates, SSH)

---

## What We Take from Each Repo (Technical Substance)

| Area | Source | We Adopt |
|------|--------|----------|
| AngryOxide integration | oxigotchi (AoManager) + pwnpwn (parser) | ✓. Spawn subprocess, parse JSON stdout |
| Firmware patching | pwnpwn (fw-patcher) | ✓. CoderFX 8-layer logic in Rust (`fw-patcher` crate) |
| Firmware monitoring | oxigotchi (firmware.rs) | ✓. SDIO RAMRW netlink, crash counter monitoring |
| Self-healing | pwnpwn (healing.rs) | ✓. 6-layer state machine, full adoption |
| Capture pipeline | oxigotchi (capture.rs) | ✓. tmpfs → validated `.22000` + `.pcapng` |
| Lua plugins | oxigotchi (lua/mod.rs) | ✓. `mlua` + ported plugins |
| WiFi keepalive | oxigotchi (`wlan_keepalive`) | ✓. Embedded in fw-patcher |
| Boot architecture | oxigotchi (~30s boot) | ✓. Same goal |
| Bluetooth | oxigotchi (BT mode, PAN tethering) | ✓. BT mode for tethering (attacks v2) |
| Epoch loop / personality | pwnagotchizero32 (epoch.rs) | ✓. Refined for classic pwnagotchi mood automata |
| Display rendering | pwnagotchizero32 (display.rs) | ✓. TTF-rendered kaomoji faces |
| Radio modes | oxigotchi (RAGE/BT/SAFE) | ✓. 3-mode atomic switching |
| Config loading | pwnagotchizero32 (config/mod.rs) | ✓. figment + conf.d + env override |
| Core domain types | pwnpwn (pwncore) | ✓. Modular: ap, channel, epoch, mood, peer, personality, station |
| RL Agent | pwnpwn (rl-agent) | ✓. Policy trait, heuristic fallback, feature extraction |

---

## What We Do Differently (Our Own Style — Not "Vibecoded Oxi")

| Area | Oxigotchi / Others | **PWNGHOST-RS** |
|------|-------------------|-----------------|
| **Faces** | 28 bull faces, XP/leveling, "Mooooood" | **Classic pwnagotchi kaomoji** (21 moods) — enhanced rendering, pre-rasterized |
| **Mood system** | Bull mood (RF-driven) | **Classic pwnagotchi automata** (epoch-based: lonely→bored→sad→angry, excited/grateful on activity) |
| **Modes** | RAGE / BT / SAFE (3-mode radio) | **RAGE (WiFi) + BT (tether) + SAFE (managed)** — 3 modes, radio switching |
| **Personality** | Aggression levels | **pwnagotchi personality.toml** params (bond_encounters_factor, max_interactions, throttle, etc.) |
| **Mesh/peers** | Custom 802.11 IE | **pwnagotchi mesh protocol** (compatible with existing units) |
| **Config** | Oxigotchi TOML | **pwnagotchi-compatible TOML** + migrations |
| **Architecture** | Monolithic main.rs (185KB) | **Clean workspace**: 12 crates, clear boundaries, ≤5 files per task |
| **RL/AI** | Not implemented | **burn (training) + candle (inference)** — pure Rust DL |
| **Testing** | Minimal | **TDD mandated** — unit, property, integration, hardware gates |

---

## Fidelity vs Reference Implementations (Research Findings, 2026-07-18)

Prompted by a legitimate concern: our compressed image (~475MB) is much smaller than
evilsocket/jayofelony images (1.1-2GB+), and our RL is a bandit, not evilsocket's A2C+LSTM. Before
continuing, we audited six reference repos (evilsocket/pwnagotchi, jayofelony/pwnagotchi,
CoderFX/oxigotchi, pineapple_pager_pagergotchi, Pwnagotchi-For-Banana-Orange-Pi,
Fancygotchi-cyd-port) to check whether small size or a simpler RL model means we're missing
functionality, or whether it's expected variance. Findings, so this isn't re-litigated later:

**Image size is not a red flag.** evilsocket's original: ~1.9-2.05GB compressed. jayofelony's fork:
~1.14-1.20GB for most of the last year, jumping to ~1.47-1.50GB only in the latest release (heavier
Trixie base, unrelated to AI). oxigotchi (pure Rust, zero AI) is still ~1.72GB compressed. Every
comparable project sits at 1.1GB+, and oxigotchi proves the size gap has nothing to do with missing
AI — it has none and is still 1.72GB. The actual drivers of reference-image size: full Kali/Buster/
Trixie base OS, broad multi-chipset firmware (`firmware-atheros`/`libertas`/`realtek` etc. for
hardware this project will never run on), unstripped build toolchains left on the image
(jayofelony's image ships both Rust and Go toolchains plus cloned bettercap/pwngrid source+vendor
trees after `make install`), and (upstream only) a heavy TF1/OpenCV/SciPy/stable-baselines Python ML
stack. None of that is present or needed in a single static Rust binary + a minimal bookworm base.

**A2C/LSTM RL is not the baseline "real" implementation to match — it's already been dropped
everywhere except one repo.** jayofelony's fork, which is what real users actually run today, has
completely removed RL/AI: no TensorFlow, no model, `pyproject.toml` explicitly strips
`numpy`/`gast`/`shimmy` with the comment "were AI training pipeline dependencies only", and
`grid.py` hardcodes `'ai': "No AI!"` to the mesh network. Same pattern in oxigotchi (no AI), the
Pineapple Pager port (AI "removed... just a simple reward calculation"), and the CYD ESP32 port
(no AI, pure threshold-based mood). Only the Banana/Orange Pi port (a direct evilsocket fork on a
full Linux SBC) keeps the original TF1/A2C training intact. Conclusion: RL/AI is the first thing
dropped under any resource or scope pressure, and our `BanditPolicy` (genuine online epsilon-greedy
learning, real Q-value updates from real reward signal) is more sophisticated than 4 of these 5
non-upstream implementations. evilsocket's *original* RL (for the record, since it's the one
credible baseline): A2C via `stable_baselines`(TF1) + `MlpLstmPolicy`, trained **online, on-device**
(not offline, correcting an earlier assumption this session) — observation = AP/STA/peer channel
histograms + 8 epoch-scalar ratios (a similar shape to our own 49-dim `Features` struct), actions =
13 personality params + per-channel hop decision (broader than our current channel/attack-only
action space), reward = a weighted sum: `+handshake_ratio (dominant) + 0.2*active_ratio +
0.1*hop_ratio - 0.3*blind_ratio - 0.3*missed_interaction_ratio - 0.2*inactive_ratio - 0.2*sad_ratio
- 0.1*bored_ratio`. Our current reward is a flat `+1.0`/`-0.2`. Enriching it with this weighted
formula (we already track most of the needed `EpochState` fields — `aps_found`,
`handshakes_this_epoch`, `blind_epochs`, `deauths_sent`, `assoc_attempts`) is a worthwhile future
improvement, not yet implemented.

**XP/leveling is a legitimate, precedented design choice, not an invention.** evilsocket's original
has no XP/leveling at all — mood only (angry/sad/bored/excited/lonely/grateful), driven by
consecutive active/inactive epoch counters, with a peer-bond override (a well-bonded unit gets
"grateful" *instead of* whatever negative mood it would otherwise compute — a real mechanic we
don't yet implement; our `compute_mood()` short-circuits to `Motivated` on non-empty peers before
the negative-mood logic even runs, rather than overriding an already-computed negative mood; low
priority since mesh peer *reception* has no real signal source yet either). oxigotchi, a sibling
Rust reimplementation, *does* have XP/leveling (quadratic curve, level cap 999, atomic JSON
persistence, +0.08 mood boost on level-up) — validating our own XP/level system as a reasonable
design, even though it diverges from evilsocket's mood-only original. jayofelony's recovery file
(`/root/.pwnagotchi-recovery`) is session-resume state (deleted after one load), not lifetime-
accumulating XP like ours — a deliberate difference, justified by wanting a lifetime "progress"
narrative rather than a per-session one.

**Display investment was the right call.** Cross-hardware ports that skip a real display driver
(the Banana/Orange Pi port disables the display by default rather than adapting drivers) are
considered incomplete/"token" ports by comparison. Our real e-ink SPI/GPIO Rust driver
(`ui/display`) is more complete than most of the ports surveyed, even before the current
"does it actually render on real silicon" bug is resolved.

**Bottom line**: nothing in this audit surfaced missing functionality relative to what real users
actually run (jayofelony's fork). The open item is enriching the RL reward function per the
weighted formula above — tracked, not urgent, not a fidelity gap serious enough to block release.

### oxigotchi Deep-Dive: Build Steps, Content, and Logic (2026-07-18)

A closer look at CoderFX/oxigotchi specifically (our nearest sibling — also a from-scratch Rust
reimplementation), covering how it's built, what it contains, and whether its logic reveals gaps.

**Build process is fundamentally different from ours, and less rigorous.** oxigotchi has no
pi-gen-style declarative build: `tools/build_image.py` SSHs into an already hand-configured, live
Pi over the USB gadget link, strips personal data (bash history, SSH keys, logs), `dd`s the SD
card block device back over SSH, then shrinks with PiShrink and gzips. `tools/bake_release.sh`
does a second pass (loop-mount, strip the ext4 journal, `zerofree`, re-create the journal, shrink,
zip). `tools/bake_v2.sh` is a more declarative loopback-mount rebuild ("20 build steps"), but still
starts from a pre-existing base image, not a from-scratch OS bootstrap. Our pi-gen stage-based,
git-versioned, CI-reproducible build (anyone can rebuild the exact same image from a clean
checkout) is meaningfully more rigorous than any of oxigotchi's three build paths — this is a
validated architectural choice, not something to change.

**USB-gadget networking: they let NetworkManager own it; we deliberately don't — and that's the
likely reason RNDIS isn't enumerating for us on Windows right now.** Both projects use the
identical kernel-level setup (`dtoverlay=dwc2` + `cmdline.txt modules-load=dwc2,g_ether`). But
oxigotchi assigns `usb0` a real NetworkManager connection profile (dual static IPs `10.0.0.2/24` +
`192.168.137.2/24` for Windows ICS fallback) with a `usb0-fallback.service` systemd oneshot as a
last resort only if NM fails; they explicitly disable Raspberry Pi OS's own `rpi-usb-gadget-ics`
helper service due to log spam, but still let NM itself manage the interface. We do the opposite:
`pi-gen/stage5/.../99-unmanage-usb0.conf` tells NetworkManager to leave `usb0` alone entirely, and
a bespoke systemd service (`usb-net.service` + `usb-net-setup.sh`) does `ip addr add` + `dnsmasq`
by hand. Since `rpi-usb-gadget-ics` is Raspberry Pi OS's first-party, vendor-supported mechanism
for exactly this scenario, bypassing NetworkManager entirely (as we do) is the more likely
explanation for why no RNDIS/network function is enumerating on Windows in current hardware
testing (see below) — tracked as the leading hypothesis, not yet fixed. **Even when their gadget
does enumerate correctly**, oxigotchi still ships `tools/setup_rndis_ip.ps1`, a companion
PowerShell script users run on the Windows side to force a static IP onto the RNDIS adapter —
confirming DHCP-from-the-Pi alone isn't reliably sufficient on Windows in practice. We have no
Windows-side companion script at all; worth adding regardless of the NM fix.

**Other real, concrete gaps worth closing** (found via a full source read of a fresh oxigotchi
clone, cross-referenced against our own crates):

- **BT/WiFi coexistence**: oxigotchi's `bluetooth/coex.rs` "contention score" is actually dead code
  (instantiated, `apply()` never called) — pure dashboard cosmetics, not a gap for us. The *real*
  coexistence primitive is `wifi::pause_for_bt`/`resume_from_pause` (`wifi/mod.rs`): a lightweight
  ~10s pause (stop AO, bring `wlan0mon` down without a full radio-mode teardown) used specifically
  during BT device-discovery/pairing scans, layered on top of their full mutual-exclusion
  `radio.rs` lock file. Our `crates/radio::RadioManager` only supports full atomic
  teardown/bringup between top-level modes (RAGE/BT/SAFE) and has no lightweight "pause WiFi
  briefly for a BT scan" primitive, and no ad-hoc BT-discovery path (BT mode currently requires a
  pre-known `bt_address`). **Worth porting**: the pause/resume-for-BT-scan pattern, to support
  interactive phone pairing without a full mode switch.
- **QPU/GPU RF subsystem**: real, wired-in code (`qpu/` + `gpu/`, ~2.9K lines, gated behind
  `config.qpu.enabled`, default off) that maps VideoCore V3D registers directly via `/dev/mem` at
  a `BCM2837`-specific base address to GPU-accelerate WiFi frame classification. Explicitly
  BCM2837-only (Pi Zero 2W's SoC; wrong base address for Pi Zero W's BCM2835), fragile (raw
  `/dev/mem` pokes), and solves a workload (classifying a few hundred frames/sec) that doesn't
  need GPU offload on either of our target SoCs. **Not worth porting.**
- **Personality face variety**: oxigotchi's `personality/variety.rs` adds a priority-ordered face
  engine on top of the base mood system — capture milestones (1/10/25/50/100 get unique faces),
  idle rotation, time-of-day faces, and a 12% rare-face roll — layered on the same mood/XP core we
  already have. Hardware-independent, cheap, genuine personality-richness win. **Worth adopting.**
- **Config migration**: comparable to our `config::migrate`; oxigotchi additionally does
  capture-directory dedup-import and generates systemd units at runtime (we do unit provisioning
  at image-build time via pi-gen instead — a valid, not-a-gap architectural difference). Minor,
  low-priority gap: capture-directory dedup-import on migration.

None of this changes release readiness; the two "worth adopting/porting" items (BT-scan pause
primitive, personality face variety) are tracked as future enhancements, not blockers.

---

## Tech Stack

| Layer | Choice | Rationale |
|-------|--------|-----------|
| OS | Raspberry Pi OS **bookworm** (32-bit armhf) | bullseye EOL; bookworm supports Pi Zero W/2W |
| Kernel | 6.6+ LTS with `brcmfmac` + **nexmon** monitor-mode patch | Required for monitor mode + injection |
| Language | **Rust 1.80+** (MSRV 1.75) | Memory safety, no GC, small binaries, good async |
| Async runtime | **tokio** (full) + **embassy** (for embedded display) | Best ecosystem; embassy for no_std display driver |
| WiFi engine | **AngryOxide** (spawned subprocess, stdout JSON parsing) | Purpose-built Rust 802.11 attack tool; validates every capture |
| RL/AI | **burn** (training) + **candle** (inference) | Pure Rust DL; candle runs LSTM fast on ARMv6 |
| Config | **figment** + **serde** (TOML) | Compatible with pwnagotchi TOML structure |
| Logging | **tracing** + **tracing-subscriber** (json + journald) | Structured logs, integrates with systemd |
| Display | **embedded-graphics** + **ssd1306** driver (SPI/I2C) | Supports all Waveshare hats |
| Web UI | **axum** + **tera** templates + **websocket** (tokio-tungstenite) | Lightweight, async, familiar patterns |
| Lua plugins | **mlua** | Sandboxed, fast, ports Python plugin logic |
| Bluetooth | **bluez** + **dbus** (via `zbus`) | PAN tethering, auto-pairing, HCI scanning |
| Image build | **pi-gen** (forked) + custom stage for Rust artifacts | Same as jayofelony; produces `.img.xz` |
| Firmware patch | Port CoderFX `install.sh` logic to Rust (`fw-patcher` crate) | Apply at first boot via systemd oneshot |

---

## Three Modes (Radio Switching)

| Mode | WiFi | Bluetooth | Purpose |
|------|------|-----------|---------|
| **RAGE** | Monitor + attack | Off | Wardriving, handshake capture |
| **BT** | Off | PAN tether + scanning | Internet via phone, BT recon (attacks v2) |
| **SAFE** | Managed (wlan0) | PAN tether | Normal internet, SSH, uploads, updates |

**Radio manager**: BCM43436B0 shares UART between WiFi/BT — mutually exclusive. `RadioManager` handles atomic teardown/bringup via `patchram` + `brcmfmac` reload.

---

## Commands

```bash
# Build the SD image (run on x86_64 build host with docker)
make image                    # Produces build/pwnghost-rs-<version>-armhf.img.xz

# Cross-compile Rust workspace for armv6 (Pi Zero W) and armv7 (Pi Zero 2W)
cargo build --release --target arm-unknown-linux-gnueabihf   # Pi Zero W
cargo build --release --target armv7-unknown-linux-gnueabihf # Pi Zero 2W

# Run unit tests
cargo test --workspace

# Run integration tests (requires hardware)
cargo test --test integration -- --ignored

# Lint / format
cargo fmt --all --check
cargo clippy --workspace -- -D warnings

# Dev: run locally (non-root, mock WiFi)
cargo run --bin pwnghost-rs -- --config test-config.toml --mock-wifi

# Flash to SD (on Linux host)
xzcat build/pwnghost-rs-*.img.xz | sudo dd of=/dev/sdX bs=4M status=progress
```

---

## Project Structure

```
pwnghost-rs/
├── Cargo.toml                    # Workspace root
├── crates/
│   ├── pwncore/                  # Core domain types: AP, Station, Handshake, Channel, Epoch, Config
│   ├── fw-patcher/               # Firmware patcher (CoderFX 8-layer logic in Rust)
│   │   ├── src/
│   │   │   ├── manifest.rs       # manifest.json parsing + trusted hash table
│   │   │   ├── patch.rs          # inplace-v7.txt parser + atomic apply
│   │   │   ├── detect.rs         # BCM chip detection (dmesg / dtb)
│   │   │   ├── gpio.rs           # WL_REG_ON power cycle via rppal
│   │   │   ├── keepalive.rs      # wlan_keepalive daemon (embedded binary)
│   │   │   └── monitor.rs        # SDIO RAMRW crash counter monitoring
│   │   └── data/                 # Embedded: manifest.json, inplace-v7.txt, wlan_keepalive bins
│   ├── angryoxide/               # AngryOxide subprocess manager + stdout parser
│   │   ├── src/
│   │   │   ├── spawn.rs          # Process spawn with crash detection
│   │   │   ├── parser.rs         # JSON line parser (AP, handshake, stats events)
│   │   │   ├── args.rs           # CLI arg builder from config
│   │   │   └── recovery.rs       # Exponential backoff restart (5s, 10s, 20s... 5min)
│   ├── rl-agent/                 # A2C agent ported to Rust (burn/candle)
│   │   ├── src/
│   │   │   ├── model.rs          # LSTM + MLP policy/value networks
│   │   │   ├── agent.rs          # ActorCritic: select_action, update
│   │   │   ├── buffer.rs         # Rollout buffer (GAE, advantage)
│   │   │   ├── features.rs       # State → tensor feature extraction
│   │   │   └── checkpoint.rs     # Load/save .pt/.safetensors
│   │   └── models/               # Pre-trained weights (git-lfs)
│   ├── agent/                    # Main agent loop (replaces Python agent.py)
│   │   ├── src/
│   │   │   ├── agent.rs          # Core FSM: Recon → Assoc/Deauth → Hop
│   │   │   ├── epoch.rs          # Epoch tracking, mood, personality (pwnagotchi compat)
│   │   │   ├── personality.rs    # Config-driven behavior params
│   │   │   ├── faces.rs          # Classic kaomoji faces (21 moods, triggers)
│   │   │   ├── mesh.rs           # Peer advertising (custom 802.11 IE, pwnagotchi compat)
│   │   │   ├── recovery.rs       # Persist/load epoch state across reboots
│   │   │   ├── plugins.rs        # Lua plugin trait + mlua sandbox
│   │   │   ├── healing.rs        # 6-layer self-healing state machine
│   │   │   └── capture.rs        # tmpfs staging → validated .22000 + .pcapng
│   ├── radio/                    # Radio mode manager (RAGE/BT/SAFE)
│   │   ├── src/
│   │   │   ├── manager.rs        # Atomic mode switching
│   │   │   ├── wifi.rs           # Monitor mode bringup/teardown (nexmon)
│   │   │   ├── bluetooth.rs      # BT PAN tether, patchram, BlueZ DBus
│   │   │   ├── patchram.rs       # BCM43436B0 patchram loader
│   │   │   └── safe.rs           # Managed WiFi (wpa_supplicant)
│   ├── ui/
│   │   ├── display/              # E-ink driver (embedded-graphics + ssd1306)
│   │   │   ├── src/
│   │   │   │   ├── driver.rs     # SSD1306 async driver (embassy/async-spi)
│   │   │   │   ├── layout.rs     # Face rendering, status bars, gauges
│   │   │   │   └── fonts.rs      # Embedded DejaVuSansMono subset + kaomoji
│   │   └── web/                  # Axum web server
│   │       ├── src/
│   │       │   ├── server.rs     # HTTP + WebSocket endpoints
│   │       │   ├── api.rs        # REST API (/api/session, /api/peers, /api/config)
│   │       │   ├── ws.rs         # Live updates (handshakes, channel, mood)
│   │       │   └── templates/    # Tera HTML templates
│   └── config/                   # Config loading (figment + serde)
│       ├── src/
│       │   ├── defaults.toml     # Embedded defaults (from pwnagotchi + oxigotchi useful bits)
│       │   ├── schema.rs         # Config struct with validation
│       │   └── migrate.rs        # Migrate legacy config.toml → new schema
├── pi-gen/                       # Forked pi-gen with custom stages
│   ├── stage2/                   # Base OS (bookworm armhf)
│   ├── stage3/                   # Kernel + nexmon + firmware
│   ├── stage4/                   # Rust toolchain + cross-compiled artifacts
│   ├── stage5/                   # pwnghost-rs install + systemd units
│   └── config                    # pi-gen config (IMG_NAME, TARGET_HOSTNAME, etc.)
├── lua/                          # Built-in Lua plugins (ported from pwnagotchi + oxigotchi)
│   ├── auto_tune.lua
│   ├── auto_backup.lua
│   ├── auto_update.lua
│   ├── bt_tether.lua
│   ├── cache.lua
│   ├── fix_services.lua
│   ├── gpio_buttons.lua
│   ├── gps.lua
│   ├── grid.lua
│   ├── logtail.lua
│   ├── memtemp.lua
│   ├── ohcapi.lua
│   ├── pisugarx.lua
│   ├── pwncrack.lua
│   ├── session_stats.lua
│   ├── ups_lite.lua
│   ├── webcfg.lua
│   ├── wigle.lua
│   └── wpa_sec.lua
├── .github/workflows/
│   ├── build.yml                 # Cross-compile + pi-gen in Docker
│   ├── test.yml                  # Unit tests on every PR
│   └── release.yml               # Tag → build image → attach artifact
├── Makefile
├── SPEC.md
├── LICENSE (GPL-3.0)
└── README.md
```

---

## Code Style

```rust
// crates/pwncore/src/ap.rs
use serde::{Deserialize, Serialize};
use std::net::MacAddr;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AccessPoint {
    pub bssid: MacAddr,
    pub ssid: Option<String>,
    pub channel: Channel,
    pub rssi: i16,
    pub encryption: EncryptionType,
    pub vendor: String,
    pub clients: Vec<Station>,
    pub last_seen: DateTime<Utc>,
    pub handshake_captured: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum EncryptionType {
    WPA,
    WPA2,
    WPA3,
    WEP,
    OPEN,
    Unknown,
}

// Naming: snake_case fn/var, PascalCase types, UPPER_SNAKE constants
// Imports: std, then external, then workspace crates
// One blank line between sections
// max line 100 cols, rustfmt default
```

---

## Classic Pwnagotchi Faces (Enhanced Rendering)

| Mood | Kaomoji | Trigger |
|------|---------|---------|
| `look_r` | `( ⚆_⚆)` | Default right |
| `look_l` | `(☉_☉ )` | Default left |
| `look_r_happy` | `( ◕‿◕)`, `( ≧◡≦)` | Activity + good mood |
| `look_l_happy` | `(◕‿◕ )`, `(≧◡≦ )` | Activity + good mood |
| `sleep` | `(⇀‿‿↼)`, `(≖‿‿≖)`, `(－_－)` | Epoch sleep |
| `awake` | `(◕‿‿◕)` | Just woke |
| `bored` | `(-__-)`, `(—__—)` | Inactive epochs > bored_num_epochs |
| `intense` | `(°▃▃°)`, `(°ロ°)` | Active attack (deauth/assoc) |
| `cool` | `(⌐■_■)`, `(单__单)` | Deauthing |
| `happy` | `(•‿‿•)`, `(^‿‿^)`, `(^◡◡^)` | Handshake captured |
| `excited` | `(ᵔ◡◡ᵔ)`, `(✜‿‿✜)` | Multiple handshakes / streak |
| `grateful` | `(^‿‿^)` | First-try capture + peers |
| `motivated` | `(☼‿‿☼)`, `(★‿★)`, `(•̀ᴗ•́)` | Active + peers |
| `demotivated` | `(≖__≖)`, `(￣ヘ￣)`, `(¬､¬)` | Long dry spell |
| `smart` | `(✜‿‿✜)` | A2C confident action |
| `lonely` | `(ب__ب)`, `(｡•́︿•̀｡)`, `(︶︹︺)` | No peers, inactive |
| `sad` | `(╥☁╥ )`, `(╥﹏╥)`, `(ಥ﹏ಥ)` | Extended inactivity |
| `angry` | `(-_-')`, `(⇀__⇀)`, `(`___`)` | Max inactivity |
| `friend` | `(♥‿‿♥)`, `(♡‿‿♡)`, `(♥‿♥ )`, `(♥ω♥ )` | Peer detected |
| `broken` | `(☓‿‿☓)` | Error/recovery |
| `upload` | `(1__0)`, `(1__1)`, `(0__1)` | WPA-SEC/WiGLE upload |

**Rendering improvements over Python**: Pre-rasterized face bitmaps, smooth transitions, per-pixel dithering for e-ink, face position configurable.

---

## A2C Agent Strategy

| Aspect | Decision |
|--------|----------|
| **Training** | `burn` (pure Rust) — train on x86_64, export safetensors |
| **Inference** | `candle` (optimized for ARM, supports quantized INT8) |
| **Model** | LSTM (64 hidden) + MLP (2 layers, 128 units) — matches stable-baselines3 MlpLstmPolicy |
| **Observation** | AP histogram (13 ch), STA histogram (13 ch), peer histogram (13 ch), epoch stats (10 dims) = 49-dim |
| **Actions** | Channel (13), deauth (bool), assoc (bool), hop_recon_time (3 levels) = discrete + continuous |
| **Weights** | Train fresh in Rust (no license issues), ship quantized INT8 (~500KB) |
| **Fallback** | Heuristic channel hopping (non-overlapping 1/6/11) if model load fails |

---

## Testing Strategy

| Level | Framework | Scope | Location |
|-------|-----------|-------|----------|
| Unit | `cargo test` | Pure logic: config parsing, feature extraction, epoch math, patch application, face triggers, healing transitions, radio switching | `crates/*/src/**/*_test.rs` |
| Property | `proptest` | Config round-trip, patch idempotency, state machine invariants, healing transitions | `tests/proptest/` |
| Integration | `cargo test --test integration` | Full agent loop against mock AngryOxide (subprocess with canned stdout), radio mode switching | `tests/integration/` |
| Hardware | Custom runner | Real Pi Zero W/2W: monitor mode up, channel hop, handshake capture, healing trigger, BT tether | `tests/hardware/` (manual) |
| Image | `pi-gen` smoke test | Boot → service up → web UI reachable → handshake saved → healing works → BT tether works | CI: QEMU armhf + expect script |

**Coverage target**: ≥ 80% line coverage on `pwncore`, `agent`, `rl-agent`, `fw-patcher`, `angryoxide`, `healing`, `radio`.

---

## Boundaries

| Tier | Rule |
|------|------|
| **Always** | Run `cargo test --workspace` before commit; `cargo fmt --check`; `cargo clippy -D warnings`; all `unsafe` in `fw-patcher::gpio` + `radio::patchram` only, documented with `// SAFETY:` |
| **Ask first** | Adding new crate dependencies (audit supply chain); changing `config.toml` schema (migration needed); modifying pi-gen stages (affects image size/boot); changing face kaomoji set; changing radio mode sequence |
| **Never** | Commit secrets/keys; use `unwrap()`/`expect()` in production paths (use `anyhow::Context`); edit vendor firmware blobs directly (always patch via `fw-patcher`); break 32-bit armhf compatibility; require 64-bit userland |

---

## Success Criteria

1. **Image boots** on Pi Zero W (BCM43430/1) and Pi Zero 2W (BCM43436B0) with 32-bit bookworm userland
2. **Monitor mode** comes up on `wlan0mon` within 10s of boot (nexmon kernel module loaded)
3. **Firmware patch** applies automatically on first boot for BCM43436B0; no-op on BCM43430/1
4. **AngryOxide** spawns, emits JSON events on stdout (APs, handshakes, stats), REST API on `127.0.0.1:8081`
5. **Agent** connects, begins recon epoch, hops channels per personality config
6. **Handshakes** captured to `/etc/pwnghost/handshakes/*.pcapng` + `.22000` (hashcat-compatible, validated)
7. **E-ink display** shows classic kaomoji face, channel, uptime, handshake count, peers
8. **Web UI** at `http://pwnghost.local:8080` shows live map, stats, config editor
9. **A2C agent** loads quantized INT8 weights, selects actions (channel, assoc, deauth) each epoch
10. **Recovery** persists epoch state across reboot (`/root/.pwnghost-recovery.json`)
11. **Mesh** advertises presence via custom 802.11 IE; peers visible in web UI (pwnagotchi compatible)
12. **Self-healing** 6 layers functional: firmware watchdog, crash loop detection, AO watchdog (exp backoff), GPIO power cycle, graceful give-up, USB lifeline (SSH at `10.0.0.2`)
13. **Lua plugins** load and execute in sandbox (20 built-in plugins)
14. **Radio modes** switch cleanly: RAGE ↔ BT ↔ SAFE (atomic, no stuck radio)
15. **BT tether** auto-pairs with known phone, provides internet for WPA-SEC upload, updates, SSH
16. **Uptime** ≥ 24h without WiFi firmware crash on Pi Zero 2W (CoderFX fix validated)
17. **Image size** ≤ 2.5 GB compressed (`.img.xz`)
18. **Boot time** ≤ 30s (first boot ~2 min for fw patch), memory ≤ 15 MB

---

## Open Questions

1. **AngryOxide CLI stability**: JSON output format may change. Need version pinning + parser resilience.
2. **Pi Zero W (BCM43430) monitor mode**: nexmon supports it but firmware differs. Confirm CoderFX patch is no-op on this chip.
3. **Patchram binary**: Need prebuilt `brcm_patchram_plus` for armhf + BCM43436B0 firmware blob.
4. **BlueZ version**: Bookworm ships BlueZ 5.66 — confirm PAN/DUN support works.
5. **Face rendering**: Pre-rasterize all kaomoji at build time for fast e-ink updates?
6. **A2C training data**: Collect synthetic epochs from mock AngryOxide for initial training?

---

## Assumptions I'm Making

1. **AngryOxide** is used as a spawned subprocess (not a library) — stdout emits JSON lines for events. We parse this.
2. **nexmon kernel module** builds cleanly on bookworm 6.6 kernel for both armv6 and armv7.
3. **Candle** can run quantized LSTM inference at ~10-20 fps on Pi Zero W (ARMv6, no NEON).
4. **Pi Zero W (original) is ARMv6** (arm-unknown-linux-gnueabihf); Pi Zero 2W is ARMv7 (armv7-unknown-linux-gnueabihf). Both 32-bit userland.
5. **Bookworm 32-bit** images exist and boot on both boards.
6. **WL_REG_ON GPIO** is GPIO 22 on Pi Zero 2W (same as Pi 3B+); power-cycle logic from CoderFX ports directly.
7. **Config migration** is one-way (jayofelony pwnagotchi TOML to our schema) — lossless for all common keys.
8. **No Python runtime** on target image — pure Rust + Lua userspace.
9. **Classic pwnagotchi faces** — 21 moods with kaomoji, triggers match personality.toml params.
10. **Three radio modes** — RAGE/BT/SAFE with atomic switching via `RadioManager`.
11. **BT tether** uses BlueZ PAN (not DUN) — phone shares internet via Bluetooth PAN.
12. **A2C weights** trained fresh in Rust with `burn`, exported as quantized INT8 safetensors.

— **Correct me now or I'll proceed with these assumptions.**