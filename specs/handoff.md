# Handoff: PWNGHOST-RS — Rust pwnagotchi Full Implementation

## Context Summary

Workspace compila **sin errores** con Rust edition 2021 en Windows. Todos los tests pasan (12 tests, 0 fallos). Basado en jayofelony Bullseye para Pi 0 W OG y 02W, usando AngryOxide.

### Estado Actual
- **Workspace**: 10 crates en `crates/` en `C:\Users\alex0\pwnagotchi-rust-rework\PWNGHOST-RS\`
- **Compilación**: `cargo check --workspace` → exit 0, solo 3 warnings (unused imports, unused mut)
- **Tests**: `cargo test --workspace` → 12/12 pass
- **Modelo activo**: nvidia/nemotron-3-ultra-550b-a55b
- **Head commit**: `9bcc952` (wip: restore state + fw-patcher dep fix + SDD prep) + unstaged fixes

### Crates Implementados (parcialmente)
| Crate | Estado | Descripción |
|-------|--------|-------------|
| `pwncore` | ✅ 4 tests, funcional | Domain types: AccessPoint, Station, Handshake, Channel, Epoch, Mood, Plugin trait |
| `fw-patcher` | ⚠️ Stub | BCM43436B0 CoderFX 8-layer patches (diferido) |
| `angryoxide` | ⚠️ Stub | Wrapper de subprocess AngryOxide + JSON parser |
| `config` | ⚠️ Stub | Carga de config con figment (toml/json/env) |
| `agent` | ❌ Stub | Core agent loop (sin implementar) |
| `rl-agent` | ❌ Stub | Reinforcement learning (sin implementar) |
| `radio` | ✅ 8 tests, funcional | Radio mode manager (RAGE/BT/SAFE) |
| `ui/display` | ❌ Stub | E-ink display driver (sin implementar) |
| `ui/web` | ❌ Stub | Web UI (sin implementar) |
| `pwnghost-rs` | ❌ Stub | Binary entrypoint (sin implementar) |

### Fixes Aplicados
1. `pwncore/Cargo.toml` — añadido `mac-addr` feat `serde`, `rand`
2. `pwncore/src/lib.rs` — cambiado `std::net::MacAddr` → `mac_addr::MacAddr`, corregido `is_target` logic (blacklist override whitelist)
3. `config/Cargo.toml` — añadido `serde_json`, `figment` con features `toml/json/env/serde`
4. `angryoxide/Cargo.toml` — añadido `serde_json`, `tempfile` dev-dep
5. `radio/Cargo.toml` — añadido `nix`/`libc` opcionales
6. Caminos relativos de workspace corregidos (`../../pwncore` → `../pwncore`)
7. `config/src/migrate.rs` — añadido campos de mood a FaceConfig
8. `fw-patcher` — stub sin submódulos, edition 2021

## Files Changed
- `crates/pwncore/Cargo.toml` — deps fix
- `crates/pwncore/src/lib.rs` — MacAddr import fix, is_target logic fix, test fix
- `crates/config/Cargo.toml` — deps add
- `crates/angryoxide/Cargo.toml` — deps add
- `crates/radio/Cargo.toml` — deps add
- `crates/radio/src/lib.rs` — tests refactored (no hardware dependency)
- `crates/fw-patcher/Cargo.toml` — edition 2021
- `crates/fw-patcher/src/lib.rs` — stub sync/async

## Tests Status
- **All passing**: ✅ 12 tests (4 pwncore + 8 radio)
- **Coverage**: No medido

## Next Steps
1. **Implementar agent loop** (`crates/agent/src/lib.rs`) — core epoch loop con mood/peers/captures
2. **Implementar pwnghost-rs binary** — config → angryoxide spawn → epoch loop → save captures
3. **Implementar display driver** — e-ink (ssd1306) con frames de mood
4. **Implementar web UI** — axum + tera templates
5. **Añadir target ARM** — cross-compile para armv6/armv7
6. **Validar en Pi 0 W** — runtime en Bullseye con angryoxide real
7. **Implementar full feature parity** — deauth, PMKID, peer mesh, plugin system

## Suggested Skills for Next Agent
- `spec-driven-fullstack-development`: SDD conductor (Step 1-7 flow)
- `dispatching-parallel-agents`: Para implementar múltiples crates en paralelo
- `implement`: Implementación de SDD Step 6
- `github-pr-workflow`: Para PR lifecycle
- `subagent-driven-development`: Orquestación de subagentes

## Known Issues
1. `radio/src/lib.rs` tiene funciones async que linter marca como Rust 2015 (falso positivo — el Cargo.toml tiene edition 2021)
2. `config/src/migrate.rs` y `schema.rs` tienen ~32KLOC de código migrado del Python que necesita revisión
3. Tests de radio son pure-state (no llaman hardware real) — necesitan mock infrastructure para integration tests
4. ARM cross-compilation no configurada aún
