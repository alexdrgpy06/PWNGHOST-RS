# PWNGHOST-RS GitHub Actions Build

This workflow builds flashable SD card images for Raspberry Pi Zero W and Pi Zero 2W.

## Triggering a Build

### Automatic (on tag push):
```bash
git tag v1.0.0
git push origin v1.0.0
```

### Manual (workflow dispatch):
Go to Actions tab → "Build PWNGHOST-RS SD Card Image" → Run workflow → Select target (pi-zero-w or pi-zero-2w)

## Artifacts

The workflow produces:
- `pwnghost-rs-<version>-armhf.img.xz` - Compressed SD card image
- SHA256 checksums
- Build logs

## Flashing the Image

```bash
# Linux/macOS
xzcat pwnghost-rs-v1.0.0-armhf.img.xz | sudo dd of=/dev/sdX bs=4M status=progress

# Windows (use balenaEtcher or Rufus)
```

## Target Boards

| Board | Architecture | Target |
|-------|--------------|--------|
| Pi Zero W | ARMv6 | arm-unknown-linux-gnueabihf |
| Pi Zero 2W | ARMv7 | armv7-unknown-linux-gnueabihf |

## Display Support

- Waveshare 2.13" V4 (250x122)
- Waveshare 2.7" V4 (264x176)
- Waveshare 2.9" V4 (296x128)

## Default Credentials

- Web UI: http://pwnghost.local:8080 (user: changeme, pass: changeme)
- SSH: pwn@pwnghost.local (pass: pwnghost)
- Hostname: pwnghost.local

## Customization

Edit `pi-gen/config` to customize:
- `IMG_NAME`
- `TARGET_HOSTNAME`
- `FIRST_USER_NAME`
- `FIRST_USER_PASS`