-- gps plugin: Reads GPS position for wardriving
-- Hooks are optional globals invoked by the agent's plugin manager.
-- Available globals when a hook runs: `epoch` (number), `status_json` (string).

local M = { name = "gps", enabled = true }

-- Called once per epoch.
function on_epoch()
  -- Reads GPS position for wardriving
  return true
end

-- Called when the plugin is first loaded.
function on_loaded()
  return true
end

return M
