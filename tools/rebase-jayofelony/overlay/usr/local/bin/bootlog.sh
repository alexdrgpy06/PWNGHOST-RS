#!/bin/bash
# bootlog.sh - Write a boot diagnostics dump to the boot partition, readable
# from any PC by pulling the SD card -- no SSH/network/serial console
# needed. Same script used in the from-scratch pi-gen build
# (pi-gen/stage5/01-runtime-overlay), copied verbatim into this rebased
# image's overlay with one difference: the USB gadget check below refers
# to this base image's own rpi-usb-gadget mechanism, not our
# usb-gadget-setup.service (which isn't installed here -- see README.md).

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
echo "--- USB gadget (rpi-usb-gadget, this base image's own mechanism -- not our own usb-gadget-setup.service) ---"
systemctl status rpi-usb-gadget 2>/dev/null || systemctl status usb-gadget 2>/dev/null || echo "no rpi-usb-gadget/usb-gadget unit found"
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
