-- cache plugin: Caches AP/handshake metadata
-- Hooks are optional globals invoked by the agent's plugin manager.
-- Available globals when a hook runs: `epoch` (number), `status_json` (string).
--
-- Flat `key=value` file, one entry per line, keyed by BSSID. Loaded once
-- into an in-Lua table at startup, rewritten in full on every write --
-- simplicity over performance, per this project's own plugin brief; the
-- table will never hold more entries than distinct APs handshake-captured
-- in a session, which is small.

local M = { name = "cache", enabled = true }

local CACHE_DIR = "/var/tmp/pwnghost"
local CACHE_PATH = CACHE_DIR .. "/plugin_cache.txt"

local cache = {}

local function load_cache()
  local f = io.open(CACHE_PATH, "r")
  if not f then
    return
  end

  for line in f:lines() do
    local key, value = line:match("^([^=]+)=(.*)$")
    if key then
      cache[key] = value
    end
  end
  f:close()
end

local function save_cache()
  local ok = os.execute('mkdir -p "' .. CACHE_DIR .. '"')
  if not ok then
    io.stderr:write("[cache] could not create cache dir " .. CACHE_DIR .. "\n")
    return
  end

  local f = io.open(CACHE_PATH, "w")
  if not f then
    io.stderr:write("[cache] could not open cache file for writing: " .. CACHE_PATH .. "\n")
    return
  end

  for key, value in pairs(cache) do
    f:write(key .. "=" .. value .. "\n")
  end
  f:close()
end

-- Called once at startup, after everything is initialized.
function on_ready()
  load_cache()
  local count = 0
  for _ in pairs(cache) do count = count + 1 end
  io.stderr:write("[cache] loaded " .. count .. " cached entries from " .. CACHE_PATH .. "\n")
  return true
end

function on_epoch()
  return true
end

function on_handshake()
  if handshake_bssid == nil or handshake_bssid == "" then
    return true
  end

  local existing = cache[handshake_bssid]
  local now = os.time()
  local value = tostring(now) .. ";" .. tostring(handshake_ssid or "")

  if existing then
    local prev_ts, prev_ssid = existing:match("^(%d+);(.*)$")
    io.stderr:write("[cache] repeat handshake for " .. handshake_bssid ..
      " (ssid=" .. tostring(handshake_ssid) ..
      ", previously seen at " .. tostring(prev_ts) ..
      (prev_ssid and prev_ssid ~= "" and (" as " .. prev_ssid) or "") .. ")\n")
  else
    io.stderr:write("[cache] new handshake cached for " .. handshake_bssid ..
      " (ssid=" .. tostring(handshake_ssid) .. ")\n")
  end

  cache[handshake_bssid] = value
  save_cache()

  return true
end

-- Called when the plugin is first loaded.
function on_loaded()
  return true
end

return M
