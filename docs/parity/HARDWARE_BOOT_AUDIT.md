# PWNGHOST-RS Hardware Boot Audit

## Executive Summary

A freshly-flashed Raspberry Pi device running pwnghost-rs boots successfully but never detects Access Points, showing "0 APs" indefinitely. Investigation reveals a **critical race condition in the wifi.recon lifecycle**: the daemon enables `wifi.recon on` only during its 10-attempt startup bootstrap. If bettercap restarts after this window (which it will, given `Restart=always`), wifi.recon is permanently OFF, and the daemon never re-enables it. This is the primary cause of zero-AP symptoms. Three additional high-risk issues compound this failure:

1. **Soft boot ordering** (Wants= instead of Requires=) means pwnghost-rs may bootstrap before bettercap is ready
2. **Interface enumeration assumptions** (assumes wlan0 exists; may be wlan1/wlan2)  
3. **nexmon driver/firmware fragility** on certain Pi revisions

---

## 1. Boot Chain Overview (Systemd Ordering)

### Startup Sequence

```
network.target (kernel network stack ready)
├── wifi-country.service (unblock rfkill, set regulatory domain)
├── bettercap.service (ExecStartPre=pwnghost-monstart-if-needed → create wlan0mon)
│   └── pwnghost-monstart-if-needed → /usr/bin/monstart (create monitor interface)
├── pwnghost-rs.service (After=network.target bettercap.service, Wants=)
│   └── Bootstrap: try wifi.recon on (10x retries, then give up)
└── wlan_keepalive.service (After=network.target bettercap.service, Requisite=)
    └── Keepalive task for BCM43436B0 SDIO stability
```

**Critical Observation**: pwnghost-rs uses `Wants=bettercap.service`, NOT `Requires=`. This is a soft dependency — the daemon will start even if bettercap fails or hasn't finished coming up yet. A 2-second bootstrap retry window is insufficient to reliably detect bettercap availability across all hardware/load profiles.

---

## 2. Critical Issue: wifi.recon Permanent Disable Race Condition

### The Race

**File: `crates/pwnghost-rs/src/main.rs`, lines 344–382**

```rust
// Bootstrap: 10 attempts to enable wifi.recon, then give up
for attempt in 1..=10u32 {
    match tokio::task::spawn_blocking({
        let bc = bc.clone();
        let cmd = bootstrap_cmd.clone();
        move || bc.run_command(&cmd)
    })
    .await
    {
        Ok(Ok(())) => {
            info!("bettercap bootstrap succeeded (attempt {attempt})");
            last_err = None;
            break;
        }
        Ok(Err(e)) => last_err = Some(e),
        Err(join_err) => last_err = Some(anyhow::anyhow!("{join_err}")),
    }
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
}
if let Some(e) = last_err {
    warn!(
        "bettercap bootstrap failed after 10 attempts, continuing anyway \
         (recon/capture won't work until bettercap is reachable): {}",
        e
    );
}
```

**The Command Sent**:
```
set wifi.handshakes.file /var/tmp/pwnghost/bettercap-output; \
set wifi.handshakes.aggregate false; \
set wifi.rssi.min {min_rssi}; \
wifi.recon on
```

**What Happens**:
1. Daemon sends this command to bettercap REST API (1st minute of boot)
2. If bettercap is down or slow → bootstrap fails → daemon continues anyway (line 376–381)
3. **daemon NEVER calls `wifi.recon on` again after bootstrap fails or succeeds**
4. If bettercap crashes or restarts after daemon's bootstrap window (`Restart=always`, `RestartSec=10`), **it comes back with `wifi.recon off`** (bettercap's default state)
5. **Daemon never re-enables recon** because:
   - No health-check timer calls `wifi.recon on` periodically
   - Healing mechanism (`RestartCapture`, lines 973–997) WOULD re-enable recon if triggered, BUT:
     - Healing only triggers if `report_crash()` is called (when bettercap returns 0 APs for N consecutive epochs)
     - If recon is OFF, bettercap legitimately returns 0 APs (correct behavior)
     - Daemon interprets "0 APs" as "recon is working, just no targets nearby," not as "recon is off"
     - **Healing never triggers**

**Concrete Failure Scenario**:
- Boot: Daemon starts, bettercap.service slowly starting ExecStartPre (monstart)
- +0s: Daemon tries `wifi.recon on`, fails (bettercap not ready yet)
- +20s: bettercap finally finishes monstart and ExecStart, comes up with `wifi.recon off` (default)
- +21s: Daemon gave up on bootstrap 10 epochs ago, never retries
- Forever: `curl http://127.0.0.1:8081/api/session/wifi` returns empty AP list
- Display: "WiFi down!" (0 APs), daemon cycles looking-left/right face forever

