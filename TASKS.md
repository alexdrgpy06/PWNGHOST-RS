# Tasks: Pwnagotchi-RS Implementation

## Phase 3: Task Breakdown

Each task is a single-session unit with acceptance criteria and verification command.

---

## Milestone 0: Foundation

### M0.1: pi-gen Stage 2 - Bookworm armhf Base
- [ ] **Task**: Configure pi-gen stage2 for Raspberry Pi OS Bookworm 32-bit (armhf)
  - Acceptance: pi-gen builds base image without errors
  - Verify: `cd pi-gen && ./build.sh 2>&1 | grep -E "(Stage 2|base|armhf)"`
  - Files: `pi-gen/stage2/`, `pi-gen/config`

### M0.2: pi-gen Stage 3 - Kernel 6.6 + Nexmon Build
- [ ] **Task**: Add kernel 6.6 LTS + nexmon monitor-mode patch to stage3
  - Acceptance: `modprobe brcmfmac` loads nexmon-patched module; `iw phy` shows monitor mode
  - Verify: `cd pi-gen && ./build.sh 2>&1 | grep -E "(Stage 3|nexmon|brcmfmac)"`
  - Files: `pi-gen/stage3/04-nexmon/`, `pi-gen/stage3/03-kernel/`

### M0.3: Rust Workspace Setup
- [ ] **Task**: Create Cargo workspace with all crate skeletons
  - Acceptance: `cargo check --workspace` passes; both armv6 and armv7 targets compile
  - Verify: `cargo check --workspace --target arm-unknown-linux-gnueabihf && cargo check --workspace --target armv7-unknown-linux-gnueabihf`
  - Files: `Cargo.toml`, `crates/*/Cargo.toml` (12 crates)

### M0.4: Cross-Compile Targets + CI
- [ ] **Task**: Add armv6/armv7 targets; configure GitHub Actions for cross-compile
  - Acceptance: CI builds all crates for both targets; artifacts uploaded
  - Verify: Check GitHub Actions "Cross-compile" workflow success
  - Files: `.github/workflows/cross-compile.yml`, `rust-toolchain.toml`, `Cross.toml`

---

## Milestone 1: Firmware Patcher & AngryOxide

### M1.1: fw-patcher - Manifest Parsing + Hash Validation
- [ ] **Task**: Implement `manifest.rs` with schema v1 parsing + trusted hash table
  - Acceptance: Parses CoderFX manifest.json; validates all SHA256 against embedded trusted table
  - Verify: `cargo test -p fw-patcher manifest -- --nocapture`
  - Files: `crates/fw-patcher/src/manifest.rs`, `crates/fw-patcher/data/manifest.json`

### M1.2: fw-patcher - Inplace Patch Parser + Atomic Apply
- [ ] **Task**: Implement `patch.rs` parsing inplace-v7.txt + atomic write with verification
  - Acceptance: Applies patch to firmware copy; verifies output SHA256 matches manifest; atomic rename
  - Verify: `cargo test -p fw-patcher patch -- --nocapture`
  - Files: `crates/fw-patcher/src/patch.rs`, `crates/fw-patcher/data/inplace-v7.txt`

### M1.3: fw-patcher - BCM Chip Detection
- [ ] **Task**: Implement `detect.rs` reading DTB/dmesg for BCM43436B0 vs BCM43430
  - Acceptance: Returns correct chip enum on both Pi Zero W and Pi Zero 2W
  - Verify: `cargo test -p fw-patcher detect -- --nocapture` (mock DTB fixtures)
  - Files: `crates/fw-patcher/src/detect.rs`

### M1.4: fw-patcher - GPIO Power Cycle (WL_REG_ON)
- [ ] **Task**: Implement `gpio.rs` using `rppal` to toggle GPIO 22 (WL_REG_ON)
  - Acceptance: Power cycles WiFi chip; `dmesg` shows brcmfmac re-enumeration
  - Verify: `cargo test -p fw-patcher gpio -- --ignored --nocapture` (requires hardware)
  - Files: `crates/fw-patcher/src/gpio.rs`

