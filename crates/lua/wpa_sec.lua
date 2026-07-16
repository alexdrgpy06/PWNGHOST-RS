-- wpa_sec plugin: Uploads handshakes to wpa-sec
-- Hooks are optional globals invoked by the agent's plugin manager.
-- Available globals when a hook runs: `epoch` (number), `status_json` (string).

local M = { name = "wpa_sec", enabled = true }

-- Called once per epoch.
function on_epoch()
  -- Uploads handshakes to wpa-sec
  return true
end

-- Called when the plugin is first loaded.
function on_loaded()
  return true
end

return M
