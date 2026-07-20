-- ups_lite plugin: UPS-Lite battery monitoring
-- Hooks are optional globals invoked by the agent's plugin manager.
-- Available globals when a hook runs: `epoch` (number), `status_json` (string).
--
-- Targets the common "UPS Lite" HAT for Pi Zero (the one built around the
-- INA219/MAX17040-style fuel gauge sold by adafruit/geekworm-alike vendors
-- and used by the original pwnagotchi ups_lite plugin). That plugin reads
-- the MAX17040 fuel gauge over I2C at address 0x36, SOC register 0x04
-- (high byte = integer percent, per TI/Maxim's MAX17040 datasheet) --
-- confirmed against the public MAX17040 register map. Some UPS Lite clones
-- instead expose a simpler ADS1115 ADC-based voltage divider at a
-- different address; we do NOT attempt to auto-detect that variant here,
-- since it needs a different formula (voltage -> percent) rather than a
-- direct register read.

local M = { name = "ups_lite", enabled = true }

local I2C_BUS = 1
local I2C_ADDR = "0x36"
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

-- Reads the MAX17040 SOC register (word, big-endian: high byte is whole
-- percent). Returns nil on any failure so callers can degrade cleanly.
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
  -- so the no-match case (device not responding, wrong address, etc. --
  -- confirmed on real hardware without a UPS Lite attached: i2cget runs
  -- but returns nothing usable) must be checked before calling tonumber.
  local hex = out:match("0x(%x+)")
  if not hex then
    return nil
  end
  local raw = tonumber(hex, 16)
  if not raw then
    return nil
  end

  -- i2cget -w returns the two bytes swapped relative to register order,
  -- so the high (percent) byte ends up in the low byte of `raw`.
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
      "[ups_lite] using i2cget at %s, querying MAX17040 @ %s reg %s\n",
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
