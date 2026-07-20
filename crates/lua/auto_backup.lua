-- auto_backup plugin: Periodically backs up handshakes
-- Hooks are optional globals invoked by the agent's plugin manager.
-- Available globals when a hook runs: `epoch` (number), `status_json` (string).

local M = { name = "auto_backup", enabled = true }

local CONFIG_PATH = "/etc/pwnghost/config.toml"
local CONFIG_TARGET = "/etc/pwnghost/config.toml"
local HANDSHAKES_DIR = "/etc/pwnghost/handshakes"
local BACKUP_DIR = "/var/log/pwnghost/backups"

local DEFAULT_INTERVAL = 100
local DEFAULT_RETENTION = 5

local warned_no_tools = false

-- config.toml has no dedicated backup section by default; read
-- `[plugins.auto_backup]` overrides if the user added them, otherwise
-- fall back to the defaults documented in this project's task brief.
local function read_config()
  local interval = DEFAULT_INTERVAL
  local retention = DEFAULT_RETENTION

  local f = io.open(CONFIG_PATH, "r")
  if not f then
    return interval, retention
  end

  local in_section = false
  for line in f:lines() do
    if line:match("^%[plugins%.auto_backup%]") then
      in_section = true
    elseif line:match("^%[") then
      in_section = false
    elseif in_section then
      local v = line:match("interval%s*=%s*(%d+)")
      if v then interval = tonumber(v) end
      local r = line:match("retention%s*=%s*(%d+)")
      if r then retention = tonumber(r) end
    end
  end
  f:close()

  return interval, retention
end

local function dir_exists(path)
  local f = io.open(path .. "/.", "r")
  if f then
    f:close()
    return true
  end
  return false
end

-- List existing backup archives, oldest first (relies on the timestamped
-- filename sorting lexically the same as chronologically).
local function list_backups()
  local backups = {}
  local p = io.popen('ls -1 "' .. BACKUP_DIR .. '" 2>/dev/null')
  if not p then return backups end
  for name in p:lines() do
    if name:match("^pwnghost%-backup%-.*%.tar%.gz$") then
      table.insert(backups, name)
    end
  end
  p:close()
  table.sort(backups)
  return backups
end

local function prune_backups(retention)
  local backups = list_backups()
  local excess = #backups - retention
  if excess <= 0 then return end

  for i = 1, excess do
    local path = BACKUP_DIR .. "/" .. backups[i]
    os.remove(path)
    io.stderr:write("[auto_backup] pruned old backup: " .. backups[i] .. "\n")
  end
end

function on_epoch()
  local interval, retention = read_config()

  if epoch == nil or epoch == 0 or (epoch % interval) ~= 0 then
    return true
  end

  if not dir_exists(BACKUP_DIR) then
    local ok = os.execute('mkdir -p "' .. BACKUP_DIR .. '"')
    if not ok then
      if not warned_no_tools then
        io.stderr:write("[auto_backup] could not create backup dir " .. BACKUP_DIR .. ", skipping\n")
        warned_no_tools = true
      end
      return true
    end
  end

  local stamp = os.date("%Y%m%d-%H%M%S")
  local archive = BACKUP_DIR .. "/pwnghost-backup-" .. stamp .. ".tar.gz"

  -- Tar the config plus handshakes dir; tolerate either being absent
  -- (fresh install with no handshakes yet) via tar's own missing-file
  -- warnings rather than pre-checking every path ourselves.
  local cmd = string.format(
    'tar -czf "%s" -C / "%s" "%s" 2>/var/tmp/pwnghost/auto_backup_last_tar_err.log',
    archive,
    CONFIG_TARGET:gsub("^/", ""),
    HANDSHAKES_DIR:gsub("^/", "")
  )

  local ok = os.execute(cmd)
  if ok then
    io.stderr:write("[auto_backup] created backup: " .. archive .. "\n")
    prune_backups(retention)
  else
    io.stderr:write("[auto_backup] tar command failed, backup not created (epoch " .. tostring(epoch) .. ")\n")
  end

  return true
end

-- Called when the plugin is first loaded.
function on_loaded()
  return true
end

return M
