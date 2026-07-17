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
rsync -a files/ "${ROOTFS_DIR}/"

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
