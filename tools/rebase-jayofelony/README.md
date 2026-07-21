# Rebase pipeline: jayofelony/pwnagotchi v2.9.5.3 + pwnghost-rs

An alternate image-build path that starts from a real, currently shipping
pwnagotchi release image instead of building the OS from scratch via
`pi-gen`, and strips out the Python pwnagotchi/bettercap/pwngrid stack in
favor of our own `pwnghost-rs` binary. This does **not** replace the
existing `pi-gen`-based build (see the repo root `pi-gen/` directory and
`Makefile`) -- both pipelines exist side by side so their resulting images
can be compared. See `SPEC.md`'s "Fidelity vs Reference Implementations"
section for the broader context that led here.

## Why this base, and why not others

**v2.9.5.4 (the original base for this pipeline) was tried first and
rejected after real hardware testing**: it built and passed every static
check (see below), but on actual Pi Zero W and Pi Zero 2W hardware it
hung at a black screen with no LED activity at all, several minutes in --
confirmed with the correctly board-matched image on each board, so it
wasn't an architecture mismatch. v2.9.5.4 is the *only* jayofelony release
built on Debian 13 "Trixie" (every other release checked, including this
one, is Bookworm or older) and the only one packaging nexmon as
`brcmfmac-nexmon-dkms`/`firmware-nexmon` apt packages rather than a
pre-baked kernel module -- both changed together in that one release, so
which one actually broke boot on these boards isn't isolated, just
correlated with the Trixie switch.

**A critical correction made mid-session**: earlier passes rejected two
older candidates --  the archived `jayofelony/pwnagotchi-bullseye` v2.8.4,
and jayofelony/pwnagotchi's own v2.8.9 -- for supposedly lacking real
nexmon support. That conclusion was based on an incomplete check (firmware
blob + generic `pwnlib` shell functions, not the actual kernel module
content). Re-checked properly by decompressing the *active* `brcmfmac.ko`
and searching it directly: **both v2.8.4 and v2.8.9 do have real nexmon**
(`nexmon_nl_ioctl_handler`, `brcmf_cfg80211_nexmon_set_channel`, build
path `/usr/local/src/nexmon/patches/driver/brcmfmac_*.y-nexmon/`) --
nexmon patching appears to be consistently baked into `brcmfmac.ko` across
every jayofelony release checked, from bullseye through this one. The
`brcmfmac-nexmon-dkms` package in v2.9.5.4 changed *how* it's packaged
(DKMS rebuild vs. a static pre-patched `.ko`), not whether nexmon is
present at all -- don't infer "no nexmon" from a missing package name
alone in any future version check; decompress and grep the actual
`brcmfmac.ko`.

**This base (`jayofelony/pwnagotchi` v2.9.5.3, Bookworm, 32-bit)** was
picked as the safer middle ground: Bookworm (not the Trixie base that
hung on real hardware), and directly confirmed to have real nexmon via
the kernel-module check above (its `.info` manifest alone doesn't show
`brcmfmac-nexmon-dkms` since this release predates that packaging, but the
patched module is baked in directly). USB-gadget/RNDIS *is* pre-configured
here, the same way it has been since evilsocket's original pwnagotchi:
`cmdline.txt` has `modules-load=dwc2,g_ether` and `config.txt` has
`dtoverlay=dwc2` -- confirmed by mounting the actual boot partition
directly (an earlier pass here wrongly checked `boot/cmdline.txt` inside
the *root* partition's mount, which is just the empty mountpoint stub, not
the real file, and concluded it was missing). This is plain `g_ether`,
not our own from-scratch build's hand-built configfs RNDIS gadget --
Windows still needs the manual driver install described in jayofelony's
wiki (`rpi-usb-gadget-driver-setup.exe`), same as every release checked.

v2.8.9 (bullseye) is also a viable candidate -- it has the user's own
direct confirmation of booting on this exact hardware, and now-confirmed
real nexmon -- but its pwnagotchi install is a system-wide pip install
(`/usr/local/lib/python3.9/dist-packages/pwnagotchi-*.dist-info`, launched
via `/usr/bin/pwnagotchi-launcher`), not the isolated venv this script's
strip step is written for (`/home/pi/.pwn`). Supporting it would need
version-specific strip logic; not yet implemented.

## What gets stripped, and why it's a lot

