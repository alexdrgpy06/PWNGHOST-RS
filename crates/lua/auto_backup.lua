-- auto_backup plugin: Periodically backs up handshakes
-- Hooks are optional globals invoked by the agent's plugin manager.
-- Available globals when a hook runs: `epoch` (number), `status_json` (string).

local M = { name = "auto_backup", enabled = true }

-- Called once per epoch.
function on_epoch()
  -- Periodically backs up handshakes
  return true
end

-- Called when the plugin is first loaded.
function on_loaded()
  return true
end

return M
