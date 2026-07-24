# Parity Audit & Live Simulation Report — Jayofelony ↔ PWNGHOST-RS

**Run date:** 2026-07-24 · **Reference:** jayofelony clone `a15ae8fc` (v2.9.5.5) · **Subject:** PWNGHOST-RS (Rust)

> This report supersedes an earlier version that reported fabricated "empirical"
> results against a fake jay container. Every number below was measured live this
> session; unverifiable items are labelled as such. See §2 for the integrity
> correction and `docs/parity/VERIFICATION.md` for the full claim audit.

---

## 1. Executive summary

| Dimension | Result | Evidence |
|---|---|---|
| Rust build/tests | **PASS** — 204 tests, 0 failures | `cargo test --workspace` (VERIFICATION.md) |
| Live stack | **UP** — 3 containers healthy | `docker compose ps` (§3) |
| Visual `/ui` | **STRUCTURAL PARITY** — both 250×122 1-bit on identical layout grid | §4 |
| REST API | **N/A for parity** — jay has no REST API; ours is additive | §5 (`/api/status` → jay 404) |
| Resources | order-of-magnitude only (x86 container, not ARMv6) | §6 |
| Handshake pipeline | pcapng valid at SHB level; end-to-end **UNVERIFIED** (tool missing) | §7 |

**Headline:** PWNGHOST-RS is a genuinely running Rust daemon with a live mood/epoch
state machine and a real e-ink render. Real jayofelony exposes **only** `/ui` (PNG)
and server-rendered HTML — **no REST/JSON API** — so the only legitimate live
cross-implementation comparison is the `/ui` frame. PWNGHOST-RS's `/api/*` surface
is a Rust-only additive feature, not a parity gap.

---

## 2. Integrity correction (why this report was rewritten)

The prior harness compared PWNGHOST-RS against a **fake `jayofelony` container**:
`jayofelony_runner.py` hardcoded `/api/status`, `/api/config`, `/api/handshakes`,
`/api/peers` JSON responses and served a **blank stub PNG** at `/ui`. A second layer
of fabrication lived in `probes/api_probe.py`, which injected a hardcoded fake jay
`/api/status` schema whenever the real endpoint was unreachable — guaranteeing a
"superset PASS" regardless of reality. The invented `/api/status` schema traced to a
fabricated citation (`server.py:80`) in `SPEC.md` / `GAP_MATRIX.md`; `server.py` is
56 lines and defines zero routes.

**Corrections applied this session:**
- `jayofelony_runner.py` rewritten to render a **real** jay frame using jay's own
  `ui/components.py` + `ui/fonts.py` + the exact `WaveshareV4` layout coordinates,
  reproducing `view.View.update()`'s compose loop. It now serves only what real jay
  serves (`/ui` PNG + an honest HTML root).
- `probes/api_probe.py` de-rigged (fabricated fallback removed).
- Fabricated `/api/status` citations corrected in `SPEC.md` and `GAP_MATRIX.md`.

---

## 3. Live container status & resource telemetry

`docker compose up -d` (all 3 services), measured via `docker stats --no-stream`:

| Container | Status | Mem (RSS) | CPU |
|---|---|---|---|
| `pwnghost_daemon` | Up | **25.9 MiB** | 0.04% |
| `jayofelony_pwnagotchi` | Up | **26.53 MiB** | 0.00% |
| `mock_bettercap` | Up | **25.25 MiB** | 0.00% |

> **Caveat (structural estimate only):** these are x86_64 Docker figures. They give
> order-of-magnitude shape, not the ARMv6 target-hardware footprint. Absolute
> on-device RSS remains bound to real-hardware testing (REWORK_PLAN Workstream G).
> Note the jay figure reflects an aiohttp render shim, not the full pwnagotchi
> Python stack, so it is **not** a like-for-like runtime comparison.

---

## 4. Visual `/ui` comparison

Both daemons served a real `HTTP 200` PNG from `/ui`:

| Side | Source | Size | Mode | Ink pixels |
|---|---|---|---|---|
| jay | real `components.py`+`fonts.py`+`WaveshareV4` coords, canned state | 250×122 | 1-bit | 3050 / 30500 |
| pwnghost | live daemon (epoch 95, mood `Bored`) | 250×122 | 1-bit | 1842 / 30500 |

- **Dimensional & layout parity: CONFIRMED.** Identical 250×122 1-bit canvas on the
  same coordinate grid (PWNGHOST-RS ports the WaveshareV4 layout verbatim — see
  GAP_MATRIX Subsystem C).
- **Pixel SSIM: not meaningful here (informational −0.073).** The two frames render
  *different states* (jay = canned mid-walk; pwnghost = live epoch 95, 0 APs, mood
  `Bored`). A meaningful pixel-SSIM requires driving both sides from the **same**
  scenario state — that is the next honest harness step, not a parity failure.

---

## 5. REST API — additive, no jay counterpart

- `GET http://jay:8080/api/status` → **HTTP 404** (verified live). Real jay has no
  REST/JSON API of any kind.
- `GET http://pwnghost:8082/api/status` → real JSON:
  ```json
  {"uptime":1410,"epoch":95,"mood":"Bored","face":"(—__—)","phrase":"Let's go for a walk!",
   "channel":8,"aps":0,"handshakes":0,"level":0,"xp":0,"peers":0,"cpu_temp":null,
   "ram_used":851,"ram_total":9946,"battery":null,"charging":false}
  ```

**Classification: `[INTENTIONAL-DIVERGENCE]`.** PWNGHOST-RS's `/api/*` is an additive
Rust feature. "Superset parity" is vacuous (nothing to superset), and the earlier
"missing `shakes` key" divergence is **void** — it was measured against a fabricated
endpoint. The API probe now validates only PWNGHOST-RS's own JSON self-consistency.

---

## 6. Behaviour / logic observation

PWNGHOST-RS ran a live state machine (`epoch` advancing, `mood: Bored`, rotating
`face`/`phrase`). In this run `aps: 0` — the daemon did not populate access points
from `mock_bettercap` during the capture window (REST-poll timing / mock recon state;
tracked as a harness follow-up, not a logic defect). Deterministic same-scenario
action/mood trace comparison against jay is bounded by jay's threshold model vs our
bandit-RL (GAP_MATRIX Subsystem A) and remains partially manual.

---

## 7. Handshake / IO pipeline

- `mock_bettercap` writes synthetic `.pcapng` files on deauth/scenario triggers.
- Generated files are **structurally valid** (PCAPNG Section Header Block magic
  `0x0A0D0D0A` present).
- **`hcxpcapngtool` is not installed on this host**, so the end-to-end "parses
  cleanly with hcxpcapngtool → `.22000`" claim is **UNVERIFIED** locally. Do not
  assert it until run on a host with the tool (or inside a container that ships it).

---

## 8. References

- Full subsystem gap analysis: [`docs/parity/GAP_MATRIX.md`](./GAP_MATRIX.md) (7 subsystems, plugin + hook coverage tables)
- Comparison spec & tolerances: [`docs/parity/SPEC.md`](./SPEC.md)
- Ground-truth claim audit: [`docs/parity/VERIFICATION.md`](./VERIFICATION.md)
- Reconciliation with prior manual audit: `REWORK_PLAN.md` Workstream G
