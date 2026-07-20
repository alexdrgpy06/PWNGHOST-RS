-- fix_services plugin: Restarts failed system services
-- Hooks are optional globals invoked by the agent's plugin manager.
-- Available globals when a hook runs: `epoch` (number), `status_json` (string).
--
-- systemd's own Restart= already handles crashes; this exists for the case
-- where a unit was stopped by something other than a crash (OOM killer
-- reaping wlan_keepalive's helper, a manual `systemctl stop` left in a bad
-- state, etc.) and never got restarted because, from systemd's point of
-- view, nothing failed. We only act on genuinely inactive units, and only
-- log+restart -- no repeated hammering within the same epoch.

local M = { name = "fix_services", enabled = true }

local UNITS = { "pwnghost-rs.service", "wlan_keepalive.service" }

local function is_active(unit)
  local ok = os.execute(string.format("systemctl is-active --quiet %s", unit))
  -- Lua 5.4's os.execute returns (true, "exit", 0) on success, or
  -- (nil/false, "exit", code) on nonzero exit -- normalize to boolean.
  return ok == true
end

local function restart(unit)
  io.stderr:write(string.format("[fix_services] %s is inactive, restarting\n", unit))
  local ok = os.execute(string.format("systemctl restart %s", unit))
  if ok == true then
    io.stderr:write(string.format("[fix_services] %s restart issued\n", unit))
  else
    io.stderr:write(string.format("[fix_services] %s restart command failed\n", unit))
  end
end

-- Called once per epoch.
function on_epoch()
  for _, unit in ipairs(UNITS) do
    if is_active(unit) then
      -- Healthy, nothing to log every epoch -- avoid journal spam.
    else
      restart(unit)
    end
  end
  return true
end

-- Called when the plugin is first loaded.
function on_loaded()
  return true
end

return M
