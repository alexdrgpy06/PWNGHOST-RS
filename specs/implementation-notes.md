# Implementation Notes: PWNGHOST-RS

## Decisions Made
1. **Workspace structure**: 10 crates flat under `crates/` — avoids deep nesting
2. **Domain types in pwncore**: Single source of truth for AccessPoint, Station, Handshake, Channel, Epoch, Mood, Plugin trait
3. **Radio mode manager**: State machine with RAGE/BT/SAFE modes, tested pure-state (no hardware deps)
4. **Edge-case in is_target**: Blacklist overrides whitelist (original pwnagotchi behavior)
5. **fw-patcher deferred**: Full CoderFX 8-layer patch implementation postponed — stub compiles clean
6. **Edition 2021**: All crates use Rust 2021 edition for async support

## Gotchas / Edge Cases
1. **Windows paths**: MSYS path translation (`/c/Users/...`) vs Windows paths (`C:\Users\...`) — Cargo resuelve correctamente
2. **Linter false positives**: linter marca `async fn` como Rust 2015 en `radio/src/lib.rs` — el compilador real (cargo check/test) lo acepta con edition 2021
3. **Test infrastructure**: Los tests de radio evitan hardware real — necesitan mock traits para integration tests futuros
4. **`angryoxide/Cargo.toml` path**: `path = "../pwncore"` desde `crates/angryoxide/` — no `../../pwncore`

## Deviations from Plan
- `radio/src/lib.rs` tests refactored: los integration tests originales con `switch_to()` real fueron reemplazados por pure-state tests (serde roundtrip, enum equality, state machine) porque el hardware WiFi no está disponible en Windows
- `fw-patcher` submodules (detect.rs, gpio.rs, keepalive.rs, manifest.rs, monitor.rs, patch.rs) eliminados del árbol — stub puro

## Performance / Security Notes
- ML deps (candle, burn) están en workspace deps pero no son necesarios aún — pueden ser feature-gated cuando se implemente rl-agent
- N/A aun

## Verifiable Artifacts
- `cargo check --workspace` → exit 0, 3 warnings
- `cargo test --workspace` → 12/12 pass
- Archivos modificados en working tree (unstaged)
