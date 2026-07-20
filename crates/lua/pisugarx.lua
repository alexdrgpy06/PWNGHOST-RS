-- pisugarx plugin: PiSugar battery monitoring
-- Hooks are optional globals invoked by the agent's plugin manager.
-- Available globals when a hook runs: `epoch` (number), `status_json` (string).
--
-- Targets PiSugar 3, I2C address 0x57 (confirmed: PiSugar's own I2C
-- datasheet, github.com/PiSugar/PiSugar/wiki/PiSugar-3-I2C-Datasheet).
-- Register 0x2A is the calculated battery percentage, 0-100, single byte,
-- read-only (confirmed by the same datasheet).
-- Register 0x02 bit 7 ("power supply") reflects whether external power is
-- connected, which we use as a charging proxy -- PiSugar's own public docs
-- disagree with themselves on whether this is bit 7 or bit 6, so treat the
-- charging flag as best-effort, not authoritative. PiSugar 3 does not
-- expose a separate "is actually charging vs. fully charged" bit here, so
-- "power connected" is the closest we get without vendor tooling.

local M = { name = "pisugarx", enabled = true }

local I2C_BUS = 1
local I2C_ADDR = "0x57"
local REG_BATTERY_PCT = "0x2A"
local REG_POWER_STATUS = "0x02"
local POWER_SUPPLY_BIT = 0x80 -- bit 7, best-effort (see note above)

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

local function read_byte(reg)
  local cmd = string.format("i2cget -y %d %s %s 2>/dev/null", I2C_BUS, I2C_ADDR, reg)
  local p = io.popen(cmd)
  if not p then
    return nil
  end
  local out = p:read("*a")
  p:close()
  -- `tonumber(nil, 16)` errors (a base argument requires a real string),
  -- so the no-match case (device not responding, wrong address, etc. --
  -- confirmed on real hardware without a PiSugar attached: i2cget runs
  -- but returns nothing usable) must be checked before calling tonumber,
  -- not fall through into it.
  local hex = (out or ""):match("0x(%x+)")
  if not hex then
    return nil
  end
  return tonumber(hex, 16)
end

function on_ready()
  i2cget_path = detect_i2cget()
  if not i2cget_path then
    io.stderr:write("[pisugarx] i2cget not found on PATH, disabling (no-op)\n")
  else
    io.stderr:write(string.format(
      "[pisugarx] using i2cget at %s, querying PiSugar 3 @ %s\n",
      i2cget_path, I2C_ADDR
    ))
  end
  return true
end

function on_epoch()
  if not i2cget_path then
    if not disabled_logged then
      io.stderr:write("[pisugarx] disabled, i2cget unavailable\n")
      disabled_logged = true
    end
    return true
  end

  local percent = read_byte(REG_BATTERY_PCT)
  local status = read_byte(REG_POWER_STATUS)

  if percent == nil then
    io.stderr:write("[pisugarx] battery read failed this epoch (device not responding?)\n")
    return true
  end

  local charging = status and (status & POWER_SUPPLY_BIT) ~= 0
  local charging_str = status and (charging and "yes" or "no") or "unknown"

  io.stderr:write(string.format(
    "[pisugarx] battery=%d%% charging=%s\n", percent, charging_str
  ))
  return true
end

-- Called when the plugin is first loaded.
function on_loaded()
  return true
end

return M
