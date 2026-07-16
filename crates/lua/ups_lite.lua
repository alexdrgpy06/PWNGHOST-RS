-- ups_lite plugin: UPS-Lite battery monitoring
-- Hooks are optional globals invoked by the agent's plugin manager.
-- Available globals when a hook runs: `epoch` (number), `status_json` (string).

local M = { name = "ups_lite", enabled = true }

-- Called once per epoch.
function on_epoch()
  -- UPS-Lite battery monitoring
  return true
end

-- Called when the plugin is first loaded.
function on_loaded()
  return true
end

return M