### M1.5: fw-patcher - Embed wlan_keepalive Binary
- [ ] **Task**: Embed ARMv6 + ARMv7 `wlan_keepalive` binaries; install as systemd service
  - Acceptance: Daemon starts on boot, binds `wlan0mon`, injects probe every 3s
  - Verify: `cargo test -p fw-patcher keepalive -- --nocapture` + hardware test
  - Files: `crates/fw-patcher/src/keepalive.rs`, `crates/fw-patcher/data/wlan_keepalive.*`

### M1.6: angryoxide - Subprocess Spawn + Crash Detection
- [ ] **Task**: Implement `spawn.rs` with tokio process, stdout/stderr capture, crash detection
  - Acceptance: Spawns AO; detects exit code ≠ 0 or stdout stall > 30s
  - Verify: `cargo test -p angryoxide spawn -- --nocapture`
  - Files: `crates/angryoxide/src/spawn.rs`

### M1.7: angryoxide - JSON Line Parser
- [ ] **Task**: Implement `parser.rs` for AO JSON events (ap, handshake, stats, client)
  - Acceptance: Parses real AO stdout (fixture file); emits typed `AoEvent` enum
  - Verify: `cargo test -p angryoxide parser -- --nocapture`
  - Files: `crates/angryoxide/src/parser.rs`, `crates/angryoxide/tests/fixtures/ao_stdout.jsonl`

### M1.8: angryoxide - CLI Arg Builder
- [ ] **Task**: Implement `args.rs` building AO command line from config
  - Acceptance: Generates valid AO args for all config permutations (targets, whitelist, channels, rate)
  - Verify: `cargo test -p angryoxide args -- --nocapture`
  - Files: `crates/angryoxide/src/args.rs`

### M1.9: angryoxide - Exponential Backoff Recovery
- [ ] **Task**: Implement `recovery.rs` with 5s/10s/20s/40s/5min backoff + max retries
  - Acceptance: On AO crash, waits backoff, restarts, resumes parsing
  - Verify: `cargo test -p angryoxide recovery -- --nocapture`
  - Files: `crates/angryoxide/src/recovery.rs`

---

## Milestone 2: Core Domain & Config

### M2.1: pwncore - Domain Types (AP, Station, Handshake, Channel)
- [ ] **Task**: Define core types with serde + validation
  - Acceptance: Round-trip serialize/deserialize; MacAddr parsing; channel 1-14 validation
  - Verify: `cargo test -p pwncore types -- --nocapture`
  - Files: `crates/pwncore/src/ap.rs`, `crates/pwncore/src/station.rs`, `crates/pwncore/src/handshake.rs`

### M2.2: pwncore - Epoch, Mood, Personality Types
- [ ] **Task**: Define epoch tracking, mood state machine, personality params
  - Acceptance: Mood transitions match personality.toml thresholds exactly
  - Verify: `cargo test -p pwncore mood -- --nocapture`
  - Files: `crates/pwncore/src/epoch.rs`, `crates/pwncore/src/mood.rs`, `crates/pwncore/src/personality.rs`

### M2.3: config - Schema (Full pwnagotchi TOML)
- [ ] **Task**: Define `Config` struct covering all sections (main, personality, ui, bettercap, fs, plugins)
  - Acceptance: Loads jayofelony defaults.toml without error
  - Verify: `cargo test -p config schema -- --nocapture`
  - Files: `crates/config/src/schema.rs`

### M2.4: config - Migration (Legacy → New)
- [ ] **Task**: Implement `migrate.rs` converting old config.toml to new schema
  - Acceptance: Migrates jayofelony config.toml losslessly for all common keys
  - Verify: `cargo test -p config migrate -- --nocapture`
  - Files: `crates/config/src/migrate.rs`

