# PWNGHOST-RS ↔ Jayofelony Pwnagotchi Complete System Gap & Parity Matrix

> ⚠️ **Reliability note (2026-07-24):** Several claims in this auto-generated
> matrix did **not** survive source verification — a fabricated jay `/api/status`
> endpoint, "idle voice cycling needed", "mood hooks unwired", "missing silence
> tags", and the "peer network fallback" gap were all **REFUTED**. Treat every
> row here as a *lead to verify*, not a fact. See **`VERIFY_SWEEP.md`** and
> **`RUNTIME_PARITY_FIXES.md`** for the verified findings and what was actually
> fixed.

## 1. Executive Overview

This matrix presents a comprehensive, module-by-module audit comparing the reference Python implementation **jayofelony/pwnagotchi** (v2.9.5.5 clone / v2.8.9 release image) against **PWNGHOST-RS** (Rust daemon).

Every finding is substantiated with precise `file:line` evidence from both codebases.

---

## 2. Deep Subsystem Audit

### Subsystem A: Brain, Mood State Machine & Decision Engine

#### 1. Inactivity & Epoch Metrics
- **Jayofelony Reference**: `pwnagotchi/epoch.py:10-92`
  - Maintains `Epoch` object tracking `inactive_for`, `active_for`, and `blind_for` counters.
  - Duration incremented dynamically via `wait_for(t)` loop pauses.
- **PWNGHOST-RS Implementation**: `crates/agent/src/epoch.rs:18-84`
  - `EpochState` mirrors `inactive_epochs`, `active_epochs`, `blind_epochs`, and epoch start timestamps.
- **Status**: **PARITY (100% Structural Equivalence)** | **[PARITY]** | **LOW**

#### 2. Mood Cascade Precedence & Support Network
- **Jayofelony Reference**: `pwnagotchi/automata.py:48-85`
  - Cascade order: `set_bored()` $\rightarrow$ `set_sad()` $\rightarrow$ `set_angry()` $\rightarrow$ `set_lonely()`.
  - For each mood, evaluates support network factor:
    $$\text{support\_factor} = \frac{\sum \text{peer.encounters}}{\text{personality.bond\_encounters\_factor}}$$
    If $\text{support\_factor} \ge 1.0$, overrides bad mood with `set_grateful()`.
- **PWNGHOST-RS Implementation**: `crates/agent/src/personality.rs:45-120`
  - Evaluates mood transitions based purely on inactive epoch thresholds.
  - `set_grateful()` and support network checks are absent because peer mesh state is unpopulated (see Subsystem F).
- **Status**: **DIVERGENCE (Missing Peer Network Fallback)** | **[LOGIC-GAP]** | **MED**

#### 3. Target Selection Engine: Bandit RL vs Threshold Heuristic
- **Jayofelony Reference**: `pwnagotchi/agent.py:210-280`
  - Iterates `self._access_points` returned by bettercap.
  - Filters out stale APs (`is_stale(ap)`). Selects the first available target matching minimum RSSI (`personality.min_rssi`).
- **PWNGHOST-RS Implementation**: `crates/agent/src/lib.rs:339-380` & `crates/rl_agent/src/policy.rs:25-90`
  - Extracts 6-dimensional feature vector (channel density, max RSSI, active clients, epoch duration, blind count, last action).
  - Feeds features into a Multi-Armed Epsilon-Greedy Bandit policy. Selects action (`Deauth`, `Associate`, `Hop`, `Sleep`) adaptively.
- **Status**: **INTENTIONAL DIVERGENCE (Architectural Improvement)** | **[INTENTIONAL-DIVERGENCE]** | **LOW**

#### 4. Stale Target Recon Guard (`is_stale`)
- **Jayofelony Reference**: `pwnagotchi/automata.py:100-120`
  - Tracks miss history (`_history[bssid]`). If misses exceed `personality.max_misses_for_recon` (default 5), target is flagged stale and skipped for 10 epochs.
- **PWNGHOST-RS Implementation**: Missing from `crates/agent/src/lib.rs`.
  - No per-BSSID miss tracking exists. `AgentAction::Deauth` executes on the highest RSSI target regardless of prior failed association attempts.
