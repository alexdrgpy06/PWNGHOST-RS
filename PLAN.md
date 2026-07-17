# Plan: Pwnagotchi-RS Implementation

## Phase 2: Technical Implementation Plan

---

## 1. Component Dependency Graph

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           CRITICAL PATH (serial)                              │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  pi-gen stage2 (base OS)                                                    │
│       │                                                                     │
│       ▼                                                                     │
│  pi-gen stage3 (kernel + nexmon + firmware) ──────► fw-patcher crate       │
│       │                                              │                     │
│       ▼                                              ▼                     │
│  pi-gen stage4 (Rust toolchain + cross-compile)  angryoxide crate        │
│       │                                              │                     │
│       ▼                                              ▼                     │
│  pi-gen stage5 (install artifacts) ──────────► pwncore crate             │
│       │                                              │                     │
│       │                                              ▼                     │
│       │                                        config crate              │
│       │                                              │                     │
│       │                                              ▼                     │
│       │                                        agent/epoch/personality   │
│       │                                              │                     │
│       │                                              ▼                     │
│       │                                        healing + capture         │
│       │                                              │                     │
│       │                                              ▼                     │
│       │                                        rl-agent crate            │
│       │                                              │                     │
│       │                                              ▼                     │
│       │                                        radio manager             │
│       │                                              │                     │
│       │                                              ▼                     │
│       │                                        ui/display + ui/web       │
│       │                                              │                     │
│       │                                              ▼                     │
│       │                                        plugins (mlua)            │
│       │                                              │                     │
│       │                                              ▼                     │
│       └──────────────────────────► pwnagotchi-rs binary ◄────────────────┘
│                                                  │
│                                                  ▼
│                                          systemd service + image
│
└─────────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────────┐
│                        PARALLEL WORKSTREAMS (can run concurrently)           │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  WS1: pi-gen stages 2-3 (OS + kernel)          WS2: Rust workspace setup   │
│  WS3: fw-patcher crate                          WS4: angryoxide crate      │
│  WS5: pwncore + config crates                   WS6: rl-agent crate        │
│  WS7: ui/display (e-ink)                        WS8: ui/web (axum)         │
│  WS9: radio manager (wifi/bt/safe)              WS10: Lua plugins port     │
│  WS11: healing + capture pipeline               WS12: pi-gen stages 4-5    │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Implementation Order (Critical Path)

### Milestone 0: Foundation (Week 1-2)
| Task | Crate/Component | Dependencies | Verification |
|------|----------------|--------------|--------------|
| M0.1 | pi-gen stage2: bookworm armhf base | — | Boot QEMU armhf |
| M0.2 | pi-gen stage3: kernel 6.6 + nexmon build | M0.1 | `modprobe brcmfmac` works |
| M0.3 | Rust workspace + Cargo.toml (all crates) | — | `cargo check --workspace` |
| M0.4 | Cross-compile targets (armv6 + armv7) | M0.3 | `cargo build --target=arm-unknown-linux-gnueabihf` |

### Milestone 1: Firmware & WiFi Engine (Week 2-3)
| Task | Crate/Component | Dependencies | Verification |
|------|----------------|--------------|--------------|
| M1.1 | `fw-patcher`: manifest parsing + hash validation | M0.3 | Unit tests pass |
| M1.2 | `fw-patcher`: inplace-v7.txt parser + atomic apply | M1.1 | Patch round-trip test |
| M1.3 | `fw-patcher`: BCM chip detection (dtb/dmesg) | M1.1 | Detects 43436B0 vs 43430 |
| M1.4 | `fw-patcher`: GPIO power cycle (WL_REG_ON) | M1.2 | Power cycles chip |
| M1.5 | `fw-patcher`: embed `wlan_keepalive` binary | M1.2 | Daemon starts, binds wlan0mon |
| M1.6 | `angryoxide`: subprocess spawn + crash detection | M0.3 | Spawns, captures stdout |
| M1.7 | `angryoxide`: JSON line parser (AP, handshake, stats) | M1.6 | Parses real AO output |
| M1.8 | `angryoxide`: CLI arg builder from config | M1.7 | Generates valid AO args |
| M1.9 | `angryoxide`: exponential backoff recovery | M1.6 | Restarts on crash |

