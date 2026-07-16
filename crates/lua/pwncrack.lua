-- pwncrack plugin: Offloads cracking to a remote node
-- Hooks are optional globals invoked by the agent's plugin manager.
-- Available globals when a hook runs: `epoch` (number), `status_json` (string).

local M = { name = "pwncrack", enabled = true }

-- Called once per epoch.
function on_epoch()
  -- Offloads cracking to a remote node
  return true
end

-- Called when the plugin is first loaded.
function on_loaded()
  return true
end

return M
