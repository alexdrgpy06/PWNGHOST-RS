-- bt_tether plugin: maintains a Bluetooth PAN tether to a phone for
-- internet access when there's no WiFi uplink.
-- Hooks: on_ready, on_epoch. Available globals on_epoch: `epoch` (number),
-- `status_json` (string).

local CONFIG_PATH = "/etc/pwnghost/config.toml"

local initialized = false
local have_bluetoothctl = false
local phone_mac = nil
local check_interval = 10

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

-- This project has no live Bluetooth PAN management wired into Lua's
-- reach (nothing here can call into `crates/radio/src/bluetooth.rs`), so
-- this plugin drives `bluetoothctl` directly, same as a user would from a
-- shell -- it can trust/pair-assuming devices, but won't fabricate a
-- working tether if the tooling isn't present.
local function init()
  if initialized then
    return
  end
  initialized = true

  have_bluetoothctl = os.execute("command -v bluetoothctl >/dev/null 2>&1") == true
  phone_mac = read_config_value("plugins.bt_tether", "mac")
  check_interval = tonumber(read_config_value("plugins.bt_tether", "check_interval")) or check_interval

  if not have_bluetoothctl then
    io.stderr:write("[bt_tether] bluetoothctl not found on this system; tethering disabled\n")
    return
  end

  if not phone_mac or phone_mac == "" then
    io.stderr:write("[bt_tether] no phone MAC configured under [plugins.bt_tether] (key 'mac'); tethering disabled\n")
    return
  end

  io.stderr:write(string.format("[bt_tether] ready, will check %s every %d epoch(s)\n", phone_mac, check_interval))
end

local function is_connected(mac)
  local handle = io.popen("bluetoothctl info " .. mac .. " 2>/dev/null")
  if not handle then
    return false
  end
  local out = handle:read("*a")
  handle:close()
  return out:find("Connected: yes") ~= nil
end

function on_ready()
  init()
  return true
end

function on_epoch()
  init()

  if not have_bluetoothctl or not phone_mac or phone_mac == "" then
    return true
  end

  if check_interval <= 0 or epoch == nil or epoch % check_interval ~= 0 then
    return true
  end

  if is_connected(phone_mac) then
    io.stderr:write("[bt_tether] " .. phone_mac .. " already connected\n")
    return true
  end

  io.stderr:write("[bt_tether] " .. phone_mac .. " not connected, attempting connect\n")
  local ok = os.execute("bluetoothctl connect " .. phone_mac .. " >/dev/null 2>&1")
  if ok == true then
    io.stderr:write("[bt_tether] connect command succeeded for " .. phone_mac .. "\n")
  else
    io.stderr:write("[bt_tether] connect command failed for " .. phone_mac .. "\n")
  end

  return true
end
