-- memtemp plugin: Shows memory and CPU temperature
-- Hooks are optional globals invoked by the agent's plugin manager.
-- Available globals when a hook runs: `epoch` (number), `status_json` (string).
--
-- This mirrors what crates/pwnghost-rs/src/main.rs already reads for the
-- e-ink footer (thermal_zone0 + /proc/meminfo). We don't touch the display
-- here; we just make the same numbers visible via the journal and a small
-- status file, for anything outside the display path (log scraping, other
-- plugins, `journalctl -u pwnghost-rs`) to consume.

local M = { name = "memtemp", enabled = true }

local STATUS_PATH = "/var/log/pwnghost/memtemp.status"

local function read_cpu_temp_c()
  local f = io.open("/sys/class/thermal/thermal_zone0/temp", "r")
  if not f then
    return nil
  end
  local raw = f:read("*a")
  f:close()
  local milli = tonumber((raw or ""):match("%-?%d+"))
  if not milli then
    return nil
  end
  return milli / 1000.0
end

local function read_ram_usage_mb()
  local f = io.open("/proc/meminfo", "r")
  if not f then
    return nil, nil
  end
  local total_kb, avail_kb = nil, nil
  for line in f:lines() do
    if not total_kb then
      total_kb = tonumber(line:match("^MemTotal:%s*(%d+)"))
    end
    if not avail_kb then
      avail_kb = tonumber(line:match("^MemAvailable:%s*(%d+)"))
    end
    if total_kb and avail_kb then
      break
    end
  end
  f:close()
  if not total_kb or not avail_kb then
    return nil, nil
  end
  local used_mb = math.floor((total_kb - avail_kb) / 1024)
  local total_mb = math.floor(total_kb / 1024)
  return used_mb, total_mb
end

-- Called once per epoch.
function on_epoch()
  local temp_c = read_cpu_temp_c()
  local used_mb, total_mb = read_ram_usage_mb()

  local temp_str = temp_c and string.format("%.1fC", temp_c) or "n/a"
  local mem_str = (used_mb and total_mb) and string.format("%d/%dMB", used_mb, total_mb) or "n/a"

  io.stderr:write(string.format(
    "[memtemp] epoch=%s temp=%s mem=%s\n",
    tostring(epoch), temp_str, mem_str
  ))

  local f = io.open(STATUS_PATH, "w")
  if f then
    f:write(string.format(
      "epoch=%s temp_c=%s mem_used_mb=%s mem_total_mb=%s ts=%d\n",
      tostring(epoch),
      temp_c and string.format("%.1f", temp_c) or "",
      used_mb and tostring(used_mb) or "",
      total_mb and tostring(total_mb) or "",
      os.time()
    ))
    f:close()
  end

  return true
end

-- Called when the plugin is first loaded.
function on_loaded()
  return true
end

return M
