#!/bin/bash
# wifi-country.sh - Unblock WiFi rfkill and set the regulatory domain on
# first boot, so the device doesn't need a manual `raspi-config` step before
# WiFi monitor mode will come up. Runs once (guarded by a marker file) so it
# never overwrites a country the user later sets deliberately via
# raspi-config.
#
# The country code comes from /boot/country.txt (or /boot/firmware/country.txt
# on newer partition layouts) if present - a plain-text 2-letter code the
# user can drop onto the boot partition before flashing, the same convention
# stock Raspberry Pi OS uses for pre-seeding wpa_supplicant.conf. Defaults to
# US if absent, since *some* working default beats requiring a manual step
# for every user.

set -u

MARKER=/etc/pwnghost/.wifi-country-configured
[ -f "$MARKER" ] && exit 0

COUNTRY="US"
for f in /boot/country.txt /boot/firmware/country.txt; do
    if [ -f "$f" ]; then
        candidate="$(tr -d '[:space:]' < "$f" | tr '[:lower:]' '[:upper:]')"
        if [ "${#candidate}" -eq 2 ]; then
            COUNTRY="$candidate"
        fi
        break
    fi
done

rfkill unblock all 2>/dev/null || true
iw reg set "$COUNTRY" 2>/dev/null || echo "wifi-country.sh: 'iw reg set $COUNTRY' failed (iw missing?)" >&2
# Also run raspi-config's own path so wpa_supplicant.conf / NetworkManager
# (whichever this image actually uses) picks up the same value consistently.
raspi-config nonint do_wifi_country "$COUNTRY" 2>/dev/null || echo "wifi-country.sh: raspi-config do_wifi_country $COUNTRY failed" >&2

# Only mark this done if the kernel's regulatory domain actually reflects
# it -- both calls above swallow their own errors (a missing `iw` binary,
# confirmed to happen on real hardware, silently no-ops otherwise), and
# writing the marker on a no-op success means it never retries on a later
# boot, leaving the radio stuck on the restrictive "00" world domain
# indefinitely with no visible sign anything went wrong.
ACTUAL="$(iw reg get 2>/dev/null | grep -m1 '^country' | sed 's/^country \([A-Z][A-Z]\).*/\1/')"
if [ "$ACTUAL" = "$COUNTRY" ]; then
    mkdir -p "$(dirname "$MARKER")"
    echo "$COUNTRY" > "$MARKER"
else
    echo "wifi-country.sh: regulatory domain is '${ACTUAL:-unknown}', not '$COUNTRY' -- not writing marker, will retry next boot" >&2
fi
