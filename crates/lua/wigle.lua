-- wigle plugin: Uploads networks to WiGLE
-- Hooks are optional globals invoked by the agent's plugin manager.
-- Available globals when a hook runs: `epoch` (number), `status_json` (string).

local M = { name = "wigle", enabled = true }

-- Called once per epoch.
function on_epoch()
  -- Uploads networks to WiGLE
  return true
end

-- Called when the plugin is first loaded.
function on_loaded()
  return true
end

return M