bettercap and pwngrid are **not** apt packages here -- they're Go binaries
built from source at image-build time (`stage3/03-bettercap-pwngrid`,
confirmed by reading that script directly), and pwnagotchi itself is a
pip-installed Python venv (`stage3/05-install-pwnagotchi`). None of this
shows up in a `dpkg -l` dump, so the `.info` manifest alone
undersells how much gets left behind. Directly measured on a mounted copy
of v2.9.5.4 while writing `build.sh` (v2.9.5.3's layout is identical
except it has no Rust toolchain to strip):

| Path | Size | What it is |
|---|---|---|
| `/root/.rustup` | 1.5 GB | Rust toolchain (v2.9.5.4 only -- v2.9.5.3 doesn't have this) |
| `/home/pi/bettercap` | 146 MB | bettercap Go source tree, left after `make install` |
| `/home/pi/.pwn` | 226 MB | pwnagotchi's Python venv |
| `/usr/local/go` | 251 MB | Go toolchain |
| `/var/lib/apt/lists` + `/var/cache/apt` | 169 MB | apt cache |
| `/home/pi/pwngrid` | 29 MB | pwngrid Go source tree |

That's up to **~2.3 GB removable** (less on v2.9.5.3 without the Rust
toolchain) -- independent, direct confirmation of what the earlier
oxigotchi audit found by reading jayofelony's install scripts alone
(leftover Go/Rust toolchains, uncleaned source trees). `build.sh` removes
all of it, `/usr/bin/pwnagotchi` and the
`pwnagotchi.service`/`bettercap.service`/`pwngrid-peer.service` units,
then `apt-get autoremove --purge`s whatever was only needed to build
those. Verified end to end on both boards: `e2fsck` clean pass, our
binary/config/services present and enabled, bettercap/pwngrid/pwnagotchi
fully gone.

**Explicitly not touched**: the patched `brcmfmac.ko`, firmware blobs,
`cmdline.txt`/`config.txt` (which already carry the `dwc2`/`g_ether` USB
gadget setup, confirmed above), NetworkManager, kernel/dtb files. The
entire point of this pipeline is to keep that proven hardware-enablement
layer exactly as shipped, not re-derive it -- none of PWNGHOST-RS's own
`usb-gadget-setup.service`/`usb0.nmconnection`/`cmdline.txt` changes from
the from-scratch build are applied here; this base's own plain-`g_ether`
setup is used as-is instead.

## Building

Requires Docker (privileged, for loop-mount + chroot) and a pre-built set
of cross-compiled artifacts -- from `Dockerfile.crosscompile` (this
directory), **not** the repo-root `Dockerfile.builder`. `Dockerfile.builder`
links against bookworm's glibc (~2.36); every base version this pipeline
supports (v2.8.9 bullseye, glibc 2.31; v2.9.5.3 bookworm, glibc ~2.36 --
same as builder, but keeping one build path avoids two artifact sets) needs
a binary linked against the *oldest* glibc among them, since glibc symbol
versioning is forward-compatible only. Confirmed the hard way on real
hardware: a `Dockerfile.builder`-linked binary flashed onto a v2.8.9 image
fails outright at startup with `version 'GLIBC_2.32' not found`, and
`pwnghost-rs.service` crash-loops (`Start request repeated too quickly`)
without ever drawing a single frame to the display.

**Resource limits -- read before running.** A real local run of this
pipeline froze the host machine hard enough to require a reboot, *despite*
Docker Desktop's WSL2 VM already being capped in `.wslconfig`
(`memory=10GB`, `swap=4GB`). The VM cap alone doesn't stop a single
container from filling it and thrashing the VM's swap file, which
manifests as the whole host going unresponsive rather than a clean
OOM-kill inside the container. `Dockerfile.crosscompile` now caps cargo's
own parallelism (`CARGO_BUILD_JOBS=2`) to reduce peak RSS, and every
command below adds explicit `--memory`/`--cpus` so a single container is
hard-capped well under the VM's ~9.7GB usable budget, with
`--memory-swap` equal to `--memory` so a container that does hit the
ceiling gets OOM-killed with a clear error instead of thrashing.
**Run the two `docker run` steps one at a time, never concurrently** --
each is sized to fit inside the VM alone, not two at once.

