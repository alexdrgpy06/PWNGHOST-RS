# Rebase pipeline: jayofelony/pwnagotchi v2.9.5.4 + pwnghost-rs

An alternate image-build path that starts from a real, currently shipping
pwnagotchi release image instead of building the OS from scratch via
`pi-gen`, and strips out the Python pwnagotchi/bettercap/pwngrid stack in
favor of our own `pwnghost-rs` binary. This does **not** replace the
existing `pi-gen`-based build (see the repo root `pi-gen/` directory and
`Makefile`) -- both pipelines exist side by side so their resulting images
can be compared. See `SPEC.md`'s "Fidelity vs Reference Implementations"
section for the broader context that led here.

## Why this base, and why not others

Two other candidates were investigated and rejected first:

- **`jayofelony/pwnagotchi-bullseye` v2.8.4** (an older, now-archived
  release): its own README says, verbatim, "Please use this repo for all
  your images from now on" -- pointing at the repo used here instead. A
  full inspection of the actual image (loop-mounted, chrooted) found no
  real nexmon patching anywhere for BCM43436B0 (Pi Zero 2W's chip) -- no
  DKMS module, no patched firmware, monitor mode was just a plain `iw ...
  type monitor` on stock firmware. Stock brcmfmac generally can't inject
  attack frames, which is the whole point of a pwnagotchi. Dropped.
- A driverless/Microsoft-OS-descriptor RNDIS gadget (built by hand this
  same session for the from-scratch pi-gen build) turned out to be a
  *harder* goal than what pwnagotchi itself even attempts: jayofelony's
  own wiki (`Step-2-Connecting.md`, cloned directly from the wiki repo)
  says Windows users must download and install a driver
  (`rpi-usb-gadget-driver-setup.exe`) -- RNDIS isn't driverless even in
  the real, current image. Worth knowing before assuming this rebase
  gets you zero-driver-install Windows networking; it doesn't, any more
  than the reference implementation does.

**This base (`jayofelony/pwnagotchi` v2.9.5.4, 32-bit, `noai` branch)**
passed the same gate check. Confirmed directly from the release's own
published build manifest (`pwnagotchi-32bit-2.9.5.4.info`, a full `dpkg
-l` dump) and a full rootfs inspection (loop-mounted, not guessed):

| Package | Version | What it means |
|---|---|---|
| `brcmfmac-nexmon-dkms` | 6.12.2 | Real nexmon driver, DKMS-built against the running kernel |
| `firmware-nexmon` | 0.2 | Real patched firmware, not stock |
| `rpi-usb-gadget` | 1.0.6 | Raspberry Pi Foundation's own first-party USB-gadget setup package |
| `libnm0` | present | NetworkManager genuinely used |

`bcm2710-rpi-zero-2-w.dtb`/`bcm2708-rpi-zero-w.dtb` and
`brcmfmac43436-sdio*`/`brcmfmac43430*` firmware are all present for both
target boards. Still `pi-gen`-based (`Generated using pi-gen,
https://github.com/RPi-Distro/pi-gen, ..., stage3`), the same upstream
foundation PWNGHOST-RS's own build uses.

## What gets stripped, and why it's a lot

bettercap and pwngrid are **not** apt packages here -- they're Go binaries
built from source at image-build time (`stage3/03-bettercap-pwngrid`,
confirmed by reading that script directly), and pwnagotchi itself is a
pip-installed Python venv (`stage3/05-install-pwnagotchi`). None of this
shows up in a `dpkg -l` dump, so the `.info` manifest alone
undersells how much gets left behind. Directly measured on a mounted copy
of the real image before writing `build.sh`:

| Path | Size | What it is |
|---|---|---|
| `/root/.rustup` | 1.5 GB | Rust toolchain (for one pip dependency needing compilation) |
| `/home/pi/bettercap` | 146 MB | bettercap Go source tree, left after `make install` |
| `/home/pi/.pwn` | 226 MB | pwnagotchi's Python venv |
| `/usr/local/go` | 251 MB | Go toolchain |
| `/var/lib/apt/lists` + `/var/cache/apt` | 169 MB | apt cache |
| `/home/pi/pwngrid` | 29 MB | pwngrid Go source tree |

That's **~2.3 GB removable out of ~6.2 GB used** on the root partition --
independent, direct confirmation of what the earlier oxigotchi audit
found by reading jayofelony's install scripts alone (leftover Go/Rust
toolchains, uncleaned source trees). `build.sh` removes all of it,
`/usr/bin/pwnagotchi` and the `pwnagotchi.service`/`bettercap.service`/
`pwngrid-peer.service` units, then `apt-get autoremove --purge`s whatever
was only needed to build those.

**Explicitly not touched**: `brcmfmac-nexmon-dkms`, `firmware-nexmon`,
`rpi-usb-gadget`, NetworkManager, kernel/firmware/dtb files. The entire
point of this pipeline is to keep that proven hardware-enablement layer
exactly as shipped, not re-derive it -- this base's own USB-gadget/RNDIS
setup is used as-is; none of PWNGHOST-RS's own
`usb-gadget-setup.service`/`usb0.nmconnection`/`cmdline.txt` changes from
the from-scratch build are applied here.

## Building

Requires Docker (privileged, for loop-mount + chroot) and a pre-built set
of cross-compiled artifacts from the existing `Dockerfile.builder` (repo
root):

```bash
# From the repo root, produce artifacts/{arm-unknown-linux-gnueabihf,armv7-unknown-linux-gnueabihf}/{pwnghost-rs,wlan_keepalive}
docker build -t pwnghost-builder -f Dockerfile.builder .
docker create --name extract pwnghost-builder
docker cp extract:/workspace/artifacts ./tools/rebase-jayofelony/artifacts
docker rm extract

cd tools/rebase-jayofelony
docker build -t pwnghost-rebase-jayofelony .

# One run per board -- both share the same base image download/cache.
docker run --rm --privileged \
  -e BOARD=pi-zero-w \
  -v "$(pwd):/work" \
  pwnghost-rebase-jayofelony bash build.sh

docker run --rm --privileged \
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
- **Not carried over in this first pass**: BT-PAN tethering
  (`bt-agent.service`/`bt-pan@.service`) and `wifi-country.service`, since
  it's unverified whether they'd conflict with whatever mechanism (if
  any) this base image already uses for the same things. Worth revisiting
  once the core rebase is verified working on real hardware.

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

## Re-running against a future jayofelony release

Bump `IMG_URL`/`IMG_SHA256` at the top of `build.sh` to the new release's
asset URL and published sha256, then re-verify the gate-check packages
(`brcmfmac-nexmon-dkms`, `firmware-nexmon`, `rpi-usb-gadget`) are still
present in that release's `.info` manifest before trusting it -- don't
skip that check just because a past version passed it.
