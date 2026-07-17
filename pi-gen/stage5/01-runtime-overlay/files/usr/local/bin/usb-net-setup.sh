#!/bin/bash
# usb-net-setup.sh - Static IP + DHCP for the classic pwnagotchi USB-ethernet
# tether (usb0), once dtoverlay=dwc2 + cmdline.txt modules-load=dwc2,g_ether
# (see stage1/00-boot-files/files/{config.txt,cmdline.txt}) have already
# created the usb0 gadget interface via the g_ether kernel module.
#
# Adapted from rpiproj/stage3/05-install-oxigotchi/files/usr/local/bin/
# usb-gadget.sh: that script also *built* an RNDIS gadget from scratch via
# configfs. We don't need that half here - g_ether already built the gadget
# via module load, so this script starts from "wait for usb0 to exist" and
# keeps only the proven IP/dnsmasq half of that logic.

set -e

# Bring usb0 up with a static IP once the interface exists.
for _ in $(seq 1 15); do
    [ -e /sys/class/net/usb0 ] && break
    sleep 1
done

if [ -e /sys/class/net/usb0 ]; then
    ip link set usb0 up || true
    ip addr flush dev usb0 2>/dev/null || true
    # 10.0.0.2 is our own scheme (DHCP served below). 192.168.137.2 is a
    # second static address on the same subnet Windows Internet Connection
    # Sharing defaults to (host 192.168.137.1) - if Windows auto-activates
    # ICS on this adapter and takes over addressing, the Pi is still
    # reachable at a fixed IP in that subnet with no manual configuration.
    ip addr add 10.0.0.2/24 dev usb0 || true
    ip addr add 192.168.137.2/24 dev usb0 || true

    # Best-effort default route via the ICS gateway, at a deliberately high
    # (low-priority) metric so it only kicks in when nothing better already
    # provides a default route (BT tether, a wlan0 client connection, etc.)
    # - `add`, never `replace`, so this can't hijack routing away from a
    # working connection.
    ip route add default via 192.168.137.1 dev usb0 metric 400 2>/dev/null || true

    # Hand the connected host an address on our subnet so `ssh pwn@10.0.0.2`
    # works with no manual IP setup, when ICS hasn't already claimed the link.
    if command -v dnsmasq >/dev/null 2>&1; then
        pkill -f "dnsmasq.*usb0" 2>/dev/null || true
        dnsmasq --interface=usb0 --bind-interfaces --except-interface=lo \
            --dhcp-range=10.0.0.10,10.0.0.30,255.255.255.0,1h \
            --dhcp-option=3 --dhcp-option=6 \
            --no-resolv --no-hosts --leasefile-ro \
            --pid-file=/run/usb0-dnsmasq.pid 2>/dev/null || true
    fi
    echo "usb-net-setup: usb0 up at 10.0.0.2 + 192.168.137.2 (dhcp 10.0.0.10-30)"
else
    echo "usb-net-setup: usb0 interface never appeared (is dtoverlay=dwc2 / modules-load=dwc2,g_ether set?)" >&2
fi
exit 0
