-- grid plugin: reports session stats to the community-run opwngrid API so
-- this unit shows up on the public pwnagotchi map/leaderboard.
-- Hooks: on_ready, on_epoch. Available globals on_epoch: `epoch` (number),
-- `status_json` (string, see crates/agent/src/epoch.rs EpochState).

local CONFIG_PATH = "/etc/pwnghost/config.toml"

local initialized = false
local api_url = "https://api.opwngrid.com/api/v1/report"
local report_interval = 60
local unit_name = "pwnghost"

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

-- Real pwnagotchi's grid plugin never talks to opwngrid.xyz directly: it
-- calls a local `pwngrid-peer` daemon that owns an RSA-2048 unit identity
-- and does JWT-signed enrollment/reporting on its behalf. This project has
-- no such daemon and no way to hold a persistent signed identity from Lua,
-- so this is a best-effort, unauthenticated stand-in that just POSTs a
-- small stats blob to a configurable endpoint every few epochs -- expect
-- a real deployment to need `api_url` overridden or this to simply fail
-- (logged, non-fatal) against an unconfirmed endpoint shape.
local function init()
  if initialized then
    return
  end
  initialized = true

  unit_name = read_config_value("main", "name") or unit_name
  api_url = read_config_value("plugins.grid", "api_url") or api_url
  report_interval = tonumber(read_config_value("plugins.grid", "report_interval")) or report_interval

  io.stderr:write(string.format(
    "[grid] best-effort session reporting to %s every %d epoch(s) -- real opwngrid protocol needs a pwngrid-peer identity daemon this build doesn't have, endpoint unconfirmed\n",
    api_url, report_interval))
end

local function extract_number(json, field)
  return json:match('"' .. field .. '":(%-?%d+%.?%d*)')
end

local function extract_string(json, field)
  return json:match('"' .. field .. '":"([^"]*)"')
end

function on_ready()
  init()
  return true
end

function on_epoch()
  init()

  if epoch == nil or epoch == 0 or report_interval <= 0 or epoch % report_interval ~= 0 then
    return true
  end

  local total_epochs = extract_number(status_json, "total_epochs") or "0"
  local total_handshakes = extract_number(status_json, "total_handshakes") or "0"
  local aps_seen = extract_number(status_json, "aps_seen") or "0"
  local mood = extract_string(status_json, "mood") or "unknown"

  local payload = string.format(
    '{"name":"%s","epoch":%s,"total_epochs":%s,"total_handshakes":%s,"aps_seen":%s,"mood":"%s"}',
    unit_name, tostring(epoch), total_epochs, total_handshakes, aps_seen, mood)

  local cmd = string.format(
    "curl -s -m 10 -o /dev/null -w '%%{http_code}' -X POST -H 'Content-Type: application/json' -d '%s' '%s' 2>/dev/null",
    payload, api_url)

  local handle = io.popen(cmd)
  local http_code = handle and handle:read("*a")
  if handle then
    handle:close()
  end

  if http_code == "200" then
    io.stderr:write("[grid] session report accepted (epoch " .. tostring(epoch) .. ")\n")
  else
    io.stderr:write("[grid] session report failed or endpoint unconfirmed (http=" .. tostring(http_code) .. ")\n")
  end

  return true
end