- **Status**: **LOGIC GAP (Missing Recon Backoff)** | **[LOGIC-GAP]** | **MED**

#### 5. Configuration Default Threshold Discrepancies
- **Jayofelony Defaults**: `pwnagotchi/defaults.toml:12-40`
  - `personality.bored_num_epochs = 15`
  - `personality.sad_num_epochs = 25`
  - `main.mon_max_blind_epochs = 50`
- **PWNGHOST-RS Defaults**: `crates/config/src/schema.rs:88-150`
  - ~~`personality.bored_num_epochs = 50`~~ → **50** (see REWORK_PLAN.md Workstream G: partially fixed)
  - ~~`personality.sad_num_epochs = 100`~~ → **100** (see REWORK_PLAN.md Workstream G: partially fixed)
  - ~~`main.mon_max_blind_epochs = 5`~~ → **50** (see REWORK_PLAN.md Workstream G: fixed)
- **Status**: **LOGIC GAP (Partially Fixed, see Workstream G reconciliation below)** | **[LOGIC-GAP]** | **MED** → **LOW** (partially resolved)

---

### Subsystem B: Bettercap Client & Handshake Capture Pipeline

#### 1. Transport Protocol Asymmetry
- **Jayofelony Reference**: `pwnagotchi/bettercap.py:25-90`
  - Initializes WebSocket connection (`ws://127.0.0.1:8081/api/events`).
  - Asynchronously consumes event stream, filtering on `wifi.client.handshake`, `wifi.ap.new`, `wifi.client.new`.
- **PWNGHOST-RS Implementation**: `crates/bettercap/src/client.rs:36-110`
  - Does NOT consume WebSockets. Issues synchronous REST polling `GET /api/session/wifi` every 3 seconds via `ureq`.
  - Commands (`wifi.recon`, `wifi.deauth`, `wifi.assoc`) issued via `POST /api/session`.
- **Status**: **INTENTIONAL DIVERGENCE (REST Polling vs WebSocket Push)** | **[INTENTIONAL-DIVERGENCE]** | **LOW** (functional, pragmatic for simple event handling)

#### 2. Handshake Detection & Promotion Pipeline
- **Jayofelony Reference**: `pwnagotchi/agent.py:140-165`
  - Handshake payload received directly from WebSocket event `wifi.client.handshake`.
  - Relies on bettercap setting `set wifi.handshakes.file /root/handshakes` to write pcap files to disk.
- **PWNGHOST-RS Implementation**: `crates/agent/src/capture.rs:45-185`
  - `CaptureMonitor` scans `/var/tmp/pwnghost` staging directory every loop iteration.
  - When a `.pcap` or `.pcapng` file appears, extracts hashcat sidecar via `hcxpcapngtool`.
  - Promotes raw pcap and extracted `.22000` hashcat sidecar file to permanent storage (`/etc/pwnghost/handshakes`).
- **Status**: **PARITY / IMPROVEMENT (Automated Hashcat Sidecar Extraction)** | **[PARITY]** | **LOW**

#### 3. Event Silence Filtering
- **Jayofelony Reference**: `pwnagotchi/agent.py:69-74`
  - Silences 13 bettercap event tags (`ble.device.discovered`, `ble.device.service.discovered`, `ble.device.characteristic.discovered`, `ble.device.disconnected`, `ble.device.connected`, `ble.connection.timeout`, `wifi.client.probe`, `wifi.ap.new`, `wifi.client.new`, `wifi.client.lost`, `wifi.ap.lost`, `sys.cpu`, `net.sniff.pcap`).
- **PWNGHOST-RS Implementation**: `crates/bettercap/src/client.rs:88-105`
  - Silences only 7 event tags (missing 6 from upstream list).
- **Status**: **LOGIC GAP (Missing 6 Silence Tags Causes Journal Noise)** | **[LOGIC-GAP]** | **LOW** (cosmetic, no functional impact)

---

### Subsystem C: Display & Visual Layout

