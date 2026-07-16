-- bt_tether plugin: Bluetooth PAN tethering to a phone
-- Hooks are optional globals invoked by the agent's plugin manager.
-- Available globals when a hook runs: `epoch` (number), `status_json` (string).

local M = { name = "bt_tether", enabled = true }

-- Called once per epoch.
function on_epoch()
  -- Bluetooth PAN tethering to a phone
  return true
end

-- Called when the plugin is first loaded.
function on_loaded()
  return true
end

return M