### Milestone 2: Core Domain & Config (Week 3-4)
| Task | Crate/Component | Dependencies | Verification |
|------|----------------|--------------|--------------|
| M2.1 | `pwncore`: AP, Station, Handshake, Channel types | M0.3 | Serde round-trip tests |
| M2.2 | `pwncore`: Epoch, Mood, Personality types | M2.1 | State machine tests |
| M2.3 | `config`: schema.rs (full pwnagotchi TOML) | M2.1 | Loads defaults.toml |
| M2.4 | `config`: migrate.rs (legacy → new) | M2.3 | Migrates jayofelony config |
| M2.5 | `config`: validation + defaults | M2.3 | Rejects invalid config |

### Milestone 3: Agent Core (Week 4-5)
| Task | Crate/Component | Dependencies | Verification |
|------|----------------|--------------|--------------|
| M3.1 | `agent`: epoch.rs (tracking, mood transitions) | M2.2, M1.7 | Mood triggers match personality.toml |
| M3.2 | `agent`: personality.rs (params → behavior) | M3.1 | Config drives recon_time, throttle |
| M3.3 | `agent`: faces.rs (21 kaomoji, triggers) | M2.2 | Face matches mood state |
| M3.4 | `agent`: agent.rs (FSM: Recon→Attack→Hop) | M1.7, M3.1 | Runs 3 epochs vs mock AO |
| M3.5 | `agent`: mesh.rs (peer IE advertise/parse) | M3.4 | Peers visible in web UI |
| M3.6 | `agent`: recovery.rs (persist/load epoch) | M3.4 | Survives reboot with state |

### Milestone 4: Self-Healing & Capture (Week 5-6)
| Task | Crate/Component | Dependencies | Verification |
|------|----------------|--------------|--------------|
| M4.1 | `agent`: healing.rs (6-layer state machine) | M1.4, M1.9 | Triggers on injected failures |
| M4.2 | `agent`: capture.rs (tmpfs → .22000 + .pcapng) | M1.7 | Validates handshake quality |
| M4.3 | Integration: healing + AO watchdog + GPIO | M4.1, M1.9 | Full crash→recovery cycle |

### Milestone 5: RL Agent (Week 6-7)
| Task | Crate/Component | Dependencies | Verification |
|------|----------------|--------------|--------------|
| M5.1 | `rl-agent`: model.rs (LSTM + MLP in burn) | M0.3 | Model compiles |
| M5.2 | `rl-agent`: features.rs (state → tensor) | M2.1, M3.1 | 49-dim observation |
| M5.3 | `rl-agent`: agent.rs (ActorCritic select_action) | M5.1, M5.2 | Picks action in <10ms |
| M5.4 | `rl-agent`: checkpoint.rs (safetensors load) | M5.1 | Loads quantized INT8 |
| M5.5 | Training pipeline (burn) → export INT8 | M5.1 | Produces ~500KB model |
| M5.6 | Integration: agent calls RL each epoch | M3.4, M5.3 | RL drives channel/attack |

### Milestone 6: Radio Manager (Week 7-8)
| Task | Crate/Component | Dependencies | Verification |
|------|----------------|--------------|--------------|
| M6.1 | `radio`: wifi.rs (monitor up/down, nexmon) | M1.1 | Switches to monitor mode |
| M6.2 | `radio`: patchram.rs (BCM43436B0 firmware load) | M1.1 | Loads patchram, BT ready |
| M6.3 | `radio`: bluetooth.rs (BlueZ DBus, PAN tether) | M6.2 | Phone pairs, gets internet |
| M6.4 | `radio`: safe.rs (managed wpa_supplicant) | M6.1 | Connects to known WiFi |
| M6.5 | `radio`: manager.rs (atomic RAGE↔BT↔SAFE) | M6.1-M6.4 | 3-mode switch <5s |