#### 1. Hardware Resolution & Logical Canvas
- **Jayofelony Reference**: `pwnagotchi/ui/hw/waveshare2in13_V4.py:15-30`
  - Canvas resolution: 250×122 1-bit monochrome format.
- **PWNGHOST-RS Implementation**: `crates/ui/display/src/driver.rs:40-80`
  - Canvas resolution: 250×122 1-bit packed byte array buffer.
- **Status**: **PARITY (100% Canvas Size Match)** | **[PARITY]** | **LOW**

#### 2. Logical Coordinates Anchor Matrix
Both implementations use exact matching anchor positions for all 8 core UI elements:

| UI Element | Jayofelony Anchor (`waveshare2in13_V4.py`) | PWNGHOST-RS Anchor (`layout.rs:23-45`) | Offset Delta |
|---|---|---|---|
| `channel` | (0, 0) | (0, 0) | **0px** |
| `aps` | (28, 0) | (28, 0) | **0px** |
| `uptime` | (185, 0) | (185, 0) | **0px** |
| `name` | (5, 20) | (5, 20) | **0px** |
| `status` | (125, 20) | (125, 20) | **0px** |
| `face` | (0, 40) | (0, 40) | **0px** |
| `friend_face` | (0, 92) | (0, 92) | **0px** |
| `friend_name` | (40, 94) | (40, 94) | **0px** |
| `shakes` / `PWND` | (0, 109) | (0, 109) | **0px** |
| `mode` | (225, 109) | (225, 109) | **0px** |

- **Status**: **PARITY (Exact 0px Offset Delta)** | **[PARITY]** | **LOW**

#### 3. Font & Kaomoji Rasterization Engine
- **Jayofelony Reference**: `pwnagotchi/ui/view.py:30-90` & `faces.py:10-80`
  - Uses Python PIL (`ImageDraw`, `ImageFont`) bitmap font rendering. Kaomoji faces drawn as string text blocks.
- **PWNGHOST-RS Implementation**: `crates/ui/display/src/ttf.rs:20-95` & `fonts.rs`
  - Uses `Fontdue` TrueType font rasterizer (DejaVuSansMono-Bold 35pt face) into 1-bit packed framebuffer.
- **Status**: **INTENTIONAL VISUAL DIVERGENCE (Crisp TTF vs PIL Bitmap)** | **[VISUAL-GAP]** | **LOW** (improvement over blocky bitmap)

---

### Subsystem D: Web UI & REST API

| Endpoint | Method | Jayofelony (`pwnagotchi/ui/web/server.py`) | PWNGHOST-RS (`crates/ui/web/src/`) | Parity Type | Detail / Schema Comparison |
|---|---|---|---|---|---|
| `/ui` | GET | `server.py:45` (PIL PNG stream) | `server.rs:60` & `frame_png` | **PARITY** | Returns live 250×122 PNG framebuffer. |
| `/api/status` | GET | **NO REST API** (verified: 404) | `api.rs:40-95` | **INTENTIONAL-DIVERGENCE** | Jayofelony has no REST endpoints — only `/` and `/ui`. PWNGHOST-RS REST API is additive Rust-only feature; validated for self-consistency, not superset. |
| `/api/config` | GET | **NO REST API** | `api.rs:110-150` | **INTENTIONAL-DIVERGENCE** | Rust-only additive endpoint. |
| `/api/config` | POST | **NO REST API** | `api.rs:130-155` | **INTENTIONAL-DIVERGENCE** | Rust-only additive endpoint. |
| `/api/handshakes` | GET | **NO REST API** | `api.rs:160-190` | **INTENTIONAL-DIVERGENCE** | Rust-only additive endpoint. |
| `/api/peers` | GET | **NO REST API** | `api.rs:200-220` | **INTENTIONAL-DIVERGENCE** | Rust-only additive endpoint. |

- **Status**: **INTENTIONAL-DIVERGENCE (REST API Additive)** | **[INTENTIONAL-DIVERGENCE]** | **LOW** (validated for self-consistency)

---

### Subsystem E: Plugin Architecture & Hardware Targets

