-- gps plugin: reads live GPS fixes from gpsd for wardriving
-- pwncore::Handshake.gps / GpsData already exist but nothing in
-- crates/agent currently populates them. This plugin reads fixes from a
-- gpsd instance expected to be listening on 127.0.0.1:2947 (the standard
-- gpsd port) and attaches them, rather than starting a second gpsd.
--
-- Stock Lua 5.4 stdlib has no raw TCP socket support, so gpsd's
-- JSON-over-TCP protocol can't be spoken directly from Lua -- `gpspipe`
-- (part of the gpsd-clients package) is used as the client instead, via
-- io.popen. If it isn't installed there is no fallback: that's a real,
-- logged limitation, not a fake fix.
-- Hooks are optional globals invoked by the agent's plugin manager.
-- Available globals when on_epoch runs: `epoch` (number), `status_json` (string).

local M = { name = "gps", enabled = true }

local FIX_DIR = "/var/tmp/pwnghost"
local FIX_PATH = FIX_DIR .. "/gps_fix.json"
local GPSD_ADDR = "127.0.0.1:2947"

local gpspipe_available = nil -- nil = unchecked, false/true = cached result

local function have_gpspipe()
  if gpspipe_available ~= nil then
    return gpspipe_available
  end
  local p = io.popen("command -v gpspipe 2>/dev/null")
  local out = p and p:read("*l") or nil
  if p then
    p:close()
  end
  gpspipe_available = out ~= nil and #out > 0
  return gpspipe_available
end

function on_ready()
  if not have_gpspipe() then
    io.stderr:write(
      "gps: gpspipe not found (install gpsd-clients) -- GPS tagging disabled; " ..
      "stock Lua has no raw socket support to talk to gpsd directly\n"
    )
  else
    io.stderr:write("gps: gpspipe found, will poll gpsd at " .. GPSD_ADDR .. " each epoch\n")
  end
  return true
end

function on_epoch()
  if not have_gpspipe() then
    return true
  end

  os.execute('mkdir -p "' .. FIX_DIR .. '"')

  -- -n 5: gpsd's first replies on a fresh connection are VERSION/WATCH
  -- handshake sentences, not TPV, so a few lines are requested to have a
  -- real chance of seeing one TPV sentence before the pipe closes.
  local p = io.popen('gpspipe -w -n 5 ' .. GPSD_ADDR .. ' 2>/dev/null')
  if not p then
    return true
  end

  local lat, lon, alt = nil, nil, nil
  for line in p:lines() do
    if line:find('"class":"TPV"') then
      local la = line:match('"lat":(%-?%d+%.?%d*)')
      local lo = line:match('"lon":(%-?%d+%.?%d*)')
      if la and lo then
        lat, lon = la, lo
        alt = line:match('"alt":(%-?%d+%.?%d*)')
        break
      end
    end
  end
  p:close()

  if not (lat and lon) then
    -- No fix this epoch (no satellites yet, gpsd not warmed up, etc.) --
    -- a normal transient state, not an error worth logging every epoch.
    return true
  end

  local fix = string.format(
    '{"lat":%s,"lon":%s,"alt":%s,"epoch":%d,"timestamp":%d}',
    lat, lon, alt or "null", epoch or 0, os.time()
  )

  local f = io.open(FIX_PATH, "w")
  if f then
    f:write(fix)
    f:close()
  end

  -- Nothing currently reads FIX_PATH back into pwncore::GpsData or
  -- attaches it to a Handshake -- that would need a Rust-side change
  -- (e.g. on_handshake reading this file before the .hc22000 is saved)
  -- which is out of scope for this plugin. This only makes a real, fresh
  -- fix available at a well-known path for that future wiring to consume.
  return true
end

return M
