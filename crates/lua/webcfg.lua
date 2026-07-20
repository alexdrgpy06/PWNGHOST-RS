-- webcfg plugin: config-writability sanity check
-- Real pwnagotchi's webcfg plugin serves an actual config-editing HTML
-- page. This project's Rust web server (crates/ui/web) already exposes a
-- real GET+POST /api/config route for that exact purpose (see
-- crates/ui/web/src/api.rs, crates/ui/web/src/server.rs), so re-serving a
-- config page from Lua would just duplicate it, not add anything. This
-- plugin's real job is narrower: a one-time startup check that the config
-- file the web route reads/writes is actually readable and writable by
-- this process, since a permissions problem would make POST /api/config
-- appear to succeed while silently failing to persist.
--
-- Note: as of this writing update_config() in api.rs only updates in-memory
-- state (see its "Save to disk would happen here" comment) -- it doesn't
-- persist to config.toml at all yet. This check still has value once that
-- TODO is wired up, and costs nothing to run in the meantime.
-- Hooks are optional globals invoked by the agent's plugin manager.
-- Available globals when a hook runs: `epoch` (number), `status_json` (string).

local M = { name = "webcfg", enabled = true }

local CONFIG_PATH = "/etc/pwnghost/config.toml"

function on_ready()
  local r = io.open(CONFIG_PATH, "r")
  if not r then
    io.stderr:write(
      "webcfg: " .. CONFIG_PATH .. " is not readable by this process -- " ..
      "GET /api/config will only ever return in-memory/default values\n"
    )
    return true
  end
  r:close()

  -- Open in append mode: a successful open+close proves write permission
  -- without truncating or otherwise touching the real file's contents,
  -- unlike opening with "w".
  local w = io.open(CONFIG_PATH, "a")
  if not w then
    io.stderr:write(
      "webcfg: " .. CONFIG_PATH .. " is not writable by this process -- " ..
      "POST /api/config changes cannot be persisted to disk\n"
    )
    return true
  end
  w:close()

  io.stderr:write("webcfg: " .. CONFIG_PATH .. " is readable and writable\n")
  return true
end

-- Nothing to do per-epoch: this plugin is a one-time startup check, not a
-- recurring task.
function on_epoch()
  return true
end

return M