### M2.5: config - Validation + Defaults
- [ ] **Task**: Add validation rules + embedded defaults.toml
  - Acceptance: Rejects invalid config with actionable errors; provides all defaults
  - Verify: `cargo test -p config validate -- --nocapture`
  - Files: `crates/config/src/schema.rs`, `crates/config/src/defaults.toml`

---

## Milestone 3: Agent Core

### M3.1: agent - Epoch Tracking + Mood Transitions
- [ ] **Task**: Implement `epoch.rs` with full state machine (inactive→bored→sad→angry, active→excited)
  - Acceptance: Mood matches pwnagotchi Python automata.py exactly for given epoch sequences
  - Verify: `cargo test -p agent epoch -- --nocapture`
  - Files: `crates/agent/src/epoch.rs`

### M3.2: agent - Personality Config → Behavior
- [ ] **Task**: Implement `personality.rs` mapping config params to agent behavior (recon_time, throttle, max_interactions)
  - Acceptance: Config changes alter epoch behavior without code change
  - Verify: `cargo test -p agent personality -- --nocapture`
  - Files: `crates/agent/src/personality.rs`

### M3.3: agent - Classic Kaomoji Faces (21 moods)
- [ ] **Task**: Implement `faces.rs` with all 21 face sets + trigger conditions
  - Acceptance: Each mood returns correct kaomoji array; triggers match epoch state
  - Verify: `cargo test -p agent faces -- --nocapture`
  - Files: `crates/agent/src/faces.rs`

### M3.4: agent - Main FSM (Recon → Attack → Hop → Sleep)
- [ ] **Task**: Implement `agent.rs` core loop integrating AO parser, epoch, faces
  - Acceptance: Runs 3 epochs against mock AO; produces correct channel hops, attacks, face changes
  - Verify: `cargo test -p agent integration -- --nocapture`
  - Files: `crates/agent/src/agent.rs`

### M3.5: agent - Mesh Peer Advertising
- [ ] **Task**: Implement `mesh.rs` with custom 802.11 IE (pwnagotchi compatible)
  - Acceptance: Peers discovered via AO events advertise IE; visible in web UI
  - Verify: `cargo test -p agent mesh -- --nocapture`
  - Files: `crates/agent/src/mesh.rs`

### M3.6: agent - Recovery Persist/Load
- [ ] **Task**: Implement `recovery.rs` saving epoch state to `/root/.pwnagotchi-recovery.json`
  - Acceptance: After reboot, agent resumes epoch count, mood, handshake history
  - Verify: `cargo test -p agent recovery -- --nocapture`
  - Files: `crates/agent/src/recovery.rs`

---

## Milestone 4: Self-Healing & Capture Pipeline

### M4.1: agent - 6-Layer Healing State Machine
- [ ] **Task**: Implement `healing.rs` with layers: fw_watchdog, crash_loop, ao_backoff, gpio_cycle, giveup, usb_lifeline
  - Acceptance: State machine transitions correctly on injected failures; logs layer activation
  - Verify: `cargo test -p agent healing -- --nocapture`
  - Files: `crates/agent/src/healing.rs`

### M4.2: agent - Capture Pipeline (tmpfs → validated .22000 + .pcapng)
- [ ] **Task**: Implement `capture.rs` staging in tmpfs, hcxtools validation, atomic move to handshakes dir
  - Acceptance: Only valid handshakes saved; .22000 hashcat-ready; .pcapng has GPS/radiotap
  - Verify: `cargo test -p agent capture -- --nocapture`
  - Files: `crates/agent/src/capture.rs`

### M4.3: Integration - Full Healing Cycle
- [ ] **Task**: End-to-end test: inject AO crash → backoff → GPIO cycle → recovery
  - Acceptance: Agent continues epochs after full healing cycle; no manual intervention
  - Verify: `cargo test -p agent healing_integration -- --ignored --nocapture` (hardware)
  - Files: `crates/agent/tests/healing_integration.rs`

