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

# Copy the whole overlay (etc/, usr/, lib/) into the rootfs.
#
# --keep-dirlinks (-K) is required here: this bookworm rootfs is usrmerged
# (${ROOTFS_DIR}/lib is a symlink to usr/lib), but the overlay's own source
# tree has a real directory at files/lib/ (for
# lib/systemd/system-shutdown/safe-shutdown.sh). Without -K, rsync's
# default behavior when the source has a real directory where the
# destination has a symlink is to DELETE the destination symlink and
# create a real directory in its place -- which replaced the entire
# /lib -> usr/lib symlink with a new, nearly-empty directory containing
# only the one path this overlay ships, destroying access to everything
# else that used to live there (confirmed the hard way: this took out
# /lib/ld-linux-armhf.so.3, the armhf dynamic linker itself, breaking
# every subsequent qemu-emulated exec in the chroot). -K tells rsync to
# follow the existing symlink instead of replacing it.
rsync -a -K files/ "${ROOTFS_DIR}/"

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
