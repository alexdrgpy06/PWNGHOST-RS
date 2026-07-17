#!/bin/bash -e

if [ "$RELEASE" != "bookworm" ]; then
	echo "WARNING: PWNGHOST-RS targets Raspberry Pi OS bookworm; RELEASE=${RELEASE}."
	echo "         nexmon firmware and package pins in stage2/stage3 are tested against bookworm only."
fi

if [ ! -d "${ROOTFS_DIR}" ]; then
	bootstrap ${RELEASE} "${ROOTFS_DIR}" http://raspbian.raspberrypi.com/raspbian/
fi