---

## Milestone 5: RL Agent

### M5.1: rl-agent - Model Architecture (LSTM + MLP in burn)
- [ ] **Task**: Define `model.rs` with LSTM(64) + MLP(2x128) policy/value heads
  - Acceptance: Model compiles; forward pass produces (action_logits, value) for 49-dim input
  - Verify: `cargo test -p rl-agent model -- --nocapture`
  - Files: `crates/rl-agent/src/model.rs`

### M5.2: rl-agent - Feature Extraction (State → Tensor)
- [ ] **Task**: Implement `features.rs` converting epoch state to 49-dim tensor
  - Acceptance: AP histogram (13), STA histogram (13), peer histogram (13), epoch stats (10) = 49
  - Verify: `cargo test -p rl-agent features -- --nocapture`
  - Files: `crates/rl-agent/src/features.rs`

### M5.3: rl-agent - ActorCritic (select_action + update)
- [ ] **Task**: Implement `agent.rs` with action sampling, log_prob, entropy, GAE advantage
  - Acceptance: Samples actions; computes loss; updates weights (test with dummy rollout)
  - Verify: `cargo test -p rl-agent actor_critic -- --nocapture`
  - Files: `crates/rl-agent/src/agent.rs`

### M5.4: rl-agent - Checkpoint Load (safetensors INT8)
- [ ] **Task**: Implement `checkpoint.rs` loading quantized INT8 safetensors
  - Acceptance: Loads model in <500ms; inference <10ms on Pi Zero W (ARMv6)
  - Verify: `cargo test -p rl-agent checkpoint -- --nocapture`
  - Files: `crates/rl-agent/src/checkpoint.rs`

### M5.5: Training Pipeline (burn) → Export INT8
- [ ] **Task**: Separate training binary using burn; export quantized INT8 safetensors
  - Acceptance: Produces ~500KB model file; validates on held-out synthetic data
  - Verify: `cargo run --release --bin train_rl_agent -- --epochs 100` (offline)
  - Files: `crates/rl-agent/src/train.rs`, `crates/rl-agent/models/` (git-lfs)

### M5.6: Integration - Agent Calls RL Each Epoch
- [ ] **Task**: Wire RL agent into main agent loop (fallback to heuristic on error)
  - Acceptance: Agent uses RL action for channel/attack decisions; falls back cleanly
  - Verify: `cargo test -p agent rl_integration -- --nocapture`
  - Files: `crates/agent/src/agent.rs`, `crates/rl-agent/src/agent.rs`

---

## Milestone 6: Radio Manager (3 Modes)

### M6.1: radio - WiFi Monitor Mode Control
- [ ] **Task**: Implement `wifi.rs` bringing up/down `wlan0mon` via nexmon + ip/iw
  - Acceptance: `wlan0mon` appears in `iw dev`; monitor mode confirmed
  - Verify: `cargo test -p radio wifi -- --ignored --nocapture` (hardware)
  - Files: `crates/radio/src/wifi.rs`

### M6.2: radio - Patchram Firmware Load (BCM43436B0)
- [ ] **Task**: Implement `patchram.rs` calling `brcm_patchram_plus` for BT firmware
  - Acceptance: `hci0` appears; `hciconfig hci0 up` succeeds
  - Verify: `cargo test -p radio patchram -- --ignored --nocapture` (hardware)
  - Files: `crates/radio/src/patchram.rs`, prebuilt `brcm_patchram_plus` in pi-gen

### M6.3: radio - Bluetooth PAN Tether (BlueZ DBus)
- [ ] **Task**: Implement `bluetooth.rs` using `zbus` for BlueZ PAN connect/disconnect
  - Acceptance: Pairs with phone; creates `bnep0` with internet; auto-reconnect
  - Verify: `cargo test -p radio bluetooth -- --ignored --nocapture` (hardware)
  - Files: `crates/radio/src/bluetooth.rs`

