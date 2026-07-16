-- auto_tune plugin: Auto-tunes recon/attack timing
-- Hooks are optional globals invoked by the agent's plugin manager.
-- Available globals when a hook runs: `epoch` (number), `status_json` (string).

local M = { name = "auto_tune", enabled = true }

-- Called once per epoch.
function on_epoch()
  -- Auto-tunes recon/attack timing
  return true
end

-- Called when the plugin is first loaded.
function on_loaded()
  return true
end

return M
