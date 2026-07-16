-- auto_update plugin: Checks for firmware/software updates
-- Hooks are optional globals invoked by the agent's plugin manager.
-- Available globals when a hook runs: `epoch` (number), `status_json` (string).

local M = { name = "auto_update", enabled = true }

-- Called once per epoch.
function on_epoch()
  -- Checks for firmware/software updates
  return true
end

-- Called when the plugin is first loaded.
function on_loaded()
  return true
end

return M