### M6.4: radio - SAFE Mode (Managed WiFi + wpa_supplicant)
- [ ] **Task**: Implement `safe.rs` managing wpa_supplicant for known networks
  - Acceptance: Connects to configured WiFi; provides internet for uploads
  - Verify: `cargo test -p radio safe -- --ignored --nocapture` (hardware)
  - Files: `crates/radio/src/safe.rs`

### M6.5: radio - Manager (Atomic RAGE↔BT↔SAFE)
- [ ] **Task**: Implement `manager.rs` orchestrating teardown/bringup sequence
  - Acceptance: Full cycle RAGE→BT→SAFE→RAGE < 5s each; no radio stuck states
  - Verify: `cargo test -p radio manager -- --ignored --nocapture` (hardware)
  - Files: `crates/radio/src/manager.rs`

---

## Milestone 7: UI

### M7.1: ui/display - SSD1306 Async SPI Driver
- [ ] **Task**: Implement `driver.rs` using `embassy` + `rppal` SPI for Waveshare 2.13" V4
  - Acceptance: Initializes display; draws test pattern; no flicker
  - Verify: `cargo test -p ui_display driver -- --ignored --nocapture` (hardware)
  - Files: `crates/ui/display/src/driver.rs`

### M7.2: ui/display - Layout (Face, Bars, Gauges)
- [ ] **Task**: Implement `layout.rs` rendering face + channel + uptime + handshakes + peers + XP
  - Acceptance: All 21 faces render correctly; status bars update per epoch
  - Verify: `cargo test -p ui_display layout -- --ignored --nocapture` (hardware)
  - Files: `crates/ui/display/src/layout.rs`

### M7.3: ui/display - Embedded Fonts (DejaVu + Kaomoji)
- [ ] **Task**: Implement `fonts.rs` with embedded DejaVuSansMono subset + pre-rasterized kaomoji
  - Acceptance: Text crisp at 12pt; kaomoji render without runtime formatting
  - Verify: `cargo test -p ui_display fonts -- --nocapture`
  - Files: `crates/ui/display/src/fonts.rs`, `crates/ui/display/data/fonts/`

### M7.4: ui/web - Axum Server + WebSocket
- [ ] **Task**: Implement `server.rs` with HTTP + WS upgrade on :8080
  - Acceptance: Serves static files; WS connects; broadcasts test message
  - Verify: `cargo test -p ui_web server -- --nocapture`
  - Files: `crates/ui/web/src/server.rs`

### M7.5: ui/web - REST API Endpoints
- [ ] **Task**: Implement `api.rs` with `/api/session`, `/api/peers`, `/api/config`, `/api/handshakes`
  - Acceptance: All endpoints return JSON matching spec; config supports PATCH
  - Verify: `cargo test -p ui_web api -- --nocapture`
  - Files: `crates/ui/web/src/api.rs`

### M7.6: ui/web - Live WebSocket Updates
- [ ] **Task**: Implement `ws.rs` pushing handshakes, channel changes, mood, peer events
  - Acceptance: Browser receives updates <100ms after agent event
  - Verify: `cargo test -p ui_web ws -- --nocapture`
  - Files: `crates/ui/web/src/ws.rs`

### M7.7: ui/web - Tera Templates (Dashboard)
- [ ] **Task**: Create Tera templates for dashboard (map, stats, config editor, handshake list)
  - Acceptance: Dashboard loads; all sections render; config editor saves via API
  - Verify: `cargo test -p ui_web templates -- --nocapture`
  - Files: `crates/ui/web/templates/*.html`, `crates/ui/web/src/templates.rs`

---

## Milestone 8: Lua Plugins

### M8.1: agent - mlua Sandbox + Plugin Trait
- [ ] **Task**: Implement `plugins.rs` with `mlua` sandbox (no os/io/debug), plugin trait, event bus
  - Acceptance: Loads hello.lua; calls on_epoch, on_handshake, on_ui_update safely
  - Verify: `cargo test -p agent plugins -- --nocapture`
  - Files: `crates/agent/src/plugins.rs`

