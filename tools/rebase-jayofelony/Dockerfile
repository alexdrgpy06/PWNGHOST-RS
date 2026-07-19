# Environment for the jayofelony/pwnagotchi image-rebase pipeline
# (tools/rebase-jayofelony/build.sh). This is deliberately separate from
# Dockerfile.builder: that image cross-compiles our Rust binaries; this one
# performs surgery on an already-built foreign armhf image (loop-mount,
# qemu-arm-static chroot, package removal, shrink). Different job, kept
# out of Dockerfile.builder to avoid coupling the two pipelines together.
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    binfmt-support \
    ca-certificates \
    curl \
    dosfstools \
    e2fsprogs \
    fdisk \
    kpartx \
    parted \
    python3 \
    qemu-user-static \
    rsync \
    util-linux \
    xz-utils \
    zerofree \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /work
