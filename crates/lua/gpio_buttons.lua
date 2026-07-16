-- gpio_buttons plugin: Handles GPIO button presses
-- Hooks are optional globals invoked by the agent's plugin manager.
-- Available globals when a hook runs: `epoch` (number), `status_json` (string).

local M = { name = "gpio_buttons", enabled = true }

-- Called once per epoch.
function on_epoch()
  -- Handles GPIO button presses
  return true
end

-- Called when the plugin is first loaded.
function on_loaded()
  return true
end

return M
