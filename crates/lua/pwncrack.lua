-- pwncrack plugin: local dictionary cracking of captured handshakes
-- Real pwnagotchi-family "pwncrack"-style plugins run a local cracker
-- (hashcat/aircrack-ng) against a wordlist as an alternative to remote
-- services like wpa_sec. This only ever runs if both hashcat and a
-- configured wordlist are actually present -- neither is guaranteed on a
-- Pi Zero image (hashcat especially is a fairly heavy dependency), so the
-- absence of either is a normal, expected, quietly-logged-once state.
-- Hooks are optional globals invoked by the agent's plugin manager.
-- Available globals when on_handshake runs: `handshake_bssid`,
-- `handshake_ssid`, `handshake_path` (path to the validated .hc22000 file).

local M = { name = "pwncrack", enabled = true }

local CONFIG_PATH = "/etc/pwnghost/config.toml"
local OUT_DIR = "/var/tmp/pwnghost/pwncrack"
local TIMEOUT_SECS = 60

local hashcat_bin = nil
local wordlist = nil
local ready = false
local warned_disabled = false

local function trim(s)
  if not s then
    return nil
  end
  return (s:gsub("^%s+", ""):gsub("%s+$", ""))
end

local function find_hashcat()
  local p = io.popen("command -v hashcat 2>/dev/null")
  if not p then
    return nil
  end
  local out = trim(p:read("*l"))
  p:close()
  if out and #out > 0 then
    return out
  end
  return nil
end

-- No TOML library in stock Lua 5.4, so the wordlist path is pulled out of
-- the [plugins.pwncrack] table by scanning line-by-line for the next
-- `wordlist = "..."` key after that section header.
local function read_wordlist()
  local f = io.open(CONFIG_PATH, "r")
  if not f then
    return nil
  end
  local in_section = false
  for line in f:lines() do
    local section = line:match("^%s*%[([%w%._]+)%]%s*$")
    if section then
      in_section = (section == "plugins.pwncrack")
    elseif in_section then
      local v = line:match('^%s*wordlist%s*=%s*"(.-)"%s*$')
      if v then
        f:close()
        return v
      end
    end
  end
  f:close()
  return nil
end

function on_ready()
  hashcat_bin = find_hashcat()
  wordlist = read_wordlist()

  if not hashcat_bin then
    io.stderr:write("pwncrack: hashcat not found in PATH, local cracking disabled\n")
    return true
  end
  if not wordlist then
    io.stderr:write("pwncrack: no [plugins.pwncrack] wordlist configured, local cracking disabled\n")
    return true
  end

  local wl = io.open(wordlist, "r")
  if not wl then
    io.stderr:write("pwncrack: configured wordlist '" .. wordlist .. "' is not readable, local cracking disabled\n")
    return true
  end
  wl:close()

  ready = true
  io.stderr:write("pwncrack: ready (hashcat=" .. hashcat_bin .. ", wordlist=" .. wordlist .. ")\n")
  return true
end

function on_handshake()
  if not ready then
    if not warned_disabled then
      io.stderr:write("pwncrack: local cracking disabled, skipping all handshakes (see on_ready log)\n")
      warned_disabled = true
    end
    return true
  end

  os.execute('mkdir -p "' .. OUT_DIR .. '"')
  local safe_bssid = (handshake_bssid or "unknown"):gsub("[^%w]", "_")
  local out_file = OUT_DIR .. "/" .. safe_bssid .. ".cracked"
  os.remove(out_file)

  -- --potfile-disable + our own -o scopes the result to this one
  -- handshake instead of hashcat's shared global potfile; `timeout` bounds
  -- a worst-case dictionary run so one slow/large wordlist can't stall the
  -- epoch loop indefinitely.
  local cmd = string.format(
    'timeout %d %s -m 22000 -a 0 "%s" "%s" --potfile-disable -o "%s" --force >/dev/null 2>&1',
    TIMEOUT_SECS, hashcat_bin, handshake_path, wordlist, out_file
  )
  os.execute(cmd)

  local f = io.open(out_file, "r")
  local line = f and f:read("*l")
  if f then
    f:close()
  end

  if line and #line > 0 then
    local password = line:match(":([^:]*)$")
    io.stderr:write(
      "pwncrack: CRACKED " .. tostring(handshake_ssid) .. " (" .. tostring(handshake_bssid) ..
      ") password=" .. tostring(password) .. "\n"
    )
  else
    io.stderr:write(
      "pwncrack: no crack within " .. TIMEOUT_SECS .. "s for " ..
      tostring(handshake_ssid) .. " (" .. tostring(handshake_bssid) .. ")\n"
    )
  end
  return true
end

return M
