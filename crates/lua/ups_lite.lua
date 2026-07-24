-- ups_lite plugin: UPS-Lite battery monitoring
-- Hooks are optional globals invoked by the agent's plugin manager.
-- Available globals when a hook runs: `epoch` (number), `status_json` (string).
--
-- Targets the real "UPS Lite" HAT for Pi Zero (marbasec's UPSLite_Plugin
-- v1.3.0, the actual plugin real pwnagotchi ships). A previous version of
-- this file targeted a MAX17040 fuel gauge at I2C address 0x36 -- the
-- wrong chip and address entirely; its own in-file comment claiming that
-- was "the original pwnagotchi ups_lite plugin" target was factually
-- wrong. The real hardware is a CW2015 fuel gauge at address 0x62,
-- register 0x04 (SOC, a 16-bit UFP8.8 fixed-point value: integer percent
-- in the high byte, fractional percent/256 in the low byte).
--
-- Byte order note (not yet verified on real UPS Lite hardware -- flag this
-- honestly rather than assert it's confirmed): `i2cget -w` on this same
-- board's other I2C fuel-gauge plugin (pisugarx, a different chip) needed
-- its word read byte-swapped relative to register order. Applying the same
-- swap here since CW2015 is a similar-generation SMBus word device, but
-- this should be confirmed the first time real UPS Lite hardware is
-- available -- if the reported percentage looks wrong/inverted, try
-- removing the swap first.

local M = { name = "ups_lite", enabled = true }

local I2C_BUS = 1
local I2C_ADDR = "0x62"
local SOC_REG = "0x04"

local i2cget_path = nil
local disabled_logged = false

local function detect_i2cget()
  local p = io.popen("command -v i2cget 2>/dev/null")
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

-- Reads the CW2015 SOC register (word): a UFP8.8 fixed-point value, integer
-- percent in the high byte, fractional percent/256 in the low byte.
-- Returns nil on any failure so callers can degrade cleanly.
local function read_battery_percent()
  local cmd = string.format(
    "i2cget -y %d %s %s w 2>/dev/null",
    I2C_BUS, I2C_ADDR, SOC_REG
  )
  local p = io.popen(cmd)
  if not p then
    return nil
  end
  local out = p:read("*a")
  p:close()
  out = out and out:gsub("%s+$", "") or ""

  -- `tonumber(nil, 16)` errors (a base argument requires a real string),
  -- so the no-match case (device not responding, wrong address, etc.)
  -- must be checked before calling tonumber.
  local hex = out:match("0x(%x+)")
  if not hex then
    return nil
  end
  local raw = tonumber(hex, 16)
  if not raw then
    return nil
  end

  -- i2cget -w byte-swaps relative to register order (see file header) --
  -- the integer-percent byte ends up in the low byte of `raw`.
  local percent = raw & 0xFF
  if percent > 100 then
    percent = 100
  end
  return percent
end

function on_ready()
  i2cget_path = detect_i2cget()
  if not i2cget_path then
    io.stderr:write("[ups_lite] i2cget not found on PATH, disabling (no-op)\n")
  else
    io.stderr:write(string.format(
      "[ups_lite] using i2cget at %s, querying CW2015 @ %s reg %s\n",
      i2cget_path, I2C_ADDR, SOC_REG
    ))
  end
  return true
end

function on_epoch()
  if not i2cget_path then
    if not disabled_logged then
      io.stderr:write("[ups_lite] disabled, i2cget unavailable\n")
      disabled_logged = true
    end
    return true
  end

  local percent = read_battery_percent()
  if percent == nil then
    io.stderr:write("[ups_lite] battery read failed this epoch (device not responding?)\n")
    return true
  end

  io.stderr:write(string.format("[ups_lite] battery=%d%%\n", percent))
  return true
end

-- Called when the plugin is first loaded.
function on_loaded()
  return true
end

return M