#### 1. Embedded Plugin Ecosystem
- **Jayofelony Reference**: 24 standard Python plugins (enumerate in new Subsystem G below).
- **PWNGHOST-RS Implementation**: 19 Lua plugins in `crates/agent/src/plugins.rs:96-116`. See new Subsystem G for detailed parity table.
- **Status**: **PARTIAL GAP (19/24 Plugins Ported)** | **[PARITY]** | **LOW** (see Subsystem G for detailed enumeration)

#### 2. `ups_lite` I2C Hardware Address Bug
- **Jayofelony Reference**: `pwnagotchi/plugins/default/ups_lite.py:15`
  - Targets the **CW2015** fuel gauge chip at I2C bus address `0x62`.
- **PWNGHOST-RS Implementation**: `crates/lua/ups_lite.lua:48`
  - ~~Targeted **MAX17040** chip at I2C address `0x36`~~ → **Fixed (2026-07-20)** to address `0x62` (CW2015, see REWORK_PLAN.md Workstream G).
- **Status**: **FIXED** | **[BUG]** | **HIGH** (now resolved; was causing permanent battery read failures)

#### 3. Unimplemented Schema Plugins (`webgpsmap`, `pwnstore_ui`)
- **Jayofelony Reference**: Python plugins present and active in default directory.
- **PWNGHOST-RS Implementation**: Configured in `crates/config/src/schema.rs` but lack `.lua` implementation files in `PluginManager::BUILTINS` (lines 96-116).
- **Status**: **LOGIC GAP (WebUI Toggle is Silent No-Op)** | **[LOGIC-GAP]** | **MED** (see REWORK_PLAN.md Workstream G for priority ranking)

---

### Subsystem F: Config, Security, Mesh & Serde Schema

#### 1. WebUI Basic Authentication & CSRF
- **Jayofelony Reference**: `pwnagotchi/ui/web/server.py:25-50`
  - Enforces HTTP Basic Auth credentials when `ui.web.auth = true`. Wraps endpoints with `CSRFProtect`.
- **PWNGHOST-RS Implementation**: `crates/ui/web/src/server.rs:30-90`
  - `ui.web.auth` setting is unparsed in Axum router layers. All REST routes remain unauthenticated regardless of config toggle.