```bash
# From the repo root, produce artifacts/{arm-unknown-linux-gnueabihf,armv7-unknown-linux-gnueabihf}/{pwnghost-rs,wlan_keepalive}
docker build --memory=6g --memory-swap=6g --cpus=4 \
  -t pwnghost-crosscompile-bullseye -f tools/rebase-jayofelony/Dockerfile.crosscompile .
docker create --name extract pwnghost-crosscompile-bullseye
docker cp extract:/workspace/artifacts ./tools/rebase-jayofelony/artifacts
docker rm extract

cd tools/rebase-jayofelony
docker build -t pwnghost-rebase-jayofelony .

# One run per board -- both share the same base image download/cache.
# Run sequentially, not in parallel (see resource-limits note above).
docker run --rm --privileged \
  --memory=6g --memory-swap=6g --cpus=4 \
  -e BOARD=pi-zero-w \
  -v "$(pwd):/work" \
  pwnghost-rebase-jayofelony bash build.sh

docker run --rm --privileged \
  --memory=6g --memory-swap=6g --cpus=4 \
  -e BOARD=pi-zero-2w \
  -v "$(pwd):/work" \
  pwnghost-rebase-jayofelony bash build.sh
```

Output: `pwnghost-rs-rebased-pi-zero-w.img.xz` /
`pwnghost-rs-rebased-pi-zero-2w.img.xz`.

## Reliability (no SD-card-corrupting crashes)

- `build.sh` runs every chroot step under `set -euo pipefail` and aborts
  the whole pipeline on any unexpected failure rather than continuing
  into a half-modified rootfs.
- `e2fsck -f -y` runs on the root partition after all modifications,
  before shrinking/compressing -- a real filesystem-consistency check on
  the *build host*, before the image ever touches a real SD card.
- The same reliability units the from-scratch build already has are
  carried over verbatim in `overlay/`: zram-backed logging
  (`zram-log.service`/`zram-data.service`/`rsync-zram.timer`), tmpfs for
  transient data, `buffer-cleaner.timer`, and `bootlog.service` (writes
  full boot diagnostics to `/boot/firmware/bootlog.txt` on every boot,
  readable from any PC by pulling the SD card -- no network/serial
  needed). `pwnghost-rs.service` keeps its existing
  `ProtectSystem=strict`+`ReadWritePaths=` hardening so a crash in our
  own daemon can't write outside its own data directories.
- **BT-PAN tethering and `wifi-country.service` are now carried over**
  (this note previously said they weren't -- that's stale, both landed in
  `build.sh`/`overlay/` since). `wifi-country.service` isn't optional: a
  real-hardware boot of this rebased image without it showed `wlan0mon`
  permanently stuck in `Operation not possible due to RF-kill` (this base
  image never runs a wifi-country first-boot step the way stock Raspberry
  Pi OS does via `raspi-config`) -- confirmed real bug, not a theoretical
  gap. `bt-agent.service` (NoInputNoOutput pairing agent) is ported from
  the from-scratch pi-gen build's overlay, which already had this
  working, confirmed directly by mounting the base image (it ships the
  bt-agent/bluetoothctl binaries this depends on). `bt-pan@.service`
  itself is a template unit, not auto-enabled -- `bt_tether.lua` starts
  it per-device once a phone's MAC is configured, matching pi-gen's
  design.

## Display

The shipped `/etc/pwnghost/config.toml` (see `overlay/etc/pwnghost/`)
defaults to `ui.display.enabled = true` and `display_type =
"waveshare_v4"`, matching the from-scratch build's default exactly. The
panel-size-resolution logic (`crates/ui/display/src/driver.rs`'s
`PanelKind::resolve`) already falls back to the 2.13" V4 panel for this
exact config with no additional width/height keys needed -- confirmed via
`cargo test -p ui-display test_panel_kind_resolve_matches_known_sizes`
during planning, not a new code change. The binary built into this image
must be built with the `hardware` and `linux-gpio` Cargo features (the
`Dockerfile.builder` invocation above already does this) or display init
silently no-ops.

## Re-running against a different jayofelony release

Bump `IMG_URL`/`IMG_SHA256` at the top of `build.sh` to the new release's
asset URL and sha256 (compute it yourself with `sha256sum` if the
release's API metadata doesn't publish a digest -- several older releases
don't). Then re-verify nexmon is real for that specific release by
downloading it, mounting the root partition, and decompressing +
`strings`-checking the *active* `brcmfmac.ko` for `nexmon_nl_ioctl_handler`
-- do **not** rely on the presence/absence of a `brcmfmac-nexmon-dkms`
package name alone; that packaging changed in v2.9.5.4 but nexmon itself
appears present (baked into the module directly) in every release checked
before it. If considering a bullseye-era release (v2.8.x) for its
different boot behavior, also verify the bettercap/pwngrid/pwnagotchi
install paths directly -- v2.8.9's pwnagotchi is a system-wide pip
install, not the isolated venv this script's strip step currently assumes.