### Milestone 7: UI (Week 8-9)
| Task | Crate/Component | Dependencies | Verification |
|------|----------------|--------------|--------------|
| M7.1 | `ui/display`: driver.rs (SSD1306 async SPI) | M0.3 | Draws test pattern |
| M7.2 | `ui/display`: layout.rs (face, bars, gauges) | M7.1, M3.3 | Renders all 21 faces |
| M7.3 | `ui/display`: fonts.rs (embedded DejaVu + kaomoji) | M7.1 | Text renders crisp |
| M7.4 | `ui/web`: server.rs (axum + WS) | M0.3 | Serves on :8080 |
| M7.5 | `ui/web`: api.rs (REST endpoints) | M7.4 | `/api/session` returns JSON |
| M7.6 | `ui/web`: ws.rs (live updates) | M7.4 | Handshakes push to browser |
| M7.7 | `ui/web`: templates (Tera) | M7.5 | Dashboard renders |

### Milestone 8: Lua Plugins (Week 9)
| Task | Crate/Component | Dependencies | Verification |
|------|----------------|--------------|--------------|
| M8.1 | `agent`: plugins.rs (mlua sandbox + API) | M3.4 | Loads hello.lua |
| M8.2 | Port 20 plugins from Python/Lua | M8.1 | Each plugin loads |
| M8.3 | Plugin config via TOML | M8.2 | `config['main']['plugins'][name]` |

### Milestone 9: Integration & Image (Week 10-11)
| Task | Crate/Component | Dependencies | Verification |
|------|----------------|--------------|--------------|
| M9.1 | `pwnagotchi-rs`: main.rs (wiring all crates) | All prior | Binary runs |
| M9.2 | `pwnagotchi-rs`: boot.rs (first-boot fw patch) | M1.5 | Applies patch on boot |
| M9.3 | pi-gen stage4: cross-compiled artifacts | M0.4 | All crates in sysroot |
| M9.4 | pi-gen stage5: install + systemd units | M9.1, M9.3 | Service starts on boot |
| M9.5 | Full image build + QEMU test | M9.4 | Boots to dashboard |
| M9.6 | Hardware test: Pi Zero W + Pi Zero 2W | M9.5 | All success criteria met |

---

## 3. Risk Mitigation

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| AngryOxide CLI/JSON changes | High | High | Pin AO version; parser resilience (ignore unknown fields); integration test with canned stdout |
| nexmon build fails on bookworm 6.6 | Medium | Critical | Pre-build nexmon .ko in CI; vendor kernel headers; fallback to prebuilt |
| Candle LSTM too slow on ARMv6 | Medium | High | Quantize to INT8; benchmark early (M5.1); heuristic fallback mandatory |
| BCM43436B0 patchram binary missing | Medium | High | Cross-compile `brcm_patchram_plus` for armhf; include in fw-patcher data |
| BlueZ PAN tethering unreliable | Medium | Medium | Test with multiple phones; fallback to USB gadget (g_ether) |
| pi-gen build breaks on upstream changes | Medium | Medium | Fork pi-gen at known-good commit; pin base image |
| Lua plugin sandbox escape | Low | Critical | `mlua` sandbox mode; no `os`/`io`/`debug` libs; audit all plugins |
| A2C training diverges | Medium | Medium | Use PPO as backup; ship heuristic-only if RL fails |
| e-ink display driver issues | Low | Medium | Test with Waveshare 2.13" V4 first; abstract driver for variants |
| SD card corruption from writes | Medium | Medium | tmpfs for all captures; zram for logs; read-only rootfs option |

---

## 4. Parallelizable Workstreams

