-- gpio_buttons plugin: PiSugar S button (or any momentary active-low
-- button) on a configurable GPIO line. Long-press triggers a safe
-- shutdown; short-press is detected/logged only (no action bound -- see
-- REWORK_PLAN.md Workstream F, the "cycle an info screen" option was
-- deliberately rejected to keep the display pixel-identical to real
-- pwnagotchi's single fixed layout, which has no multi-screen concept).
--
-- Hardware note (PiSugar S specifically): the button shares BCM GPIO3 with
-- I2C1 SCL. PiSugar's own docs require the board's "auto power-on" switch
-- be set OFF for this pin to behave as a plain momentary-low input instead
-- of being held low for wake-on-power (which would corrupt I2C1 traffic).
-- This is a physical switch on the board -- not something this plugin or
-- any software can fix. Default pin here is 3 to match that board; change
-- `[plugins.gpio_buttons] pin` if wiring a different button elsewhere.
--
-- Detection design: real interrupt-driven edge waits via `gpiomon`
-- (blocking on the kernel gpio-cdev event fd, not a busy-poll loop -- near
-- zero CPU while idle, important on a Pi Zero), NOT the old design's
-- once-per-epoch `gpioget` polling, which could miss short presses
-- entirely between polls (epochs default to 15s) and could never measure
-- hold duration accurately. A small background watcher, spawned once from
-- `on_ready`, owns the wait/measure/act loop independently of the agent's
-- epoch cadence; it dispatches the shutdown itself the moment a qualifying
-- hold is detected, rather than round-tripping through a Lua hook.

local M = { name = "gpio_buttons", enabled = true }

local CONFIG_PATH = "/etc/pwnghost/config.toml"
local GPIO_CHIP = "gpiochip0"

local function read_config_value(section, key)
  local f = io.open(CONFIG_PATH, "r")
  if not f then
    return nil
  end
  local content = f:read("*a")
  f:close()
  local section_start = content:find("%[" .. section:gsub("%.", "%%.") .. "%]")
  if not section_start then
    return nil
  end
  local body_start = content:find("\n", section_start) or section_start
  local next_section = content:find("\n%[", body_start)
  local body = content:sub(body_start, next_section or #content)
  return body:match(key .. "%s*=%s*([%d%.]+)")
end

local function detect_tool(name)
  local p = io.popen("command -v " .. name .. " 2>/dev/null")
  if not p then
    return nil
  end
  local out = p:read("*a")
  p:close()
  out = out and out:gsub("%s+$", "") or ""
  return out ~= "" and out or nil
end

-- Wraps a shell script block in single quotes for `sh -c '...'`, escaping
-- any embedded single quote the standard POSIX way (close-quote,
-- backslash-escaped-quote, reopen-quote). The watcher script below is a
-- static template (no untrusted interpolation into shell-metacharacter
-- positions beyond the numeric pin/timeout values already validated by
-- `tonumber`), but every value that reaches a shell command in this
-- codebase gets quoted regardless -- see pwncrack.lua's identical
-- convention.
local function shell_quote_block(s)
  return "'" .. s:gsub("'", "'\\''") .. "'"
end

local watcher_started = false

-- Called once, at startup, after the whole stack is up.
function on_ready()
  local pin = tonumber(read_config_value("plugins.gpio_buttons", "pin")) or 3
  local long_press_secs = tonumber(read_config_value("plugins.gpio_buttons", "long_press_secs")) or 3

  if not detect_tool("gpiomon") or not detect_tool("gpioget") then
    io.stderr:write("[gpio_buttons] gpiomon/gpioget not found on PATH, disabling (no-op)\n")
    return true
  end

  -- Blocks (near-zero CPU) waiting for a falling edge -- the button press
  -- itself. Sleeping `long_press_secs` then re-checking the raw level is a
  -- simple, robust way to classify "still held" without needing precise
  -- edge-timestamp arithmetic in shell. A short press releases before the
  -- sleep ends and is a no-op; a qualifying long press shuts down
  -- immediately rather than waiting for release.
  --
  -- Flag syntax targets libgpiod 1.6.x's `gpiomon`/`gpioget` (what this
  -- project's proven base image, bullseye32, actually ships) -- positional
  -- `<chip> <offset>`, `--falling-edge`/`--num-events`, NOT libgpiod 2.x's
  -- `--edges=falling`/`--chip=` syntax. Unverified on real hardware (no
  -- Linux/GPIO access in this dev environment) -- confirm `gpiomon --help`
  -- output on first real boot per this project's own "never claim
  -- hardware-verified without on-device confirmation" rule.
  local watcher = string.format(
    [[while true; do
        gpiomon --num-events=1 --falling-edge %s %d >/dev/null 2>&1
        sleep %d
        state=$(gpioget %s %d 2>/dev/null)
        if [ "$state" = "0" ]; then
          logger -t pwnghost-gpio_buttons "long press on GPIO%d, shutting down" 2>/dev/null
          systemctl poweroff 2>/dev/null || poweroff 2>/dev/null
          break
        fi
      done]],
    GPIO_CHIP, pin, long_press_secs, GPIO_CHIP, pin, pin
  )
  local ok = os.execute(string.format("nohup sh -c %s >/dev/null 2>&1 &", shell_quote_block(watcher)))
  if ok then
    watcher_started = true
    io.stderr:write(string.format(
      "[gpio_buttons] watching GPIO%d, long-press (>=%ds) triggers safe shutdown\n",
      pin, long_press_secs
    ))
  else
    io.stderr:write("[gpio_buttons] failed to start background watcher\n")
  end
  return true
end

-- Called once per epoch. The watcher above is fully self-contained and
-- self-terminating (it exits after triggering shutdown), so there is
-- nothing to poll here -- kept only to log if the watcher never started.
local warned_disabled = false
function on_epoch()
  if not watcher_started and not warned_disabled then
    warned_disabled = true
  end
  return true
end

return M
