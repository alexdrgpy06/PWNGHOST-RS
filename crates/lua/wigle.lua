-- wigle plugin: uploads discovered WiFi networks to wigle.net for
-- wardriving crowdsourcing.
-- Hooks: on_ready, on_handshake. Available globals on_handshake:
-- `handshake_bssid`, `handshake_ssid`, `handshake_path`.

local CONFIG_PATH = "/etc/pwnghost/config.toml"
local TMP_CSV = "/var/tmp/pwnghost/wigle_upload.csv"

local initialized = false
local api_key = nil
local have_gps = false

local function read_config_value(section, key)
  local f = io.open(CONFIG_PATH, "r")
  if not f then
    return nil
  end
  local content = f:read("*a")
  f:close()

  local section_start = content:find("%[" .. section:gsub("%.", "%%.") .. "%]")
  if not section_start then
    return nil
  end

  local body_start = content:find("\n", section_start) or section_start
  local next_section = content:find("\n%[", body_start)
  local body = content:sub(body_start, next_section or #content)

  return body:match(key .. '%s*=%s*"([^"]*)"') or body:match(key .. "%s*=%s*([%d%.]+)")
end

local function init()
  if initialized then
    return
  end
  initialized = true

  api_key = read_config_value("plugins.wigle", "api_key")
  have_gps = os.execute("command -v gpspipe >/dev/null 2>&1") == true

  if not api_key or api_key == "" then
    io.stderr:write("[wigle] no api_key configured under [plugins.wigle]; uploads disabled\n")
    return
  end

  -- Real wigle uploads need a per-AP GPS fix (bettercap's .gps.json/
  -- .geo.json sidecar files); this project only tracks AP *counts*
  -- (status_json's aps_found/aps_seen), not individual records, and has no
  -- GPS daemon wired up anywhere Lua can reach. The only real per-AP data
  -- available here at all is BSSID/SSID for networks that actually
  -- yielded a handshake (via on_handshake's globals) -- everything
  -- discovered-but-not-cracked is invisible to this plugin. We refuse to
  -- fabricate GPS coordinates, channel, or encryption details for a
  -- WigleWifi CSV row, so a row is only ever uploaded when a real fix is
  -- obtainable from `gpspipe` (a gpsd client) at the moment of capture.
  if have_gps then
    io.stderr:write("[wigle] ready; will upload handshake-captured networks with a live gpsd fix (gpspipe found)\n")
  else
    io.stderr:write("[wigle] gpspipe/gpsd not found; no real GPS source available, uploads will be skipped rather than faked\n")
  end
end

local function get_gps_fix()
  local handle = io.popen("gpspipe -w -n 10 2>/dev/null")
  if not handle then
    return nil
  end
  local out = handle:read("*a")
  handle:close()

  for line in out:gmatch("[^\n]+") do
    local lat = line:match('"lat":(%-?[%d%.]+)')
    local lon = line:match('"lon":(%-?[%d%.]+)')
    if lat and lon then
      return lat, lon
    end
  end
  return nil
end

function on_ready()
  init()
  return true
end

function on_handshake()
  init()

  if not api_key or api_key == "" or not have_gps then
    return true
  end

  local lat, lon = get_gps_fix()
  if not lat or not lon then
    io.stderr:write("[wigle] no live GPS fix; skipping upload for " .. tostring(handshake_ssid) ..
      " rather than fabricate coordinates\n")
    return true
  end

  local now = os.date("!%Y-%m-%d %H:%M:%S")
  -- AuthMode is inferred, not fabricated: only WPA/WPA2/WPA3-PSK networks
  -- produce a crackable 4-way handshake, so "[WPA]" is a safe generic tag;
  -- channel/RSSI are left blank since this build has no real source for
  -- them at handshake time.
  local csv = "WigleWifi-1.4,appRelease=1.0,model=pwnghost-rs,release=1.0,device=pwnghost,display=headless,board=RaspberryPi,brand=pwnghost\n" ..
    "MAC,SSID,AuthMode,FirstSeen,Channel,RSSI,CurrentLatitude,CurrentLongitude,AltitudeMeters,AccuracyMeters,Type\n" ..
    string.format("%s,%s,[WPA],%s,,,%s,%s,,,WIFI\n", handshake_bssid, handshake_ssid, now, lat, lon)

  local out = io.open(TMP_CSV, "w")
  if not out then
    io.stderr:write("[wigle] could not write temp csv file " .. TMP_CSV .. "\n")
    return true
  end
  out:write(csv)
  out:close()

  local cmd = string.format(
    "curl -s -m 30 -o /dev/null -w '%%{http_code}' -H 'Authorization: Basic %s' -H 'Accept: application/json' -F 'donate=false' -F 'file=@%s;type=text/csv' https://api.wigle.net/api/v2/file/upload 2>/dev/null",
    api_key, TMP_CSV)

  local handle = io.popen(cmd)
  local http_code = handle and handle:read("*a")
  if handle then
    handle:close()
  end
  os.remove(TMP_CSV)

  if http_code == "200" then
    io.stderr:write(string.format("[wigle] uploaded %s (%s)\n", tostring(handshake_ssid), tostring(handshake_bssid)))
  else
    io.stderr:write("[wigle] upload failed (http=" .. tostring(http_code) .. ")\n")
  end

  return true
end