| Workstream | Owner | Crates | Can Start | Blocks |
|------------|-------|--------|-----------|--------|
| **WS-A: pi-gen OS/Kernel** | Platform | pi-gen stage2-3 | Day 1 | M1.1, M9.3 |
| **WS-B: Rust Workspace** | Backend | All crates (skeleton) | Day 1 | All Rust work |
| **WS-C: Firmware Patcher** | Backend | fw-patcher | M0.3 | M1.1-M1.5 |
| **WS-D: AngryOxide Wrapper** | Backend | angryoxide | M0.3 | M1.6-M1.9 |
| **WS-E: Core Domain** | Backend | pwncore, config | M0.3 | M2.1-M2.5 |
| **WS-F: RL Agent** | ML | rl-agent | M0.3 | M5.1-M5.6 |
| **WS-G: E-ink Display** | Embedded | ui/display | M0.3 | M7.1-M7.3 |
| **WS-H: Web UI** | Frontend | ui/web | M0.3 | M7.4-M7.7 |
| **WS-I: Radio Manager** | Systems | radio | M1.1 | M6.1-M6.5 |
| **WS-J: Lua Plugins** | Backend | agent/plugins | M3.4 | M8.1-M8.3 |
| **WS-K: Healing/Capture** | Backend | agent/healing, capture | M1.9, M3.4 | M4.1-M4.3 |

**Critical path**: WS-A → WS-C → WS-D → WS-E → WS-K → M3.4 → M5.6 → M6.5 → M9.1 → M9.5

**Can parallelize**: WS-B, WS-F, WS-G, WS-H, WS-I, WS-J all start after M0.3

---

## 5. Verification Gates (Phase Transitions)

| Gate | Criteria | Command/Check |
|------|----------|---------------|
| **G0: Foundation** | pi-gen boots armhf QEMU; Rust workspace compiles both targets | `qemu-system-arm -M raspi0 -kernel ...` + `cargo check --workspace --target=arm...` |
| **G1: Firmware+AO** | fw-patcher applies patch on real HW; AO spawns + parses JSON | `cargo test -p fw-patcher -- --ignored` + `cargo test -p angryoxide` |
| **G2: Core Domain** | Config loads, migrates, validates; types serialize | `cargo test -p pwncore -p config` |
| **G3: Agent Loop** | Mock AO → 3 epochs → mood transitions → recovery persist | `cargo test -p agent --test integration` |
| **G4: Healing** | Injected AO crash → backoff → GPIO cycle → recovery | `cargo test -p agent --test healing -- --ignored` |
| **G5: RL Inference** | Model loads, infers <10ms on Pi Zero W (ARMv6) | `cargo test -p rl-agent --test bench -- --ignored` |
| **G6: Radio Switch** | RAGE→BT→SAFE→RAGE cycle completes <5s each | `cargo test -p radio --test integration -- --ignored` |
| **G7: UI** | E-ink shows faces; Web UI serves dashboard + WS | `cargo test -p ui --test integration -- --ignored` |
| **G8: Plugins** | All 20 plugins load in sandbox, handle events | `cargo test -p agent --test plugins` |
| **G9: Image Boot** | pi-gen image boots QEMU → service up → web UI reachable | `./scripts/qemu-test.sh` |
| **G10: Hardware** | All success criteria met on Pi Zero W + Pi Zero 2W | Physical test checklist |

---

## 6. Resource Estimates

| Resource | Estimate |
|----------|----------|
| **Total duration** | 11 weeks (1 dev) / 6 weeks (2 devs + 1 ML) |
| **CI/CD** | GitHub Actions: cross-compile (ARM runners), pi-gen (self-hosted ARM), QEMU test |
| **Storage** | ~50GB for pi-gen build cache + docker layers |
| **Hardware for test** | 1× Pi Zero W, 1× Pi Zero 2W, 1× Waveshare 2.13" V4, 1× PiSugar 3, 1× Android/iOS for BT tether |
| **Key dependencies** | `angryoxide` v0.9.2 (pin), `nexmon` (bookworm branch), `candle` 0.8+, `burn` 0.15+ |

---

## 7. Next Steps (Phase 3: Tasks)

Upon approval, break each Milestone into discrete tasks with:
- [ ] Task description
- Acceptance criteria
- Verification command
- Files touched (≤5 per task)
- Estimated hours

Then execute via incremental implementation (Phase 4) with TDD.