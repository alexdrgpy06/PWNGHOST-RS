# Handoff — Master 7-Domain Parity Audit & Flashable Builds

Read this document alongside `REWORK_PLAN.md` and `walkthrough.md`.

## Executive Summary & Status
All 7 feature domains of original Python Pwnagotchi have been audited, integrated, and verified against `PWNGHOST-RS` with **100% test pass rate**:
1. **Face UI & E-Ink Rendering Engine**: Rendered using bundled `DejaVuSansMono-Bold` and `DejaVuSansMono` TTF fonts, matching the exact size, layout (`(0, 40)` face, `(125, 20)` status, `(0, 0)` CH, `(28, 0)` APS, `(185, 0)` UP), and 250x122 monochrome frame structure.
2. **Web UI REST & WebSocket Endpoints**: Verified `/api/status`, `/api/session`, `/api/config`, `/api/peers`, `/api/handshakes`, `/api/cracked`, `/api/wpa-sec/cracked`, `/api/plugins`.
3. **Core Automata & Perception**: AP perception, channel hopping, XP progression, and mood transitions (`Awake`, `Excited`, `Bored`, `Sad`, `Lonely`, `Angry`, `Sleep`).
4. **Bettercap Integration**: Basic Auth REST client, `/api/session/wifi` polling, and command dispatch (`wifi.recon.channel`, `wifi.deauth`, `wifi.assoc`, `wifi.rssi.min`).
5. **Handshake Capture Pipeline**: Candidate staging, `hcxpcapngtool` validation, and promotion to `/etc/pwnghost/handshakes`.
6. **WPA-Sec & Local Password Views**: Potfile (`bssid:clientmac:ssid:password`) and local hashcat cracked JSON view parsing.
7. **USB RNDIS Gadget & Hardware**: USB vendor/product IDs (`0x1d6b:0x0104`) configured for automatic Windows 10/11 RNDIS driver binding.

---

## Recent Fixes Applied

- **DejaVu TTF Rendering across All Display Fields**:
  Replaced legacy `FONT_6X10` bitmap font calls with `draw_ttf_line` and `draw_labeled_value_ttf`, rendering `CH`, `APS`, `UP`, `PWND`, `MODE`, `name`, and `status` with TrueType DejaVu Sans Mono.
- **Uptime Formatting (`HH:MM:SS`)**:
  Updated `uptime` string formatting from raw seconds (`123s`) to standard `HH:MM:SS` (e.g. `00:02:03`), matching OG Pwnagotchi `pwnagotchi/ui/view.py`.
- **Active Attack & Pwning Status Phrases**:
  Wired `AgentAction::Deauth` and `AgentAction::Associate` to update status phrases with `"Deauthenticating <target>..."` and `"Associating to <target>..."` as attacks dispatch to bettercap.

---

## Flashable OS Image Artifacts

The final flashable `.img.xz` builds generated via Docker are available under `tools/rebase-jayofelony/`:
- `pwnghost-rs-rebased-pi-zero-2w-2.8.9.img.xz` (Raspberry Pi Zero 2W, 64-bit / 32-bit Bullseye)
- `pwnghost-rs-rebased-pi-zero-w-2.8.9.img.xz` (Raspberry Pi Zero W, 32-bit Bullseye ARMv6)

### How to Flash:
```bash
# Decompress and flash using balenaEtcher or dd:
xz -d tools/rebase-jayofelony/pwnghost-rs-rebased-pi-zero-2w-2.8.9.img.xz
sudo dd if=tools/rebase-jayofelony/pwnghost-rs-rebased-pi-zero-2w-2.8.9.img of=/dev/sdX bs=4M status=progress conv=fsync
```

---

## Automated Verification Suite

Re-run the complete test suite locally anytime:
```powershell
# 1. Run all workspace unit and integration tests:
cargo test --workspace --lib --tests

# 2. Run master 7-domain parity audit:
python tools/run_full_parity_audit.py

# 3. Render 3x side-by-side visual comparison image:
python tools/compare_side_by_side.py
```
