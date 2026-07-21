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

-- POSIX shell single-quote escaping, same pattern as pwncrack.lua's
-- shell_quote -- phone_mac ultimately comes from a user-edited
-- config.toml value, so it shouldn't be trusted to never contain shell
-- metacharacters just because it's "supposed to be" a MAC address.
local function shell_quote(s)
  s = s or ""
  return "'" .. s:gsub("'", "'\\''") .. "'"
end

-- This project's Rust-side Bluetooth code (crates/radio/src/bluetooth.rs)
-- isn't wired into the running agent's mode-selection logic yet, so this
-- plugin drives the same bt-pan@<MAC>.service systemd template unit a
-- user would start by hand (installed by this project's own overlay --
-- see tools/rebase-jayofelony/overlay and pi-gen/stage5's runtime
-- overlay -- ExecStart=/usr/local/bin/bt-pan-connect %i). Just calling
-- `bluetoothctl connect` (an earlier revision of this plugin) only
-- establishes the BT-level connection, not the actual PAN network
-- interface (bnep0 + DHCP) -- bt-pan-connect does both, so going through
-- the systemd unit instead gives a real working tether, not just a paired
-- device.
local function mac_to_instance(mac)
  return (mac:gsub(":", "-"))
end

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

-- bluetoothctl reporting "Connected: yes" only proves the BT-level link
-- is up, not that the bnep PAN interface (the thing that actually gives
-- internet access) is. Check the systemd unit's own active state instead
-- -- bt-pan-connect only exits 0 (leaving the oneshot unit "active")
-- once it found a bnep interface and brought it up, so this is a real
-- tether check, not just a paired-device check.
local function pan_active(mac)
  local unit = "bt-pan@" .. mac_to_instance(mac) .. ".service"
  local ok = os.execute("systemctl is-active --quiet " .. shell_quote(unit))
  return ok == true
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

  if pan_active(phone_mac) then
    io.stderr:write("[bt_tether] " .. phone_mac .. " PAN tether already active\n")
    return true
  end

  io.stderr:write("[bt_tether] " .. phone_mac .. " PAN tether not active, starting bt-pan@ unit\n")
  local unit = "bt-pan@" .. mac_to_instance(phone_mac) .. ".service"
  local ok = os.execute("systemctl start " .. shell_quote(unit) .. " >/dev/null 2>&1")
  if ok == true then
    io.stderr:write("[bt_tether] " .. unit .. " started for " .. phone_mac .. "\n")
  else
    io.stderr:write("[bt_tether] " .. unit .. " failed to start for " .. phone_mac
      .. " (device may not be paired/in range yet -- bt-pan-connect itself also "
      .. "runs pair/trust/connect via bluetoothctl, see bt-pan-connect script)\n")
  end

  return true
end