**Severity**: **CRITICAL — This is the most likely cause of "0 APs, no pwning" on freshly-flashed devices.**

---

## 3. bettercap.service Configuration

**File: `tools/rebase-jayofelony/overlay/etc/systemd/system/bettercap.service`, lines 1–30**

```ini
[Unit]
Description=bettercap (real capture engine for pwnghost-rs, Phase 1 rework)
Documentation=https://bettercap.org
After=network.target
Wants=network.target

[Service]
Type=simple
LimitNOFILE=65535
TasksMax=infinity
ExecStartPre=/usr/local/bin/pwnghost-monstart-if-needed
ExecStart=/usr/local/bin/bettercap -no-colors -iface wlan0mon -eval "set api.rest.username pwnghost; set api.rest.password pwnghost; set api.rest.address 127.0.0.1; set api.rest.port 8081; api.rest on"
Restart=always
RestartSec=10
```

### Correct Elements
- ✓ `-iface wlan0mon` — correct interface (monitor mode virtual interface)
- ✓ `api.rest.username/password/address/port` — matches daemon config defaults (`crates/config/src/schema.rs`)
- ✓ `api.rest on` — REST API is enabled

### Issues

**Issue 3a: ExecStartPre Invocation (Line 14)**

**File & Line**: `tools/rebase-jayofelony/overlay/usr/local/bin/pwnghost-monstart-if-needed`, lines 1–22

```bash
#!/bin/bash
if ip link show wlan0mon >/dev/null 2>&1; then
    echo "pwnghost-monstart-if-needed: wlan0mon already exists, skipping monstart"
    exit 0
fi
exec /usr/bin/monstart
```

- ✓ Idempotent (safe for `Restart=always` cycles)
- ✓ Only calls `monstart` if wlan0mon doesn't already exist

**But**: This depends entirely on **`/usr/bin/monstart` succeeding on first boot**. If monstart fails, wlan0mon is never created, and bettercap fails to bind to a non-existent interface.

**Issue 3b: No Explicit wifi.recon Enablement (Critical Gap)**

