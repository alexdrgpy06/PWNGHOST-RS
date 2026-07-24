# Spec: Parity Analysis & Simulation (PWNGHOST-RS ↔ Jayofelony)

## Objective
The objective is to analyze the complete jayofelony pwnagotchi system and measure how our Rust reimplementation (pwnghost-rs) differs from it in terms of logic, processes, and visual output. We need to methodically validate this parity using a deterministic, multi-process simulation harness where a mock bettercap server, the jayofelony reference implementation, and pwnghost-rs run simultaneously in Docker containers. We must provide extensive documentation and explicit visual proof of the system's execution.

## Tech Stack
- **PWNGHOST-RS**: Rust (1.70+), Cargo, Debian Bullseye base.
- **Jayofelony Pwnagotchi**: Python 3.11, Debian Bullseye base.
- **Simulation/Orchestration**: Docker, Docker Compose, bash scripts.
- **Mock Bettercap**: Python 3.11 with `aiohttp` for REST + WebSocket simulation.
- **Probes**: Python 3.11 with `requests`, `numpy`, `skimage` (for SSIM visual analysis).

## Commands
```bash
# Build and Run the entire simulation stack
cd tools/parity && ./run.sh

# Stop and clean up the simulation stack
cd tools/parity/compose && docker compose down -v

# Run the API and Visual Probes against the active simulation
python3 tools/parity/probes/api_probe.py
python3 tools/parity/probes/visual_probe.py
```

## Project Structure
```
tools/parity/
├── compose/
│   ├── docker-compose.yml       → Orchestration for the 3-container stack
│   ├── Dockerfile.pwnghost      → Rust daemon container
│   ├── Dockerfile.jayofelony    → Python reference container
│   ├── Dockerfile.mock          → Mock bettercap container
│   └── jayofelony_runner.py     → Python entrypoint for the reference server
├── mock_bettercap/
│   └── server.py                → Dual-protocol (REST/WS) bettercap simulator
├── probes/
│   ├── api_probe.py             → Validates REST API parity (superset constraint)
│   └── visual_probe.py          → Captures /ui images and calculates SSIM
└── run.sh                       → Master execution script
```

## Code Style
All Python scripts should be typed and handle network errors gracefully:
```python
import requests
from typing import Dict, Any

def fetch_status(url: str) -> Dict[str, Any]:
    try:
        response = requests.get(url, timeout=5)
        response.raise_for_status()
        return response.json()
    except requests.RequestException as e:
        print(f"Error fetching status from {url}: {e}")
        return {}
```
Rust code follows standard `rustfmt` conventions and uses `tokio` for async operations.

## Testing Strategy
- **Simulation Harness**: Integration testing via Docker Compose, verifying all 3 containers start and stay healthy.
- **API Validation Testing**: `api_probe.py` validates that PWNGHOST-RS REST endpoints (`/api/status`, `/api/config`, `/api/handshakes`, `/api/peers`) return valid JSON with documented keys. Note: jayofelony has no REST API (verified: `GET /api/status` → HTTP 404); PWNGHOST-RS REST API is an additive feature [INTENTIONAL-DIVERGENCE].
- **Visual Parity Testing**: `visual_probe.py` asserts SSIM ≥ 92% and 0px coordinate offsets against the `/ui` endpoint (the only live cross-implementation surface).
- **Unit Testing**: `cargo test --workspace --lib` for the Rust daemon.
- **Visual Proof**: The simulation MUST generate queryable logs and save actual `.png` artifacts from the `/ui` endpoints to prove the web servers are functioning.

## Boundaries
- **Always do:** Capture logs from all three containers to prove they are running and interacting. Wait for services to be healthy before running probes. Provide visual/saved output as proof.
- **Ask first:** Modifying the core logic of `pwnghost-rs` to match jayofelony if it fundamentally breaks the existing Rust architecture.
- **Never do:** Falsify parity results or skip the actual execution of the jayofelony and bettercap containers.

## Success Criteria
1. The 3-container stack (`mock_bettercap`, `jayofelony`, `pwnghost-rs`) builds and runs simultaneously without crashing.
2. Extensive logs from `docker compose logs` prove that all three containers are actively communicating.
3. The `visual_probe.py` successfully connects to both `/ui` endpoints, retrieves valid image data, saves the images to disk (as visual proof), and computes an SSIM score.
4. The `api_probe.py` successfully validates PWNGHOST-RS `/api/status` returns valid JSON with documented keys (`name`, `status`, `epoch`, `aps`, `shakes`, `mode`, and additive keys). Jayofelony provides no REST API equivalent (verified: `/api/status` → 404).
5. PWNGHOST-RS REST API is validated for self-consistency and additive scope (not superset against a non-existent endpoint).

## Open Questions
1. The `visual_probe.py` previously failed to identify the image file. Should we add a step to dump the raw HTTP response body from the mock and jayofelony to disk if parsing fails, so we can inspect exactly what is being returned?
2. Are there specific logs or states in `jayofelony` that we must explicitly trigger (e.g., forcing a mock handshake) to consider the visual proof "complete"?
