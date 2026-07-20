-- logtail plugin: Tails the log to the display
-- Hooks are optional globals invoked by the agent's plugin manager.
-- Available globals when a hook runs: `epoch` (number), `status_json` (string).

local M = { name = "logtail", enabled = true }

local LOG_PATH = "/var/log/pwnghost/pwnghost.log"
local SNAPSHOT_DIR = "/var/tmp/pwnghost"
local SNAPSHOT_PATH = SNAPSHOT_DIR .. "/logtail.txt"
local TAIL_LINES = 20

local warned_missing_log = false

local function tail_lines(path, n)
  local f = io.open(path, "r")
  if not f then
    return nil
  end

  -- Log rotation (config.toml's `[main.log.rotation]`) caps file size, so
  -- reading the whole file each epoch is acceptable per this plugin's
  -- brief -- no need for a seek-from-end trick.
  local lines = {}
  for line in f:lines() do
    table.insert(lines, line)
    if #lines > n then
      table.remove(lines, 1)
    end
  end
  f:close()

  return lines
end

function on_epoch()
  local lines = tail_lines(LOG_PATH, TAIL_LINES)
  if lines == nil then
    if not warned_missing_log then
      io.stderr:write("[logtail] log file not found yet: " .. LOG_PATH .. "\n")
      warned_missing_log = true
    end
    return true
  end
  warned_missing_log = false

  local ok = os.execute('mkdir -p "' .. SNAPSHOT_DIR .. '"')
  if not ok then
    io.stderr:write("[logtail] could not create snapshot dir " .. SNAPSHOT_DIR .. "\n")
    return true
  end

  local out = io.open(SNAPSHOT_PATH, "w")
  if not out then
    io.stderr:write("[logtail] could not write snapshot: " .. SNAPSHOT_PATH .. "\n")
    return true
  end

  for _, line in ipairs(lines) do
    out:write(line .. "\n")
  end
  out:close()

  return true
end

-- Called when the plugin is first loaded.
function on_loaded()
  return true
end

return M
