-- cache plugin: Caches AP/handshake metadata
-- Hooks are optional globals invoked by the agent's plugin manager.
-- Available globals when a hook runs: `epoch` (number), `status_json` (string).

local M = { name = "cache", enabled = true }

-- Called once per epoch.
function on_epoch()
  -- Caches AP/handshake metadata
  return true
end

-- Called when the plugin is first loaded.
function on_loaded()
  return true
end

return M
