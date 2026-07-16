-- session_stats plugin: Tracks per-session statistics
-- Hooks are optional globals invoked by the agent's plugin manager.
-- Available globals when a hook runs: `epoch` (number), `status_json` (string).

local M = { name = "session_stats", enabled = true }

-- Called once per epoch.
function on_epoch()
  -- Tracks per-session statistics
  return true
end

-- Called when the plugin is first loaded.
function on_loaded()
  return true
end

return M
