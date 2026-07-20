-- gpio_buttons plugin: Handles GPIO button presses
-- Hooks are optional globals invoked by the agent's plugin manager.
-- Available globals when a hook runs: `epoch` (number), `status_json` (string).
--
-- We have no raw ioctl/GPIO access from Lua (no libgpiod binding exposed to
-- the host), so this is implemented as polling `gpioget` once per epoch
-- rather than real interrupt-driven edge detection. That means presses
-- shorter than one epoch's interval can be missed -- acceptable for a
-- plugin whose job is "notice a physical button was pressed", not
-- millisecond-accurate input handling.
--
-- Default pin: BCM GPIO26. The display already owns BUSY=24, DC=25, RST=17
-- (see crates/pwnghost-rs display init), so 26 is chosen specifically to
-- avoid colliding with those.

local M = { name = "gpio_buttons", enabled = true }

local GPIO_CHIP = "gpiochip0"
local GPIO_LINE = 26

local gpioget_path = nil
local last_state = nil
local disabled_logged = false

local function detect_gpioget()
  local p = io.popen("command -v gpioget 2>/dev/null")
  if not p then
    return nil
  end
  local out = p:read("*a")
  p:close()
  out = out and out:gsub("%s+$", "") or ""
  if out == "" then
    return nil
  end
  return out
end

local function read_line_state()
  local cmd = string.format("gpioget %s %d 2>/dev/null", GPIO_CHIP, GPIO_LINE)
  local p = io.popen(cmd)
  if not p then
    return nil
  end
  local out = p:read("*a")
  p:close()
  local n = tonumber((out or ""):match("%d+"))
  return n
end

-- Called once, at startup, after the whole stack is up.
function on_ready()
  gpioget_path = detect_gpioget()
  if not gpioget_path then
    io.stderr:write("[gpio_buttons] gpioget not found on PATH, disabling (no-op)\n")
  else
    io.stderr:write(string.format(
      "[gpio_buttons] using gpioget at %s, polling %s line %d\n",
      gpioget_path, GPIO_CHIP, GPIO_LINE
    ))
  end
  return true
end

-- Called once per epoch.
function on_epoch()
  if not gpioget_path then
    if not disabled_logged then
      io.stderr:write("[gpio_buttons] disabled, gpioget unavailable\n")
      disabled_logged = true
    end
    return true
  end

  local state = read_line_state()
  if state == nil then
    io.stderr:write("[gpio_buttons] gpioget read failed this epoch\n")
    return true
  end

  if last_state ~= nil and state ~= last_state then
    if state == 0 then
      io.stderr:write(string.format("[gpio_buttons] button press detected on GPIO%d (falling edge)\n", GPIO_LINE))
    else
      io.stderr:write(string.format("[gpio_buttons] button release detected on GPIO%d (rising edge)\n", GPIO_LINE))
    end
  end
  last_state = state

  return true
end

-- Called when the plugin is first loaded.
function on_loaded()
  return true
end

return M