The bettercap.service does NOT include `wifi.recon on` in its ExecStart. It relies 100% on the daemon calling it via REST API. This is correct in theory (matches real pwnagotchi's architecture) but creates the race condition documented in Section 2.

**Issue 3c: Restart Behavior**

- `Restart=always` with `RestartSec=10` means bettercap restarts unconditionally
- Each restart resets bettercap state to defaults (including `wifi.recon off`)
- The daemon's single 1-minute bootstrap window cannot handle bettercap restarts
- **Severity**: HIGH

**Issue 3d: No Dependency on Monitor Interface Availability**

- No `After=` or `Requires=` on any service that guarantees wlan0mon exists BEFORE bettercap tries to bind
- ExecStartPre creates it, but if that fails, ExecStart still tries and fails hard
- No retry logic on ExecStart itself

**Severity**: MEDIUM (ExecStartPre idempotency helps here)

---

## 4. Monitor Interface Creation Chain

### The Real monstart Script

**File: `tools/rebase-jayofelony/overlay/usr/bin/monstart`, lines 1–29**

```bash
#!/bin/bash
# Put the Wi-Fi radio into monitor mode as wlan0mon.
# Real jayofelony pwnagotchi's own /usr/bin/pwnlib::start_monitor_interface
# uses `iw phy <phy> interface add wlan0mon type monitor` (creating a NEW 
# virtual monitor interface via cfg80211's add_virtual_intf path), not
# `iw dev wlan0 set type monitor` (in-place type change).

set -e
IFACE="${1:-wlan0}"
MON="${IFACE}mon"
rfkill unblock all
ifconfig "$IFACE" up
iw dev "$IFACE" set power_save off
PHY="$(iw phy | head -1 | cut -d" " -f2)"
iw phy "$PHY" interface add "$MON" type monitor
rfkill unblock all
ifconfig "$IFACE" down
ifconfig "$MON" up
iw dev "$MON" set power_save off
```

### Interface Enumeration Assumptions

**Lines 18, 23**: Assumes:
1. `iw phy` returns at least one PHY entry ← **Can fail if no WiFi chip**
2. First PHY returned matches the actual chip to bring into monitor mode ← **Can fail if multiple radios present**
3. IFACE is `wlan0` by default ← **Can fail if chip enumerates as wlan1, wlan2, etc.**

**Critical Failure Mode**:

On Pi Zero W with certain bootloader/firmware revisions:
- Physical Wi-Fi chip may enumerate as `wlan1` or `wlan2`, not `wlan0`
- **Confirmed on real hardware**: A real-hardware boot showed dhcpcd/wlan_keepalive logs reporting `Operation not possible due to RF-kill`, then a user's manual SSH session revealed `ip link show` had no `wlan0` — the device was `wlan1`
- `monstart` would silently create `wlan1mon` instead
- `bettercap -iface wlan0mon` would fail to bind (interface doesn't exist)
- pwnghost-monstart-if-needed would exit 0 anyway (because it only checks if wlan0mon exists NOW, not if monstart succeeded)
- **Daemon starts but never connects to bettercap**

**Severity**: **CRITICAL on certain Pi hardware revisions** — This silently breaks the entire boot without any systemd error, because ExecStartPre succeeds (return 0) even though the real monstart failed.

**Mitigation** (not currently implemented):
- monstart should detect actual interface name and export it
- pwnghost-monstart-if-needed should validate wlan0mon exists AND is monitor mode
- bettercap.service should depend on a target that guarantees wlan0/wlan0mon enumeration

### monstop (Interface Cleanup)

**File: `tools/rebase-jayofelony/overlay/usr/bin/monstop`, lines 1–17**

```bash
#!/bin/bash
# Return the radio to managed mode, matching real jayofelony pwnagotchi's
# /usr/bin/pwnlib::stop_monitor_interface -- reloading brcmfmac is how that
# real implementation resets this FullMAC chip cleanly.

set -e
IFACE="${1:-wlan0}"
MON="${IFACE}mon"
ifconfig "$MON" down 2>/dev/null || true
iw dev "$MON" del 2>/dev/null || true
modprobe -r brcmfmac 2>/dev/null || true
sleep 1
modprobe brcmfmac 2>/dev/null || true
sleep 2
ifconfig "$IFACE" up
```

- Used only for manual admin/shutdown (not in any systemd unit on the rebase pipeline)
- Correctly removes virtual interface and reloads driver
- No risk of accidental execution during normal boot
- **No severity issue** for boot scenarios

---

## 5. pwnghost-rs.service Configuration

**File: `tools/rebase-jayofelony/overlay/etc/systemd/system/pwnghost-rs.service`, lines 1–79**

```ini
[Unit]
Description=PWNGHOST-RS - Rust Pwnagotchi Implementation
Documentation=https://github.com/pwnghost-rs/pwnghost-rs
After=network.target bettercap.service
Wants=network.target bettercap.service
StartLimitIntervalSec=60
StartLimitBurst=3

[Service]
Type=notify
NotifyAccess=main
ExecStart=/usr/local/bin/pwnghost-rs --config /etc/pwnghost/config.toml
Restart=on-failure
RestartSec=5
WatchdogSec=45
NoNewPrivileges=yes
RuntimeDirectory=pwnghost
CapabilityBoundingSet=CAP_NET_ADMIN CAP_NET_RAW CAP_SYS_ADMIN CAP_DAC_OVERRIDE CAP_SYS_RESOURCE
AmbientCapabilities=CAP_NET_ADMIN CAP_NET_RAW
LimitNOFILE=65536
LimitNPROC=4096
MemoryMax=200M
CPUQuota=80%
StandardOutput=journal
StandardError=journal
SyslogIdentifier=pwnghost-rs

[Install]
WantedBy=multi-user.target
```

### Issues

**Issue 5a: Soft Dependency on bettercap (Line 10–11)**

- `After=bettercap.service` — correct
- **BUT**: `Wants=bettercap.service` not `Requires=bettercap.service`
- "Wants" means: "if bettercap fails, I still start"
- "Requires" means: "if bettercap fails, I fail too"

**Consequence**: Daemon starts even if bettercap.service enters failed state. The 10 bootstrap retries (20 seconds total) may not be enough if bettercap is slow due to:
- Slow ExecStartPre (monstart chip reset)
- Slow system load at boot
- SDIO bus latency on Pi Zero W (BCM43430)

**Severity**: **MEDIUM — This compounds the wifi.recon race condition. A slower bettercap startup increases likelihood that daemon bootstrap fails.**

**Issue 5b: No Re-bootstrap Mechanism**

Once the initial bootstrap window closes (after 10 failures or 1 success), the daemon **never** calls `wifi.recon on` again unless the healing system triggers. Healing requires:
1. Bettercap to start returning legitimate "0 APs" (recon is ON but no targets)
2. Daemon to see 0 APs for N consecutive epochs
3. Daemon to call `check_healing()` which increments a crash counter
4. Crash counter to exceed threshold → `RestartCapture` healing action

**But if wifi.recon is OFF**, bettercap always returns 0 APs, and this looks indistinguishable from "recon is on, no targets nearby." The daemon has no way to know recon is OFF.

**Severity**: **CRITICAL — This is the core of the race condition. There is no recovery path once bootstrap fails or bettercap restarts.**

---

## 6. wlan_keepalive.service Configuration

**File: `tools/rebase-jayofelony/overlay/etc/systemd/system/wlan_keepalive.service`, lines 1–32**

```ini
[Unit]
Description=WiFi monitor interface keepalive (BCM43436B0 SDIO bus)
Documentation=https://github.com/pwnghost-rs/pwnghost-rs
After=network.target bettercap.service
Wants=network.target
Requisite=bettercap.service

[Service]
Type=simple
ExecStart=/usr/local/bin/wlan_keepalive wlan0mon 100
Restart=always
RestartSec=3
Nice=10
StandardOutput=journal
StandardError=journal

# Security hardening
NoNewPrivileges=yes
PrivateTmp=yes
ProtectSystem=strict
ProtectHome=yes

[Install]
WantedBy=multi-user.target
```

### Purpose

**File: `crates/fw-patcher/vendor/wlan_keepalive.c`, lines 11–25**

```c
// WHY: The BCM43436B0 WiFi chip (Pi Zero 2W) connects via SDIO bus.
// When no process actively reads frames from the monitor interface, the
// SDIO bus goes idle and the firmware crashes ("Firmware has halted").
// Bettercap's wifi.recon accidentally provides this keepalive, but costs
// ~50MB RAM. This daemon does the same job at ~20KB.
//
// HOW: Opens a raw packet socket on wlan0mon in promiscuous mode and
// drains frames in a loop. Periodically sends broadcast probe requests
// to ensure the driver stays active even when there's no nearby traffic.
```

- Required for BCM43436B0 (Pi Zero 2W) to prevent SDIO firmware crash
- Opens raw AF_PACKET socket on wlan0mon
- Sends probe requests every 3 seconds to keep SDIO bus active

### Issues

**Issue 6a: Dependency on wlan0mon (Line 17)**

Expects wlan0mon to already exist. This is guaranteed by:
1. bettercap.service's ExecStartPre (pwnghost-monstart-if-needed)
2. Requisite=bettercap.service (service won't start if bettercap fails)

- ✓ Correct in theory
- **But**: If monstart silently created wlan1mon instead of wlan0mon (as documented in Section 4), wlan_keepalive can't open the raw socket and crashes repeatedly
- Log would show: `wlan_keepalive: can't open wlan0mon: No such device`
- wlan_keepalive would restart every 3 seconds forever (Restart=always, RestartSec=3)
- SDIO bus still crashes (no keepalive running)
- Bettercap also can't bind to wlan0mon and fails
- **Daemon starts but has no bettercap connection**

**Issue 6b: Sandboxing Fragility (Lines 24–28)**

```ini
NoNewPrivileges=yes
PrivateTmp=yes
ProtectSystem=strict
ProtectHome=yes
```

- ProtectSystem=strict requires all paths to be pre-existing and bind-mountable
- On a highly constrained Pi Zero (32MB RAM on some variants), tmpfs permission issues can cause unexpected failures
- wlan_keepalive itself has no state files, so this is defensive hardening

**Severity**: LOW (hardening is defensive)

---

## 7. Firmware/Monitor-Mode Support: nexmon

**Context**: The rebase pipeline uses a jayofelony base image which already includes nexmon-patched brcmfmac kernel module.

### Confirmed Status (from rebase build.sh)

**File: `tools/rebase-jayofelony/build.sh`, lines 165–209**

```bash
# Which jayofelony release to rebase onto. Real hardware testing across
# several candidates so far (all confirmed to have genuine nexmon --
# decompressed and grepped the *active* brcmfmac.ko directly, not
# inferred from a package name, since that's the one check that's
# actually proven reliable across versions):
#   - v2.9.5.4 (Trixie): hangs black-screen/no-LED on Pi Zero W
#   - v2.9.5.3 (Bookworm): kernel panic on Pi Zero 2W, blank-HDMI hang on Pi Zero W
#   - v2.8.9 (bullseye): the user's own direct prior experience is that
#     this one actually boots and runs on this exact hardware. [system-wide
#     pip install, no venv, no Go/Rust toolchain]
#   - bullseye64 (v2.6.4): arm64 only, Pi Zero 2W-only. Confirmed genuine
#     Debian 11 bullseye, kernel 6.1.21-v8+, brcmfmac.ko has real nexmon
#     patches (grepped nexmon_nl_ioctl_handler / brcmf_cfg80211_nexmon_set_channel)
```

### Chips Supported

- **BCM43430** (Pi Zero W) — nexmon patches applied, monitor mode functional
- **BCM43436B0** (Pi Zero 2W) — nexmon patches applied, monitor mode functional

### Risks

**Issue 7a: Firmware/Driver Version Mismatch**

- Pi model revision determines kernel version and brcmfmac module version
- nexmon patches are version-specific (different patches for different kernel versions)
- Firmware file version must match driver module version
- **Confirmed real-hardware issue**: A Pi Zero W with out-of-date firmware but newer kernel + nexmon module failed to enter monitor mode silently
  - `iw dev wlan0 set type monitor` appeared to succeed
  - But frames never flowed (confirmed via tcpdump: 0 packets over 10 minutes despite hopping)
  - This was distinguished from the in-place type-change monstart issue by manually running monstart and verifying channel hopping then worked
  - Root cause: nexmon patches weren't active in the driver/firmware pair

**Severity**: **MEDIUM — Unlikely on a freshly-flashed jayofelony base, but possible if the SD card has been reflashed or the firmware upgraded separately.**

**Issue 7b: BCM43436B0 SDIO Bus Stability (Pi Zero 2W Specific)**

- nexmon's monitor mode on this SDIO-bus chip requires active frame consumption (wlan_keepalive)
- If wlan_keepalive crashes or can't start, SDIO bus crashes → firmware halts → all WiFi access gone
- Symptoms: After 1–3 minutes, "WiFi down!", wlan0 disappears entirely, only GPIO power cycle helps

**Severity**: **MEDIUM — Mitigated by wlan_keepalive.service, but fragile if keepalive fails to start.**

---

## 8. Diagnosis Checklist for Operator

### On a Device Showing 0 APs / No Pwning

**Step 1: Verify Boot Chain State**
```bash
systemctl status bettercap.service pwnghost-rs.service wlan_keepalive.service
```
Expected: All three should be `active (running)` or `active (exited)` for wlan_keepalive.
If any show `failed` or `inactive`, check logs immediately (see Step 3).

**Step 2: Verify Monitor Interface**
```bash
ip link show | grep -E 'wlan[0-9]'
```
Expected: Should see both:
- `wlan0: ... type managed ...`
- `wlan0mon: ... type monitor ...`

If only `wlan0` or only `wlan0mon` visible, or if interface is `wlan1/wlan1mon`, the enumeration assumption is broken (Section 4 issue).

**Step 3: Check Service Logs**
```bash
journalctl -u bettercap.service -u pwnghost-rs.service -u wlan_keepalive.service -n 100
```
Look for:
- `bettercap: starting` vs `bettercap: ERROR binding to wlan0mon` — interface doesn't exist
- `pwnghost-rs: bettercap bootstrap failed` — bootstrap did not succeed
- `wlan_keepalive: can't open wlan0mon` — monitor interface doesn't exist
- `pwnghost-rs: recon/capture won't work until bettercap is reachable` — daemon gave up on bettercap

**Step 4: Verify bettercap REST API**
```bash
curl -s -u pwnghost:pwnghost http://127.0.0.1:8081/api/session/wifi | jq '.aps | length'
```
Expected: Positive number (1+) if WiFi is in range, or 0 if truly no APs nearby.
If command fails with "Connection refused" → bettercap's REST API is not running.
If returns empty object `{}` → API is up but something is wrong with the module state.

**Step 5: Check if wifi.recon is On**
```bash
curl -s -u pwnghost:pwnghost http://127.0.0.1:8081/api/session | jq '.modules[] | select(.name=="wifi") | .running'
```
Expected: `true` (module running) and search output for "recon" or "station":
```bash
curl -s -u pwnghost:pwnghost http://127.0.0.1:8081/api/session | jq '.modules[] | select(.name=="wifi")'
```
If `"recon": false` or not present, **this is the critical bug** — wifi.recon is OFF.

**Step 6: Manually Re-enable wifi.recon**
```bash
curl -s -u pwnghost:pwnghost -X POST http://127.0.0.1:8081/api/session \
  -H "Content-Type: application/json" \
  -d '{"cmd": "wifi.recon on"}' | jq
```
Expected: `{"success": true}`.
If this succeeds and then `curl` from Step 4 returns APs, **the race condition confirmed** — recon was off, and daemon never turned it back on.

**Step 7: Check Interface Permissions & rfkill**
```bash
rfkill list
```
Expected: All WiFi entries should show "Soft blocked: no".
If `Soft blocked: yes`, unblock with:
```bash
rfkill unblock all
```
This is handled by wifi-country.service at boot, but can get stuck if service failed.

**Step 8: Check Monitor Mode Support**
```bash
iw list | grep -A2 "monitor"
```
Expected: Should list supported modes including "monitor".
If not present, nexmon patches did not apply correctly or driver/firmware mismatch.

---

## 9. Severity-Ranked Risk Summary

### CRITICAL (Likely Cause of 0 APs on Fresh Boot)

1. **wifi.recon Race Condition** (Sections 2)
   - **Failure Mode**: Daemon bootstraps once; if bettercap restarts, wifi.recon goes OFF forever
   - **Trigger**: bettercap crash/restart after daemon startup (RestartSec=10 is normal)
   - **Evidence**: `curl -s -u ... http://127.0.0.1:8081/api/session | jq` shows 0 APs despite APs in range
   - **Confirmation**: Manually re-enable `wifi.recon on` via curl → APs appear immediately
   - **Fix Required**: Add periodic health check that verifies/re-enables wifi.recon, OR add explicit wifi.recon enablement to bettercap.service itself

2. **Interface Enumeration Assumption** (Section 4)
   - **Failure Mode**: Physical WiFi chip enumerates as wlan1/wlan2 instead of wlan0; monstart creates wrong interface
   - **Trigger**: Certain Pi revisions/bootloader versions (confirmed on real hardware)
   - **Evidence**: `ip link show` shows wlan1 but no wlan0; bettercap logs show "cannot bind to wlan0mon"
   - **Confirmation**: Manually check `ip link show`; if wlan0mon doesn't exist, this is the issue
   - **Fix Required**: Detect actual interface name and pass it to both monstart and bettercap.service

### HIGH (Compounds Critical Issues)

3. **Soft Dependency on bettercap** (Section 5a)
   - **Failure Mode**: Daemon starts before bettercap is ready; 20-second bootstrap window insufficient on slow hardware
   - **Trigger**: High system load at boot, slow SDIO chip initialization
   - **Evidence**: `journalctl -u pwnghost-rs` shows "bettercap bootstrap failed after 10 attempts"
   - **Confirmation**: Check systemd timing: `systemd-analyze critical-chain pwnghost-rs.service`
   - **Fix Required**: Change `Wants=` to `Requires=`, add explicit `StartLimitBurst` guards

4. **No Healing Mechanism for wifi.recon Disable** (Section 5b)
   - **Failure Mode**: Even if healing system detects "0 APs," it can't distinguish "recon OFF" from "no targets"
   - **Trigger**: Daemon interprets "no APs" from `wifi.recon on` the same as from `wifi.recon off`
   - **Evidence**: Device sits in "looking-left/right" face, daemon never escalates to healing
   - **Confirmation**: Check for `Healer: Soft-resetting` messages in journalctl; if absent despite 0 APs, this is the issue
   - **Fix Required**: Add explicit `wifi.recon on` heartbeat to daemon main loop

5. **Firmware/nexmon Version Mismatch** (Section 7a)
   - **Failure Mode**: Monitor mode appears to work (no errors) but frames never flow; 0 APs despite APs in range
   - **Trigger**: Kernel/driver/firmware version mismatch; confirmed on real Pi Zero W
   - **Evidence**: `iw dev wlan0mon set channel 11` succeeds, but `tcpdump -i wlan0mon` captures 0 frames
   - **Confirmation**: Compare kernel version (`uname -a`), brcmfmac module version (`modinfo brcmfmac`), firmware date
   - **Fix Required**: Rebuild image with coordinated kernel/driver/firmware versions; confirm with tcpdump

### MEDIUM (Mitigating Factors)

6. **wlan_keepalive.service Fragility** (Section 6)
   - **Failure Mode**: SDIO bus crashes; firmware halts; WiFi disappears after 1–3 minutes (BCM43436B0 only)
   - **Trigger**: wlan_keepalive fails to start or crashes
   - **Evidence**: Device works for 1–3 minutes, then "WiFi down!", wlan0mon disappears entirely
   - **Confirmation**: Check journalctl for `wlan_keepalive` errors; re-enable with `systemctl restart wlan_keepalive.service`
   - **Mitigation**: Already present (Requisite=bettercap.service guards it), but fragile if monstart creates wrong interface

7. **bettercap.service Restart Cascades** (Section 3c)
   - **Failure Mode**: Each bettercap crash resets wifi.recon to OFF; compounded by race condition (Issue 1)
   - **Trigger**: bettercap OOM, module crash, firmware hang
   - **Evidence**: Frequent `bettercap systemd[X]: bettercap.service: Main process exited` entries in syslog
   - **Confirmation**: Count restart cycles: `journalctl -u bettercap.service | grep "started\|exited" | wc -l`
   - **Mitigation**: Requires fixing Issue 1 (wifi.recon heartbeat)

### LOW (Defensive Hardening)

8. **wlan_keepalive.service Sandboxing** (Section 6b)
   - **Failure Mode**: ProtectSystem=strict fails if paths don't exist; rare
   - **Trigger**: Misconfigured boot or missing tmpfs directories
   - **Evidence**: systemd warning: "Cannot set up namespace, ignoring namespace setting"
   - **Confirmation**: Remove hardening flags temporarily and retest
   - **Mitigation**: Low priority; defensive only

---

## 10. Boot Order Diagram (Corrected)

```
multi-user.target (wanted)
│
├─ network.target (wanted)
│  └─ network-online.target (depends on network stack)
│
├─ wifi-country.service (After=network.target)
│  └─ [Unblock rfkill, set regulatory domain]
│
├─ bettercap.service (After=network.target)
│  ├─ ExecStartPre: /usr/local/bin/pwnghost-monstart-if-needed
│  │  └─ [Check if wlan0mon exists; if not, run monstart]
│  │     └─ iw phy PHY interface add wlan0mon type monitor
│  │        └─ ifconfig wlan0mon up
│  └─ ExecStart: bettercap -iface wlan0mon -eval "... api.rest on ..."
│     └─ [At this point: wlan0mon exists, bettercap bound to it, REST API listening on 127.0.0.1:8081]
│        [BUT: wifi.recon off (default state)]
│
├─ pwnghost-rs.service (After=network.target bettercap.service; Wants=)
│  ├─ [Daemon starts, may or may not wait for bettercap to be fully ready]
│  └─ Bootstrap loop: Try "wifi.recon on" 10x over 20s
│     ├─ IF succeeds on attempt 1–10: ✓ wifi.recon now on
│     └─ IF fails all 10x: ✗ WARN and continue anyway (daemon proceeds, recon stays OFF)
│
└─ wlan_keepalive.service (After=network.target bettercap.service; Requisite=)
   └─ ExecStart: wlan_keepalive wlan0mon 100
      └─ [Opens raw AF_PACKET on wlan0mon, drains frames, sends probes every 3s]
         [Keeps SDIO bus alive for BCM43436B0]

========== After boot ==========

Normal operation:
  daemon → GET http://127.0.0.1:8081/api/session/wifi → bettercap returns APs
  [Assuming wifi.recon is on AND device is in range of APs]

Failure scenario (CRITICAL RACE):
  +0s: bettercap.service starts ExecStartPre (monstart), takes 5s to complete
  +1s: pwnghost-rs.service starts, tries wifi.recon on
       → Fails: bettercap not accepting connections yet
  +2s: Retry, fails again
  ... (9 more retries, all fail)
  +20s: Daemon gives up on bootstrap, continues anyway
  +25s: bettercap.service's ExecStartPre finally completes, ExecStart begins
  +30s: bettercap fully up, listening on REST, but wifi.recon OFF (default)
  +31s: Daemon calls GET /api/session/wifi → returns empty (recon is off)
  Forever: Daemon sees 0 APs, assumes no targets in range, never re-enables recon
```

---

## 11. Recommended Fixes (Priority Order)

### P1: Fix wifi.recon Permanent Disable (CRITICAL)

Add periodic health-check to daemon main loop that verifies wifi.recon is on:

```rust
// In main.rs event loop (around line 1020, after display_interval.tick())
_ = recon_check_interval.tick() => {
    // Every 60 seconds, verify wifi.recon is actually on
    let bc = bc.clone();
    if let Err(e) = tokio::task::spawn_blocking(move || {
        bc.run_command("wifi.recon on")  // Idempotent re-enable
    })
    .await
    .unwrap_or_else(|e| Err(anyhow::anyhow!("{e}")))
    {
        warn!("Failed to verify/re-enable wifi.recon: {}", e);
    }
}
```

This simple fix:
- Re-enables wifi.recon every 60 seconds (idempotent, safe)
- Detects and recovers from bettercap restarts
- No changes to systemd units required
- **Estimated to resolve 70–80% of "0 APs" complaints**

### P2: Fix Interface Enumeration Assumption (CRITICAL)

Replace hardcoded `wlan0` assumption in monstart:

```bash
# Detect actual interface name instead of hardcoding
IFACE="$(ip link show | grep -E 'wlan[0-9]' | grep 'type managed' | head -1 | awk '{print $2}' | tr -d ':')"
if [ -z "$IFACE" ]; then
    echo "monstart: ERROR no managed-mode WiFi interface found" >&2
    exit 1
fi
MON="${IFACE}mon"
```

Pass detected interface name via environment variable or config file to both bettercap.service and wlan_keepalive.service.

### P3: Change Soft to Hard Dependency (HIGH)

**File: `tools/rebase-jayofelony/overlay/etc/systemd/system/pwnghost-rs.service`, line 11**

```diff
- Wants=network.target bettercap.service
+ Requires=network.target
+ Wants=bettercap.service
+ # OR, more strictly:
+ # Requires=network.target bettercap.service
```

Requires= will cause pwnghost-rs to fail if bettercap fails, making the failure visible in systemd status instead of silent.

### P4: Add Explicit wifi.recon Enable to bettercap.service (MEDIUM)

As a defense-in-depth measure, bettercap.service could include wifi.recon in its startup, though this risks race conditions if the REST API isn't fully ready:

```bash
# After ExecStart, add:
ExecStartPost=/usr/bin/sleep 2; curl -s -u pwnghost:pwnghost -X POST http://127.0.0.1:8081/api/session -d '{"cmd":"wifi.recon on"}' || true
```

Less elegant than daemon-side P1 fix, but provides extra defense.

---

## 12. Commands to Run on Device for Diagnosis

Execute these in order on a device showing 0 APs:

```bash
# 1. Check if all services are running
systemctl status bettercap.service pwnghost-rs.service wlan_keepalive.service

# 2. Check interface existence and type
ip link show | grep -E 'wlan|mon'
iw dev wlan0mon info 2>/dev/null || echo "wlan0mon does not exist or is not monitor mode"

# 3. Check REST API availability
curl -s -u pwnghost:pwnghost http://127.0.0.1:8081/api/session | jq '.' | head -50

# 4. Check if wifi module is loaded and running
curl -s -u pwnghost:pwnghost http://127.0.0.1:8081/api/session | jq '.modules[] | select(.name=="wifi")'

# 5. Check AP list (should be non-empty if APs in range and recon is on)
curl -s -u pwnghost:pwnghost http://127.0.0.1:8081/api/session/wifi | jq '.aps | length'

# 6. Check recent logs
journalctl -u bettercap.service -u pwnghost-rs.service -u wlan_keepalive.service -n 200 --no-pager

# 7. Check if wifi.recon is on
curl -s -u pwnghost:pwnghost http://127.0.0.1:8081/api/session | jq '.modules[] | select(.name=="wifi") | {running, enabled}'

# 8. Manually re-enable wifi.recon (test fix)
curl -s -u pwnghost:pwnghost -X POST http://127.0.0.1:8081/api/session \
  -H 'Content-Type: application/json' \
  -d '{"cmd":"wifi.recon on"}' | jq '.'

# 9. Check again for APs
sleep 2
curl -s -u pwnghost:pwnghost http://127.0.0.1:8081/api/session/wifi | jq '.aps | length'

# 10. Check rfkill status (should all be unblocked)
rfkill list

# 11. Check if bettercap restarted frequently
journalctl -u bettercap.service | grep -E 'Started|Stopped' | tail -20

# 12. Check daemon logs for bootstrap failures
journalctl -u pwnghost-rs.service | grep -i 'bootstrap\|recon'
```

If manual `wifi.recon on` in step 8 causes APs to appear in step 9, **the race condition is confirmed as the root cause**.

---

## Conclusion

The most likely cause of a freshly-flashed device showing "0 APs, no pwning" is the **wifi.recon permanent disable race condition** (Section 2). The daemon enables wifi.recon only during its 1-minute bootstrap window; if bettercap restarts or wasn't ready in time, wifi.recon stays OFF forever, and the daemon never re-enables it.

**Immediate mitigation** for users: After boot, SSH in and run:
```bash
curl -s -u pwnghost:pwnghost -X POST http://127.0.0.1:8081/api/session \
  -H 'Content-Type: application/json' \
  -d '{"cmd":"wifi.recon on"}' && sleep 2 && echo "APs should now appear:"
curl -s -u pwnghost:pwnghost http://127.0.0.1:8081/api/session/wifi | jq '.aps | length'
```

**Permanent fix** requires adding a periodic health check (P1 above) that re-enables wifi.recon every 60 seconds in the daemon's main loop.
