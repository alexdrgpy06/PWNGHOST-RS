-- auto_update plugin: Checks for firmware/software updates
-- Hooks are optional globals invoked by the agent's plugin manager.
-- Available globals when a hook runs: `epoch` (number), `status_json` (string).
--
-- There is no packaged release channel for this project (no apt repo, no
-- `pwnghost-rs --update` subcommand, no bundled updater binary anywhere in
-- the tree). The only thing that could honestly constitute "checking for
-- an update" here is: this binary was installed from a git checkout that
-- is still present on disk, and that checkout has a newer commit upstream.
-- If neither condition holds we say so once and then stay quiet.

local M = { name = "auto_update", enabled = true }

-- Every ~daily-equivalent at a typical epoch cadence of a few seconds to
-- ~30s per epoch; this is a rough order-of-magnitude interval, not a
-- promise, since epoch length varies with recon/attack timing.
local DEFAULT_INTERVAL = 2000

local CONFIG_PATH = "/etc/pwnghost/config.toml"

-- Plausible locations a git checkout of this project's source could live
-- on a running unit. None of these are created by any installer script
-- found in this repo (pi-gen/tools install a compiled binary only), so
-- this is a best-effort guess, not a documented convention.
local CANDIDATE_REPOS = {
  "/opt/pwnghost-rs",
  "/usr/local/src/pwnghost-rs",
  "/root/pwnghost-rs",
  "/home/pi/pwnghost-rs",
}

local have_git = nil
local have_curl = nil
local repo_path = nil
local update_source_known = false
local warned_no_source = false

local function tool_available(name)
  return os.execute("command -v " .. name .. " >/dev/null 2>&1") == true
end

local function is_git_repo(path)
  local f = io.open(path .. "/.git/HEAD", "r")
  if f then
    f:close()
    return true
  end
  return false
end

local function read_interval()
  local interval = DEFAULT_INTERVAL
  local f = io.open(CONFIG_PATH, "r")
  if not f then return interval end

  local in_section = false
  for line in f:lines() do
    if line:match("^%[plugins%.auto_update%]") then
      in_section = true
    elseif line:match("^%[") then
      in_section = false
    elseif in_section then
      local v = line:match("interval%s*=%s*(%d+)")
      if v then interval = tonumber(v) end
    end
  end
  f:close()
  return interval
end

-- Called once at startup, after everything is initialized.
function on_ready()
  have_git = tool_available("git")
  have_curl = tool_available("curl")

  for _, path in ipairs(CANDIDATE_REPOS) do
    if is_git_repo(path) then
      repo_path = path
      break
    end
  end

  if have_git and have_curl and repo_path then
    update_source_known = true
    io.stderr:write("[auto_update] found git checkout at " .. repo_path .. ", will check for updates periodically\n")
  else
    io.stderr:write("[auto_update] no real update source configured for this build (no git checkout of source found, or git/curl missing) -- auto-update is a no-op\n")
  end

  return true
end

function on_epoch()
  if not update_source_known then
    return true
  end

  local interval = read_interval()
  if epoch == nil or epoch == 0 or (epoch % interval) ~= 0 then
    return true
  end

  -- `git fetch` alone (no merge/reset) so this stays a check, not a
  -- silent self-update -- actually pulling and rebuilding a Rust binary
  -- from a running plugin VM is out of scope for what this hook can do.
  local ok = os.execute('cd "' .. repo_path .. '" && git fetch --quiet 2>/var/tmp/pwnghost/auto_update_last_err.log')
  if not ok then
    io.stderr:write("[auto_update] git fetch failed at epoch " .. tostring(epoch) .. "\n")
    return true
  end

  local behind = io.popen('cd "' .. repo_path .. '" && git rev-list --count HEAD..@{u} 2>/dev/null')
  if behind then
    local count = behind:read("*l")
    behind:close()
    count = tonumber(count) or 0
    if count > 0 then
      io.stderr:write("[auto_update] " .. count .. " new commit(s) available upstream at " .. repo_path .. " -- manual rebuild/restart required\n")
    else
      io.stderr:write("[auto_update] up to date (epoch " .. tostring(epoch) .. ")\n")
    end
  end

  return true
end

-- Called when the plugin is first loaded.
function on_loaded()
  return true
end

return M
