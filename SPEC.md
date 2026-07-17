# Spec: Pwnagotchi-RS (Rust Rewrite)

## Objective

Build a **complete, flashable SD card image** for:
- **Raspberry Pi Zero W** (ARMv6, BCM43430/1, 32-bit)
- **Raspberry Pi Zero 2W** (ARMv7/ARMv8, BCM43436B0, 32-bit userland)

That replaces the Python pwnagotchi with a **pure Rust implementation** using **AngryOxide** as the WiFi attack engine. The image must:

- Be a drop-in replacement for jayofelony's pwnagotchi (bookworm, 32-bit userland)
- Use **AngryOxide** (Ragnt/AngryOxide) for WiFi recon, handshake capture, deauth/assoc/PMKID/CSA/anon-reassoc/rogue-M2 attacks
- Include **CoderFX's BCM43436B0 firmware stability patches** (8 layers) + userspace keepalive daemon
- **Port the A2C RL agent (LSTM + MLP policy)** to Rust for intelligent channel selection and attack decisions
- Support **Waveshare e-ink displays** (SSD1306, 2.13"/2.7"/2.9" V4) + **web UI** (port 8080)
- Use **TOML config** compatible with existing `config.toml` / `defaults.toml`
- Run as a single `pwnagotchi-rs` systemd service (no Python, no bettercap binary)
- Include **Lua plugin system** (via `mlua`) for extensibility
- Feature **classic pwnagotchi faces** (kaomoji mood system) enhanced with better rendering
- Implement **6-layer self-healing** (firmware watchdog, crash detection, AO watchdog, GPIO power cycle, graceful give-up, USB lifeline)
- Use **tmpfs capture pipeline** (staging → validated `.22000` + `.pcapng`) for SD card longevity
- **Bluetooth mode** with auto tethering/pairing for internet access (WPA-SEC upload, updates, SSH)

---

## What We Take from Oxigotchi (Technical Substance)

| Area | Oxigotchi Approach | We Adopt |
|------|-------------------|----------|
| AngryOxide integration | Spawn subprocess, parse JSON stdout | ✅ Exact same pattern |
| Firmware patching | CoderFX 8-layer BCM43436B0 patch | ✅ Full port to Rust |
| Self-healing | 6 layers (watchdog, crash loop, AO backoff, GPIO, give-up, USB) | ✅ Full adoption |
| Capture pipeline | tmpfs → validated .22000 + .pcapng | ✅ SD card protection |
| Lua plugins | `mlua` sandbox, 14 plugins | ✅ `mlua` + ported plugins |
| WiFi keepalive | `wlan_keepalive` daemon | ✅ Embedded in fw-patcher |
| Boot architecture | ~30s boot, single Rust binary | ✅ Same goal |
| Bluetooth | BT mode with PAN tethering, 6 attack types | ✅ BT mode for tethering (attacks deferred) |

## What We Do Differently (Our Own Thing)

| Area | Oxigotchi | Pwnagotchi-RS |
|------|-----------|---------------|
| **Faces** | 28 bull faces, XP/leveling, "Mooooood" | **Classic pwnagotchi kaomoji** (16 moods) — enhanced rendering |
| **Mood system** | Bull mood (RF-driven) | **Classic pwnagotchi automata** (epoch-based: lonely→bored→sad→angry, excited/grateful on activity) |
| **Modes** | RAGE / BT / SAFE (3-mode radio) | **RAGE (WiFi) + BT (tether) + SAFE (managed)** — 3 modes, radio switching |
| **Personality** | Aggression levels | **pwnagotchi personality.toml** params (bond_encounters_factor, max_interactions, throttle, etc.) |
| **Mesh/peers** | Custom 802.11 IE | **pwnagotchi mesh protocol** (compatible with existing units) |
| **Config** | Oxigotchi TOML | **pwnagotchi-compatible TOML** + migrations |
| **Architecture** | aarch64 only | **Dual: armv6 (Pi Zero W) + armv7 (Pi Zero 2W)** both 32-bit userland |

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
make image                    # Produces build/pwnagotchi-rs-<version>-armhf.img.xz

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
cargo run --bin pwnagotchi-rs -- --config test-config.toml --mock-wifi

# Flash to SD (on Linux host)
xzcat build/pwnagotchi-rs-*.img.xz | sudo dd of=/dev/sdX bs=4M status=progress
```

---

## Project Structure

```
pwnagotchi-rs/
├── Cargo.toml                    # Workspace root
├── crates/
│   ├── pwncore/                  # Core domain types: AP, Station, Handshake, Channel, Epoch, Config
│   ├── fw-patcher/               # Firmware patcher (CoderFX 8-layer logic in Rust)
│   │   ├── src/
│   │   │   ├── manifest.rs       # manifest.json parsing + trusted hash table
│   │   │   ├── patch.rs          # inplace-v7.txt parser + atomic apply
│   │   │   ├── detect.rs         # BCM chip detection (dmesg / dtb)
│   │   │   ├── gpio.rs           # WL_REG_ON power cycle via rppal
│   │   │   └── keepalive.rs      # wlan_keepalive daemon (embedded binary)
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
│   │   │   ├── features.rs       # State -> tensor feature extraction
│   │   │   └── checkpoint.rs     # Load/save .pt/.safetensors
│   │   └── models/               # Pre-trained weights (git-lfs)
│   ├── agent/                    # Main agent loop (replaces Python agent.py)
│   │   ├── src/
│   │   │   ├── agent.rs          # Core FSM: Recon -> Assoc/Deauth -> Hop
│   │   │   ├── epoch.rs          # Epoch tracking, mood, personality (pwnagotchi compat)
│   │   │   ├── personality.rs    # Config-driven behavior params
│   │   │   ├── faces.rs          # Classic kaomoji faces (16 moods, triggers)
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
│   │   ├── web/                  # Axum web server
│   │   │   ├── src/
│   │   │   │   ├── server.rs     # HTTP + WebSocket endpoints
│   │   │   │   ├── api.rs        # REST API (/api/session, /api/peers, /api/config)
│   │   │   │   ├── ws.rs         # Live updates (handshakes, channel, mood)
│   │   │   │   └── templates/    # Tera HTML templates
│   ├── config/                   # Config loading (figment + serde)
│   │   └── src/
│   │       ├── defaults.toml     # Embedded defaults (from pwnagotchi + oxigotchi useful bits)
│   │       ├── schema.rs         # Config struct with validation
│   │       └── migrate.rs        # Migrate legacy config.toml -> new schema
│   ├── pwnagotchi-rs/            # Main binary
│   │   └── src/
│   │       ├── main.rs           # Entry: init logging, config, spawn tasks
│   │       ├── boot.rs           # First-boot: fw-patch, generate keys, calibrate
│   │       └── service.rs        # Systemd service installer helper
├── pi-gen/                       # Forked pi-gen with custom stages
│   ├── stage2/                   # Base OS (bookworm armhf)
│   ├── stage3/                   # Kernel + nexmon + firmware
│   ├── stage4/                   # Rust toolchain + cross-compiled artifacts
│   ├── stage5/                   # pwnagotchi-rs install + systemd units
│   └── config                    # pi-gen config (IMG_NAME, TARGET_HOSTNAME, etc.)
├── lua/                          # Built-in Lua plugins (ported from pwnagotchi + oxigotchi)
│   ├── auto_tune.lua             # Channel/attack auto-tuning
│   ├── auto_backup.lua           # Config backup
│   ├── auto_update.lua           # Self-update from GitHub
│   ├── bt_tether.lua             # Bluetooth tethering (mode switching)
│   ├── cache.lua                 # AP/handshake caching
│   ├── fix_services.lua          # Systemd service recovery
│   ├── gpio_buttons.lua          # Hardware button handling (mode cycle)
│   ├── gps.lua                   # GPS support
│   ├── grid.lua                  # Mesh/peer communication
│   ├── logtail.lua               # Log viewing
│   ├── memtemp.lua               # Memory/temp display
│   ├── ohcapi.lua                # OpenHandshakes API
│   ├── pisugarx.lua              # PiSugar battery
│   ├── pwncrack.lua              # WPA crack integration
│   ├── session_stats.lua         # Session statistics
│   ├── ups_lite.lua              # UPS HAT support
│   ├── webcfg.lua                # Web config UI
│   ├── wigle.lua                 # WiGLE upload
│   └── wpa_sec.lua               # WPA-SEC upload
├── .github/workflows/
│   ├── build.yml                 # Cross-compile + pi-gen in Docker
│   ├── test.yml                  # Unit tests on every PR
│   └── release.yml               # Tag -> build image -> attach artifact
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
    pub channel: u8,
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
| `bored` | `(-__-)`, `(—__—)` | Inactive epochs ≥ bored_num_epochs |
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
| `angry` | `(-_-')`, `(⇀__⇀)`, `(\`___\`)` | Max inactivity |
| `friend` | `(♥‿‿♥)`, `(♡‿‿♡)`, `(♥‿♥ )`, `(♥ω♥ )` | Peer detected |
| `broken` | `(☓‿‿☓)` | Error/recovery |
| `upload` | `(1__0)`, `(1__1)`, `(0__1)` | WPA-SEC/WiGLE upload |
| `png` | `false` | Disable for e-ink |

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
| Image | `pi-gen` smoke test | Boot -> service up -> web UI reachable -> handshake saved -> healing works -> BT tether works | CI: QEMU armhf + expect script |

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
6. **Handshakes** captured to `/etc/pwnagotchi/handshakes/*.pcapng` + `.22000` (hashcat-compatible, validated)
7. **E-ink display** shows classic kaomoji face, channel, uptime, handshake count, peers
8. **Web UI** at `http://pwnagotchi.local:8080` shows live map, stats, config editor
9. **A2C agent** loads quantized INT8 weights, selects actions (channel, assoc, deauth) each epoch
10. **Recovery** persists epoch state across reboot (`/root/.pwnagotchi-recovery.json`)
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

→ **Correct me now or I'll proceed with these assumptions.**