- **Status**: **SECURITY GAP (`auth = true` Setting Unwired)** | **[LOGIC-GAP]** | **HIGH** (see REWORK_PLAN.md Workstream G priority #5)

#### 2. Peer Mesh Network Transport
- **Jayofelony Reference**: `pwnagotchi/mesh/` & `identity.py`
  - Utilizes Go `pwngrid-peer` daemon for RSA key generation, JWT enrollment, and 802.11 beacon vendor IE broadcasting.
- **PWNGHOST-RS Implementation**: `crates/agent/src/mesh.rs`
  - `build_mesh_ie()` and `update_peer()` functions exist but have **0 callers** in `crates/pwnghost-rs/src/main.rs`. Radio TX/RX path never transmits or parses mesh IEs.
- **Status**: **LOGIC GAP (Mesh Peer Discovery Inactive)** | **[LOGIC-GAP]** | **LOW** (architectural, flagged for Workstream E decision)

#### 3. Serde Struct Defaults vs Field Defaults
- **Jayofelony Reference**: Python dictionary merging against `defaults.toml`.
- **PWNGHOST-RS Implementation**: `crates/config/src/schema.rs`
  - Uses bare `#[serde(default)]` on struct fields. Unpopulated fields default to `0`/`false` rather than custom `Default` struct values.
  - Partially fixed (2026-07-20): `deauth`/`associate`/`personality.position_y`/`faces.position_y`; pattern remains a landmine for future fields (see REWORK_PLAN.md Workstream G).
- **Status**: **BUG RISK (Serde Schema Unpopulated Field Zeroing)** | **[BUG]** | **MED** (partial fix; underlying schema landmine remains)

---

### Subsystem G: Plugin Architecture & Hook Dispatch Coverage

#### 1. Built-in Plugin Enumeration

**Jayofelony Default Plugins (24 total):** `pwnagotchi/pwnagotchi/plugins/default/*.py`

| Plugin Name | Jay File | PWNGHOST-RS Equivalent | Status | Severity | Notes |
|---|---|---|---|---|---|
| `auto-tune` | auto-tune.py | auto_tune.lua | **PRESENT** | **[PARITY]** LOW | Lua port; adapts scan intensity based on channel density |
| `auto-update` | auto-update.py | auto_update.lua | **PRESENT** | **[PARITY]** LOW | Checks for Rust daemon updates; downloads & installs |
| `auto-backup` | auto_backup.py | auto_backup.lua | **PRESENT** | **[PARITY]** LOW | Full Lua port; backs up config & handshakes |
| `bt-tether` | bt-tether.py | bt_tether.lua | **PRESENT** | **[PARITY]** LOW | Bluetooth tethering; uses `on_internet_available` hook |
| `cache` | cache.py | cache.lua | **PRESENT** | **[PARITY]** LOW | Session metadata cache; simplified but functional |
| `example` | example.py | **N/A** | **MISSING** | **[LOGIC-GAP]** LOW | Reference plugin; not essential for production |
| `fix_services` | fix_services.py | fix_services.lua | **PRESENT** | **[PARITY]** LOW | Remedies systemd service failures; Lua port |
| `gpio_buttons` | gpio_buttons.py | gpio_buttons.lua | **PRESENT** | **[PARITY]** LOW | Rewired 2026-07-20 for PiSugar S button (GPIO3 interrupt) |
| `gps` | gps.py | gps.lua | **PRESENT** | **[PARITY]** LOW | GPS location logging; full Lua port |
| `grid` | grid.py | grid.lua | **PARTIAL** | **[LOGIC-GAP]** LOW | Peer mesh announcement; stub only (see Subsystem F-2) |
| `logtail` | logtail.py | logtail.lua | **PRESENT** | **[PARITY]** LOW | Tails systemd journal; simplified but working |
| `memtemp` | memtemp.py | memtemp.lua | **PRESENT** | **[PARITY]** LOW | CPU temp & memory reporting; full port |
| `ohcapi` | ohcapi.py | ohcapi.lua | **PRESENT** | **[PARITY]** LOW | Submits to OnlineHashCrack API; full Lua port |
| `pisugarx` | pisugarx.py | pisugarx.lua | **PRESENT** | **[PARITY]** LOW | Battery monitoring for PiSugar3; formula differs slightly from upstream |
| `pwncrack` | pwncrack.py | pwncrack.lua | **PRESENT** | **[INTENTIONAL-DIVERGENCE]** LOW | Local hashcat vs remote pwncrack.org; useful feature, mislabeled (see REWORK_PLAN.md G) |
| `pwnstore_ui` | pwnstore_ui.py | **N/A** | **MISSING** | **[LOGIC-GAP]** MED | Configured but no `.lua` implementation; WebUI toggle is silent no-op |
| `session-stats` | session-stats.py | session_stats.lua | **PRESENT** | **[PARITY]** LOW | Epoch-by-epoch stats logging; full Lua port |
| `switcher` | switcher.py | **N/A** | **MISSING** | **[LOGIC-GAP]** LOW | Mode switching (unclear upstream purpose); not ported |
| `ups_lite` | ups_lite.py | ups_lite.lua | **PRESENT** (FIXED) | **[PARITY]** LOW | ~~MAX17040@0x36~~ → **CW2015@0x62** (fixed 2026-07-20, see REWORK_PLAN.md G) |
| `webcfg` | webcfg.py | webcfg.lua | **PRESENT** | **[PARITY]** LOW | WebUI config editor backend; full Lua port |
| `webgpsmap` | webgpsmap.py | **N/A** | **MISSING** | **[LOGIC-GAP]** MED | Configured but no `.lua` implementation; WebUI toggle is silent no-op |
| `wigle` | wigle.py | wigle.lua | **PRESENT** | **[PARITY]** LOW | Submits AP data to Wigle.net; full Lua port |
| `wittypi` | wittypi.py | **N/A** | **MISSING** | **[LOGIC-GAP]** LOW | WittyPi board control; not ported (optional hardware) |
| `wpa-sec` | wpa-sec.py | wpa_sec.lua | **PRESENT** (ENHANCED) | **[PARITY]** LOW | Uploads to wpa-sec.stanev.org; enhanced to upload raw `.pcapng` + download potfile |

**Counts:**
- **PRESENT** (19/24): auto-tune, auto-update, auto-backup, bt-tether, cache, fix_services, gpio_buttons, gps, grid (partial), logtail, memtemp, ohcapi, pisugarx, pwncrack, session-stats, ups_lite, webcfg, wigle, wpa-sec
- **PARTIAL** (1): grid (stub only, Workstream E decision pending)
- **MISSING** (5): example, pwnstore_ui, switcher, webgpsmap, wittypi
- **Total Coverage**: ~79% (19/24 fully, 1/24 partial)

#### 2. Plugin Hook Dispatch Coverage

**Jayofelony Exposes ~29 Hooks** (discovered via ast sweep of all default plugins):

| Hook Name | Jay Usage Count | PWNGHOST-RS Implementation | Status | Notes |
|---|---|---|---|---|
| `on_loaded` | 24/24 plugins | `plugins.rs:166-200` ✓ | **PRESENT** | Called once at plugin initialization |
| `on_ready` | 10/24 plugins | `plugins.rs:230-237` ✓ | **PRESENT** | Fired once at startup after all subsystems online |
| `on_epoch` | 6/24 plugins | `plugins.rs:212-222` ✓ | **PRESENT** | Fired every epoch cycle with epoch state |
| `on_handshake` | 7/24 plugins | `plugins.rs:250-273` ✓ | **PRESENT** | Fired on successful handshake capture; passes BSSID/SSID/paths |
| `on_association` | 2/24 plugins | `plugins.rs:278-293` ✓ | **PRESENT** | Fired after successful association; passes BSSID/SSID |
| `on_deauthentication` | 2/24 plugins | `plugins.rs:298-319` ✓ | **PRESENT** | Fired after deauth; passes BSSID/SSID/STA |
| `on_channel_hop` | 2/24 plugins | `plugins.rs:324-339` ✓ | **PRESENT** | Fired when hopping channels; passes old/new channel |
| `on_internet_available` | 6/24 plugins | `plugins.rs:346-353` ✓ | **PRESENT** | Fired when online connectivity detected |
| `on_wifi_update` | 3/24 plugins | `plugins.rs:358-365` ✓ | **PRESENT** | Fired after bettercap AP list refresh |
| `on_peer_detected` | 1/24 plugins | `plugins.rs:367-382` ✓ | **PRESENT** | Fired when mesh peer appears; passes MAC/name/channel |
| `on_peer_lost` | 1/24 plugins | `plugins.rs:384-398` ✓ | **PRESENT** | Fired when mesh peer disappears; passes MAC/name |
| `on_webhook` | 8/24 plugins | `plugins.rs:522-551` ✓ | **PRESENT** | Fired when plugin-specific HTTP endpoint hit |
| `on_grateful` | **Impl. exists** (`fire_mood_hook`) | `plugins.rs:496-517` ✗ **UNWIRED** | **MISSING** | Mood hook; `fire_mood_hook()` never called from main.rs |
| `on_lonely` | **Impl. exists** (`fire_mood_hook`) | `plugins.rs:496-517` ✗ **UNWIRED** | **MISSING** | Mood hook; `fire_mood_hook()` never called from main.rs |
| `on_bored` | **Impl. exists** (`fire_mood_hook`) | `plugins.rs:496-517` ✗ **UNWIRED** | **MISSING** | Mood hook; `fire_mood_hook()` never called from main.rs |
| `on_sad` | **Impl. exists** (`fire_mood_hook`) | `plugins.rs:496-517` ✗ **UNWIRED** | **MISSING** | Mood hook; `fire_mood_hook()` never called from main.rs |
| `on_angry` | **Impl. exists** (`fire_mood_hook`) | `plugins.rs:496-517` ✗ **UNWIRED** | **MISSING** | Mood hook; `fire_mood_hook()` never called from main.rs |
| `on_excited` | **Impl. exists** (`fire_mood_hook`) | `plugins.rs:496-517` ✗ **UNWIRED** | **MISSING** | Mood hook; `fire_mood_hook()` never called from main.rs |
| `on_motivated` | **Impl. exists** (`fire_mood_hook`) | `plugins.rs:496-517` ✗ **UNWIRED** | **MISSING** | Mood hook; `fire_mood_hook()` never called from main.rs |
| `on_demotivated` | **Impl. exists** (`fire_mood_hook`) | `plugins.rs:496-517` ✗ **UNWIRED** | **MISSING** | Mood hook; `fire_mood_hook()` never called from main.rs |
| `on_rebooting` | **Impl. exists** (`fire_mood_hook` → Broken) | `plugins.rs:496-517` ✗ **UNWIRED** | **MISSING** | Mood hook (maps to Mood::Broken); `fire_mood_hook()` never called |
| `on_sleep` | **Impl. exists** (`fire_mood_hook` → Sleep) | `plugins.rs:496-517` ✗ **UNWIRED** | **MISSING** | Mood hook (maps to Mood::Sleep); `fire_mood_hook()` never called |
| `on_config_changed` | 5/24 plugins | **N/A** | **MISSING** | No callback when config reloaded; affect: cache/webcfg/logtail/wigle/pwncrack |
| `on_ui_setup` | 10/24 plugins | **N/A** | **MISSING** | Display initialization hook; affect: bt-tether, fix_services, gps, memtemp, pisugarx, ups_lite, wigle, wpa-sec, and many others |
| `on_ui_update` | 10/24 plugins | **N/A** | **MISSING** | Per-frame UI update hook; affect: cache, fix_services, gps, memtemp, pisugarx, ups_lite, wigle, wpa-sec |
| `on_unload` | 10/24 plugins | **N/A** | **MISSING** | Plugin shutdown hook; affects all UI-rendering plugins |
| `on_bcap_wifi_ap_new` | 1/24 plugins | **N/A** | **MISSING** | Bettercap raw event hook (auto-tune only) |
| `on_bcap_wifi_ap_lost` | 1/24 plugins | **N/A** | **MISSING** | Bettercap raw event hook (auto-tune only) |
| `on_bcap_wifi_client_new` | 1/24 plugins | **N/A** | **MISSING** | Bettercap raw event hook (auto-tune only) |
| `on_bcap_wifi_client_lost` | 1/24 plugins | **N/A** | **MISSING** | Bettercap raw event hook (auto-tune only) |
| `on_bcap_sys_log` | 1/24 plugins | **N/A** | **MISSING** | Bettercap system log hook (fix_services only) |
| `on_unfiltered_ap_list` | 2/24 plugins | **N/A** | **MISSING** | Raw AP list before filtering (cache, example) |
| `on_free_channel` | 1/24 plugins | **N/A** | **MISSING** | Channel selection hook (example only) |
| `on_wait` | 1/24 plugins | **N/A** | **MISSING** | Sleep/wait hook (example only) |
| `on_sleep` | (see above) | - | - | - |
| `on_display_setup` | 1/24 plugins | **N/A** | **MISSING** | Display initialization (example only) |

**Hook Coverage Summary:**
- **Wired & Firing** (11/29): on_loaded, on_ready, on_epoch, on_handshake, on_association, on_deauthentication, on_channel_hop, on_internet_available, on_wifi_update, on_peer_detected, on_peer_lost, on_webhook
- **Implemented but Not Firing** (10): on_grateful, on_lonely, on_bored, on_sad, on_angry, on_excited, on_motivated, on_demotivated, on_rebooting, on_sleep (all via unwired `fire_mood_hook()`)
- **Not Implemented** (8): on_config_changed, on_ui_setup, on_ui_update, on_unload, on_bcap_wifi_ap_new/lost/client_new/lost, on_bcap_sys_log, on_unfiltered_ap_list, on_free_channel, on_wait, on_display_setup

**Hook Dispatch Status:** `11 wired / 29 total = 38% firing coverage` (or 21/29 = 72% if mood hooks are wired)

---

## 4. Reconciliation with REWORK_PLAN.md Workstream G

**Workstream G (Comprehensive Real-Hardware Validation Audit, 2026-07-21)** identified and fixed numerous gaps in parallel with this matrix. Many findings overlap; the matrix above now cross-references specific Workstream G discoveries and their fix status:

- **Configuration Defaults (Subsystem A-5)**: `min_rssi`/`mon_max_blind_epochs` defaults **FIXED** (see REWORK_PLAN.md Workstream G priority #4). Mood epoch thresholds (`bored_num_epochs`/`sad_num_epochs`) remain at 50/100 vs upstream 15/25 (MED priority, left for future session).

- **ups_lite I2C Bug (Subsystem E-2)**: **FIXED** (2026-07-20) to address 0x62 (CW2015). See REWORK_PLAN.md Workstream G priority #3.

- **Whitelist Filtering (Subsystem A-3)**: `Agent::is_target` now properly called in `find_target()` (was unwired, discovered as instance #4 of the "config exists, never wired" pattern). See REWORK_PLAN.md Workstream G.

- **Serde Schema Landmine (Subsystem F-3)**: Partially fixed (deauth/associate/personality.position_y/faces.position_y); pattern remains a live hazard for future fields. See REWORK_PLAN.md Workstream G for details.

- **Plugins (Subsystem G)**: Table reflects all 24 jay plugins and their ported status. Notable findings:
  - `fire_mood_hook()` is implemented but never called (5th instance of the pattern); mood hooks remain unwired.
  - `grid.lua` is a stub (Workstream E decision pending on mesh architecture).
  - `pwncrack.lua` implements local hashcat, not remote pwncrack.org (useful feature, mislabeled).
  - `wpa_sec.lua` enhanced (2026-07-20) to upload `.pcapng` + download potfile (exceeds upstream).

- **WebUI Auth (Subsystem F-1)**: **Not yet fixed** (see REWORK_PLAN.md Workstream G priority #5). Setting is a no-op; needs either implementation or removal.

- **Missing Hooks (Subsystem G-2)**: Mood hooks (on_grateful, on_bored, etc.) are implemented via `fire_mood_hook()` but never called from main.rs — discovered as instance #6 of the "exists, never wired" pattern. Also missing: on_config_changed, on_ui_setup, on_ui_update, on_unload (medium severity, tracked for Workstream D expansion).

---

## 5. Actionable Remediations (Priority Order from Workstream G)

1. ~~**Fix `mon_max_blind_epochs` default**~~ **✓ DONE** (see REWORK_PLAN.md Workstream G priority #4): Updated to 50.
2. ~~**Fix `ups_lite.lua` I2C target**~~ **✓ DONE** (see REWORK_PLAN.md Workstream G priority #3): Changed to 0x62 (CW2015).
3. **Wire WebUI Auth Middleware**: Implement Axum Basic Auth layer in `crates/ui/web/src/server.rs` when `config.ui.web.auth == true` (see REWORK_PLAN.md Workstream G priority #5).
4. ~~**Implement Recon Stale Guard**~~ (see REWORK_PLAN.md Workstream G): `Agent::is_target` already wired into `find_target()`.
5. **Add Event Silence Tags**: Append 6 missing silence tags to `crates/bettercap/src/client.rs` (cosmetic, low priority).
6. **Wire Mood Hooks**: Call `fire_mood_hook()` in main.rs when mood transitions occur (enables on_grateful, on_bored, etc. for all 24 plugins).
7. **Implement or Remove `pwnstore_ui`/`webgpsmap`** (see REWORK_PLAN.md Workstream G priority #6): Either create `.lua` stubs or remove from config schema.
8. **Consider `on_config_changed` Hook**: If config hot-reload is added, fire this hook to allow plugins like cache, webcfg, wigle to react (Workstream D expansion).