### M8.2: Port 20 Plugins (Python/Lua → Rust mlua)
- [ ] **Task**: Port all plugins from `lua/` directory to mlua-compatible Lua
  - Acceptance: Each plugin loads; handles enabled/disabled; config via TOML
  - Verify: `cargo test -p agent plugin_* -- --nocapture` (20 tests)
  - Files: `lua/*.lua` (20 files), `crates/agent/src/plugins.rs` (registration)

### M8.3: Plugin Config via TOML
- [ ] **Task**: Wire plugin `options` from `config['main']['plugins'][name]`
  - Acceptance: Plugin reads config; config changes reload plugin
  - Verify: `cargo test -p agent plugin_config -- --nocapture`
  - Files: `crates/agent/src/plugins.rs`, `crates/config/src/schema.rs`

---

## Milestone 9: Integration & Image

### M9.1: pwnagotchi-rs - Main Binary Wiring
- [ ] **Task**: Implement `main.rs` initializing all subsystems, spawning tokio tasks
  - Acceptance: Binary runs; all crates initialized; graceful shutdown on SIGTERM
  - Verify: `cargo run --bin pwnagotchi-rs -- --config test.toml --mock` (dev)
  - Files: `crates/pwnagotchi-rs/src/main.rs`

### M9.2: pwnagotchi-rs - First Boot (fw-patch + keys)
- [ ] **Task**: Implement `boot.rs` detecting first boot, running fw-patcher, generating mesh keys
  - Acceptance: On fresh image, applies FW patch, generates ed25519 keypair, writes config
  - Verify: `cargo test -p pwnagotchi-rs boot -- --ignored --nocapture` (hardware)
  - Files: `crates/pwnagotchi-rs/src/boot.rs`

### M9.3: pi-gen Stage 4 - Cross-Compiled Artifacts
- [ ] **Task**: Stage 4 installs all release binaries + Lua plugins to sysroot
  - Acceptance: All 12 crates' release binaries in `/usr/local/bin`; plugins in `/usr/share/pwnagotchi-rs/lua`
  - Verify: `cd pi-gen && ./build.sh 2>&1 | grep -E "(Stage 4|install)"`
  - Files: `pi-gen/stage4/01-rust-artifacts/`

### M9.4: pi-gen Stage 5 - Systemd + Config
- [ ] **Task**: Stage 5 installs systemd units, default config, logrotate, tmpfiles
  - Acceptance: `systemctl enable pwnagotchi-rs`; service starts on boot; logs to journald
  - Verify: `cd pi-gen && ./build.sh 2>&1 | grep -E "(Stage 5|systemd)"`
  - Files: `pi-gen/stage5/01-service/`, `pi-gen/stage5/02-config/`

### M9.5: Full Image Build + QEMU Smoke Test
- [ ] **Task**: Build complete .img.xz; boot in QEMU armhf; verify service + web UI
  - Acceptance: QEMU boots to login; `systemctl status pwnagotchi-rs` = active; curl :8080 works
  - Verify: `./scripts/qemu-test.sh` (custom script)
  - Files: `pi-gen/`, `scripts/qemu-test.sh`

### M9.6: Hardware Validation (Pi Zero W + Pi Zero 2W)
- [ ] **Task**: Flash image to SD; test all 18 success criteria on both boards
  - Acceptance: All criteria met (see SPEC.md Success Criteria)
  - Verify: Physical test checklist (separate document)
  - Files: N/A (hardware test)

---

## Task Execution Rules

1. **One task at a time** - Complete verification before moving on
2. **TDD** - Write test first, then implementation
3. **Verify command must pass** - No "it should work" without evidence
4. **Max 5 files per task** - If more, split the task
5. **Commit after each task** - `git commit -m "task: M3.1 epoch tracking"`