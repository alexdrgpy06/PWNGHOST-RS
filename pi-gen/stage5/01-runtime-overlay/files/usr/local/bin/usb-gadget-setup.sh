#!/bin/bash
# usb-gadget-setup.sh - Build the USB gadget (RNDIS network + ACM serial
# console) from scratch via configfs, replacing reliance on the legacy
# g_ether kernel module.
#
# Why not just g_ether (modules-load=dwc2,g_ether)?  g_ether's default
# composition has no Microsoft OS descriptor, so Windows has no inbox
# driver for its network function -- only the accidental CDC ACM half
# (which Windows *does* have a generic driver for, usbser.sys) ever
# enumerates usably. That was confirmed the hard way during hardware
# testing: the gadget showed up as an unusable "unknown device" or an
# errored COM port, but never as a network adapter, across several
# different cables/ports/rebuilds.
#
# Building the gadget by hand lets the RNDIS function carry the Microsoft
# OS descriptor (compatible_id "RNDIS" / sub_compatible_id "5162001") that
# tells Windows to bind its native "Remote NDIS Compatible Device" driver
# automatically, with no manual driver install -- matching how
# pwnagotchi's original USB gadget setup presents itself on Windows.
#
# Runs as a oneshot systemd service (usb-gadget-setup.service) before
# NetworkManager and usb-net.service, both of which expect a "usb0"
# interface to already exist.

set -e

modprobe libcomposite 2>/dev/null || true
modprobe usb_f_rndis 2>/dev/null || true
modprobe usb_f_acm 2>/dev/null || true

mountpoint -q /sys/kernel/config || mount -t configfs none /sys/kernel/config

G=/sys/kernel/config/usb_gadget/pwnghost
if [ -d "$G" ]; then
    echo "usb-gadget-setup: $G already exists, nothing to do"
    exit 0
fi

mkdir -p "$G"
cd "$G"

echo 0x1d6b > idVendor   # Linux Foundation
echo 0x0104 > idProduct  # Multifunction Composite Gadget
echo 0x0100 > bcdDevice
echo 0x0200 > bcdUSB
# Interface Association Descriptor device class -- required for a host to
# correctly group the RNDIS + ACM functions as one composite device.
echo 0xEF > bDeviceClass
echo 0x02 > bDeviceSubClass
echo 0x01 > bDeviceProtocol

mkdir -p strings/0x409
echo "pwnghost0001" > strings/0x409/serialnumber
echo "PWNGHOST-RS" > strings/0x409/manufacturer
echo "pwnghost-rs USB gadget" > strings/0x409/product

mkdir -p configs/c.1/strings/0x409
echo "RNDIS network + serial console" > configs/c.1/strings/0x409/configuration
echo 250 > configs/c.1/MaxPower

# RNDIS function -- fixed, locally-administered MAC addresses (bit 0x02 set
# on the first octet) so the interface's identity is stable across boots.
mkdir -p functions/rndis.usb0
echo "02:1a:2b:3c:4d:5e" > functions/rndis.usb0/host_addr
echo "02:1a:2b:3c:4d:5f" > functions/rndis.usb0/dev_addr
mkdir -p functions/rndis.usb0/os_desc/interface.rndis
echo "RNDIS" > functions/rndis.usb0/os_desc/interface.rndis/compatible_id
echo "5162001" > functions/rndis.usb0/os_desc/interface.rndis/sub_compatible_id
ln -s functions/rndis.usb0 configs/c.1/

# ACM function -- creates ttyGS0, used by serial-getty@ttyGS0.service
# (see stage5/01-runtime-overlay/00-run.sh) as a fallback login console.
mkdir -p functions/acm.GS0
ln -s functions/acm.GS0 configs/c.1/

# Microsoft OS descriptors: must be enabled at the gadget level (not just
# the function level) for Windows to actually query and honor the
# RNDIS compatible_id set above.
mkdir -p os_desc
echo 1 > os_desc/use
echo 0xcd > os_desc/b_vendor_code
echo "MSFT100" > os_desc/qw_sign
ln -s configs/c.1 os_desc

UDC=$(ls /sys/class/udc | head -n1)
if [ -z "$UDC" ]; then
    echo "usb-gadget-setup: no UDC found under /sys/class/udc (is dtoverlay=dwc2 set?)" >&2
    exit 1
fi
echo "$UDC" > UDC

echo "usb-gadget-setup: gadget bound to UDC $UDC (RNDIS usb0 + ACM ttyGS0)"
