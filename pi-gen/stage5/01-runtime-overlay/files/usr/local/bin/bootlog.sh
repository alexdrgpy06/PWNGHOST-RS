#!/bin/bash
# bootlog.sh - Write a boot diagnostics dump to the boot partition, readable
# from any PC by pulling the SD card -- no SSH/network/serial console
# needed. Ported from oxigotchi's tools/bootlog.sh (confirmed no equivalent
# existed here, 2026-07-18 audit), adapted for pwnghost-rs's own service
# names and USB gadget setup.
#
# This is exactly the diagnostic this project needed the hard way during
# USB gadget hardware testing this session: when the network/serial link
# itself is the thing that's broken, this is the one channel that still
# works, since it only requires physically removing and reading the card.

sleep 3
if mountpoint -q /boot/firmware; then
    LOG=/boot/firmware/bootlog.txt
else
    LOG=/var/log/bootlog.txt
fi
exec >> "$LOG" 2>&1

echo "=== Boot $(date) ==="
echo "Uptime: $(uptime)"
echo "--- Failed services ---"
systemctl list-units --failed
echo "--- pwnghost-rs ---"
systemctl status pwnghost-rs
echo "--- pwnghost-rs journal (last 40 lines) ---"
journalctl -u pwnghost-rs --no-pager -n 40
echo "--- SSH ---"
systemctl status ssh 2>/dev/null || systemctl status sshd 2>/dev/null
echo "--- USB gadget ---"
systemctl status usb-gadget-setup.service
systemctl status usb-net.service
echo "--- Network ---"
ip addr
nmcli -f DEVICE,TYPE,STATE,CONNECTION device 2>/dev/null
echo "--- Listening ports ---"
ss -tlnp
echo "--- bettercap (capture backend) ---"
pgrep -a bettercap || echo "bettercap not running"
echo "--- Disk ---"
df -h
echo "=== End ==="

# Self-heal SSH if it's not actually listening -- cheap, safe: regenerate
# host keys (covers the "keys went missing/corrupted" case) and restart.
# Unlike oxigotchi's equivalent, there's no emergency-ssh fallback daemon
# to fall back to here (that's a separate, deliberate security tradeoff,
# not yet made for this project).
if ! ss -tln | grep -q ":22 "; then
    ssh-keygen -A
    systemctl restart ssh 2>/dev/null || systemctl restart sshd 2>/dev/null
    echo "SSH healed at $(date)"
fi
