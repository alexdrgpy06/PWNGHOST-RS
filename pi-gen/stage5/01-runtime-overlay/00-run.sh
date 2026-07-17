#!/bin/bash -e
# 01-runtime-overlay/00-run.sh - Install the BT-PAN / USB-tether / zram /
# watchdog runtime overlay.
#
# Salvaged from oxigotchi/services/ (18 systemd units) and
# rpiproj/stage3/05-install-oxigotchi/files/ (the matching rootfs overlay
# tree), with identifiers renamed to their pwnghost-rs equivalents
# (oxigotchi.service -> pwnghost-rs.service, /etc/oxigotchi -> /etc/pwnghost,
# /var/tmp/pwnagotchi -> /var/tmp/pwnghost, etc.) and the USB gadget path
# adapted for the classic dtoverlay=dwc2 + modules-load=dwc2,g_ether tether
# (see stage1/00-boot-files) instead of building a configfs gadget from
# scratch (g_ether already does that via kernel module parameters).

# Runtime directories the daemon / overlay scripts expect.
install -d -m 755 \
    "${ROOTFS_DIR}/var/lib/pwnghost/log" \
    "${ROOTFS_DIR}/var/lib/pwnghost/data"

# DIAGNOSTIC (temporary): two real CI runs have failed here with
# "qemu-arm[-static]: Could not open '/lib/ld-linux-armhf.so.3'" -- the
# exact same failure survived switching qemu's binfmt registration from
# dynamic (qemu-arm) to static (qemu-arm-static), which rules out a qemu/
# binfmt-registration cause (a static interpreter needs no shared libs of
# its own) and points at the *target* rootfs's own /lib actually missing
# the file at this exact point, despite on_chroot (which itself needs this
# same interpreter just to start bash) working fine one substage earlier
# in 00-install-pwnghost. Capture hard evidence instead of guessing again.
echo "=== pwnghost-rs diagnostic: state of ${ROOTFS_DIR} before overlay rsync ==="
ls -la "${ROOTFS_DIR}/lib/ld-linux-armhf.so.3" 2>&1 || echo "MISSING before rsync"
readlink -f "${ROOTFS_DIR}/lib/ld-linux-armhf.so.3" 2>&1 || true
ls -la "${ROOTFS_DIR}/lib/arm-linux-gnueabihf/" 2>&1 | head -5 || true
mount | grep "$(realpath "${ROOTFS_DIR}")" 2>&1 || echo "no active mounts under ROOTFS_DIR"
echo "=== end diagnostic (pre-rsync) ==="

# Copy the whole overlay (etc/, usr/, lib/) into the rootfs.
rsync -a files/ "${ROOTFS_DIR}/"

echo "=== pwnghost-rs diagnostic: state of ${ROOTFS_DIR} after overlay rsync ==="
ls -la "${ROOTFS_DIR}/lib/ld-linux-armhf.so.3" 2>&1 || echo "MISSING after rsync"
echo "=== end diagnostic (post-rsync) ==="
on_chroot << 'CANARY'
echo "chroot-canary: pwd=$(pwd) whoami=$(whoami) uname=$(uname -m)"
ls -la /lib/ld-linux-armhf.so.3 2>&1 || echo "chroot-canary: MISSING from inside chroot"
echo "chroot-canary: ok"
CANARY

chmod 755 "${ROOTFS_DIR}"/usr/local/bin/*.sh 2>/dev/null || true
chmod 755 "${ROOTFS_DIR}/usr/local/bin/bt-pan-connect"
chmod 755 "${ROOTFS_DIR}/usr/local/bin/bt-pan-disconnect"
chmod 755 "${ROOTFS_DIR}/lib/systemd/system-shutdown/safe-shutdown.sh"

# logrotate for the on-zram logs.
install -d -m 755 "${ROOTFS_DIR}/etc/logrotate.d"
cat > "${ROOTFS_DIR}/etc/logrotate.d/pwnghost-rs" << 'EOF'
/var/log/pwnghost/*.log {
    daily
    rotate 7
    compress
    delaycompress
    missingok
    notifempty
    create 644 root root
}
EOF

on_chroot << EOF
set +e
for unit in usb-net.service wifi-country.service zram-log.service zram-data.service \
            rsync-zram.timer buffer-cleaner.timer bt-agent.service nm-watchdog.service \
            safe-shutdown.service; do
    systemctl enable "\$unit" && echo "enabled \$unit" || echo "pwnghost-rs: could not enable \$unit (continuing)"
done
systemctl enable NetworkManager.service 2>/dev/null || true
systemctl enable ssh.service 2>/dev/null || systemctl enable sshd.service 2>/dev/null || true

# bt-pan@.service is a template unit started per-device MAC at runtime
# (systemctl start bt-pan@AA:BB:CC:DD:EE:FF.service), not enabled here.

usermod -aG bluetooth ${FIRST_USER_NAME} 2>/dev/null || true
chown -R ${FIRST_USER_NAME}:${FIRST_USER_NAME} /etc/pwnghost /var/log/pwnghost /var/tmp/pwnghost /var/lib/pwnghost
EOF

echo "runtime overlay installation complete"
