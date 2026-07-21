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

-- Minimal JSON string escaping (no library available in stock Lua 5.4),
-- matching the hand-rolled JSON convention already used by session_stats.lua.
local function json_escape(s)
  if not s then
    return ""
  end
  return (s:gsub('[\\"\n\r\t]', {
    ["\\"] = "\\\\",
    ['"'] = '\\"',
    ["\n"] = "\\n",
    ["\r"] = "\\r",
    ["\t"] = "\\t",
  }))
end

-- POSIX shell single-quote escaping for values interpolated into
-- os.execute()/io.popen() command strings below. Wrapping in single
-- quotes and escaping any embedded single quote (close-quote,
-- backslash-escaped-quote, reopen-quote) is the standard safe pattern --
-- unlike json_escape above (which only escapes for JSON string context),
-- this is required wherever a value crosses into an actual shell
-- command, since handshake_path/wordlist can in principle contain shell
-- metacharacters ($, `, ;, etc.) that a naive "%s" interpolation
-- wouldn't neutralize.
local function shell_quote(s)
  s = s or ""
  return "'" .. s:gsub("'", "'\\''") .. "'"
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

  os.execute('mkdir -p ' .. shell_quote(OUT_DIR))
  local safe_bssid = (handshake_bssid or "unknown"):gsub("[^%w]", "_")
  local out_file = OUT_DIR .. "/" .. safe_bssid .. ".cracked"
  os.remove(out_file)

  -- --potfile-disable + our own -o scopes the result to this one
  -- handshake instead of hashcat's shared global potfile; `timeout` bounds
  -- a worst-case dictionary run so one slow/large wordlist can't stall the
  -- epoch loop indefinitely. Every interpolated value is shell_quote()'d,
  -- not just naively wrapped in "%s" -- handshake_path/wordlist ultimately
  -- trace back to a capture filename / user-edited config.toml value, and
  -- neither is guaranteed free of shell metacharacters.
  local cmd = string.format(
    'timeout %d %s -m 22000 -a 0 %s %s --potfile-disable -o %s --force >/dev/null 2>&1',
    TIMEOUT_SECS, shell_quote(hashcat_bin), shell_quote(handshake_path), shell_quote(wordlist), shell_quote(out_file)
  )
  local started = os.time()
  os.execute(cmd)
  local duration = os.time() - started

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

    -- Also write a small, structured JSON summary alongside hashcat's raw
    -- potfile-format `.cracked` file, so the web UI can show cracked
    -- passwords without needing to parse hashcat's -m 22000 hash format
    -- (real BSSID/ESSID are already known here from the handshake hooks;
    -- re-deriving them from the potfile line would be redundant parsing).
    -- Same atomic write-then-rename convention as session_stats.lua.
    -- Richer metadata so the web UI can show HOW it was cracked, not just
    -- the result (C4b). `hash_type`/`attack_mode` are fixed by the hashcat
    -- invocation above (-m 22000 WPA, -a 0 straight dictionary).
    local wl_name = (wordlist or ""):match("([^/]+)$") or tostring(wordlist)
    local json = string.format(
      '{"bssid":"%s","ssid":"%s","password":"%s","cracked_at":%d,' ..
      '"source":"local","duration_secs":%d,"attack_mode":"straight (-a 0)",' ..
      '"wordlist":"%s","hash_type":"WPA (22000)"}',
      json_escape(handshake_bssid), json_escape(handshake_ssid),
      json_escape(password), os.time(), duration, json_escape(wl_name)
    )
    local json_path = OUT_DIR .. "/" .. safe_bssid .. ".json"
    local tmp_path = json_path .. ".tmp"
    local jf = io.open(tmp_path, "w")
    if jf then
      jf:write(json)
      jf:close()
      os.rename(tmp_path, json_path)
    end
  else
    io.stderr:write(
      "pwncrack: no crack within " .. TIMEOUT_SECS .. "s for " ..
      tostring(handshake_ssid) .. " (" .. tostring(handshake_bssid) .. ")\n"
    )
  end
  return true
end

return M
