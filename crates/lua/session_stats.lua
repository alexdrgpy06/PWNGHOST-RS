-- session_stats plugin: periodic JSON snapshot of session state
-- Real pwnagotchi's session_stats plugin does the same thing: dump the
-- live session's key numbers to a well-known file so other tooling
-- (dashboards, scripts) can poll session state without touching the
-- running agent process itself.
-- Hooks are optional globals invoked by the agent's plugin manager.
-- Available globals when a hook runs: `epoch` (number), `status_json` (string).

local M = { name = "session_stats", enabled = true }

local STATS_DIR = "/var/tmp/pwnghost"
local STATS_PATH = STATS_DIR .. "/session_stats.json"

local dir_ready = false

local function ensure_dir()
  if dir_ready then
    return true
  end
  os.execute('mkdir -p "' .. STATS_DIR .. '"')
  local probe = io.open(STATS_DIR .. "/.session_stats_wtest", "w")
  if probe then
    probe:close()
    os.remove(STATS_DIR .. "/.session_stats_wtest")
    dir_ready = true
    return true
  end
  return false
end

-- No JSON library in stock Lua 5.4, so fields are pulled out of the raw
-- status_json string with plain patterns rather than a real parse.
local function field(json, key, pattern)
  return json:match('"' .. key .. '":' .. pattern)
end

function on_ready()
  io.stderr:write("session_stats: writing snapshots to " .. STATS_PATH .. " each epoch\n")
  return true
end

function on_epoch()
  if not ensure_dir() then
    io.stderr:write("session_stats: cannot create/write " .. STATS_DIR .. ", skipping snapshot\n")
    return true
  end

  local j = status_json or ""
  local epoch_n = field(j, "epoch", "(%d+)") or tostring(epoch or 0)
  local total_epochs = field(j, "total_epochs", "(%d+)") or "0"
  local total_handshakes = field(j, "total_handshakes", "(%d+)") or "0"
  local handshakes_this_epoch = field(j, "handshakes_this_epoch", "(%d+)") or "0"
  local aps_found = field(j, "aps_found", "(%d+)") or "0"
  local aps_seen = field(j, "aps_seen", "(%d+)") or "0"
  local clients_seen = field(j, "clients_seen", "(%d+)") or "0"
  local channel = field(j, "channel", "(%d+)") or "0"
  local deauths_sent = field(j, "deauths_sent", "(%d+)") or "0"
  local assoc_attempts = field(j, "assoc_attempts", "(%d+)") or "0"
  local mood = field(j, "mood", '"([%a_]+)"') or "unknown"
  local mode = field(j, "mode", '"([%a_]+)"') or "unknown"

  local snapshot = string.format(
    '{"epoch":%s,"total_epochs":%s,"total_handshakes":%s,' ..
      '"handshakes_this_epoch":%s,"aps_found":%s,"aps_seen":%s,' ..
      '"clients_seen":%s,"channel":%s,"deauths_sent":%s,' ..
      '"assoc_attempts":%s,"mood":"%s","mode":"%s","generated_at":%d}',
    epoch_n, total_epochs, total_handshakes, handshakes_this_epoch,
    aps_found, aps_seen, clients_seen, channel, deauths_sent,
    assoc_attempts, mood, mode, os.time()
  )

  -- Write to a temp file and rename over the real path so a concurrent
  -- reader never sees a half-written snapshot.
  local tmp_path = STATS_PATH .. ".tmp"
  local f = io.open(tmp_path, "w")
  if not f then
    io.stderr:write("session_stats: failed to open " .. tmp_path .. " for writing\n")
    return true
  end
  f:write(snapshot)
  f:close()
  os.rename(tmp_path, STATS_PATH)
  return true
end

return M
