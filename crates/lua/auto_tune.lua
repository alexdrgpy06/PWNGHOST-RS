-- auto_tune plugin: Auto-tunes recon/attack timing
-- Hooks are optional globals invoked by the agent's plugin manager.
-- Available globals when a hook runs: `epoch` (number), `status_json` (string).
--
-- Real steering is not possible today: AngryOxide is spawned exactly once
-- at agent startup (crates/angryoxide/src/spawn.rs) with a fixed set of
-- CLI args built from `AngryOxideConfig` (crates/angryoxide/src/args.rs,
-- e.g. `--dwell`, `-r`/rate) baked in before the process starts, and it
-- manages its own channel hopping internally over netlink from then on --
-- there is no IPC/socket/signal this plugin (or anything else in the Rust
-- side) uses to push new timing into an already-running AO process. A
-- crash-triggered respawn (see spawn.rs's monitor loop) reuses the same
-- original config, not a freshly recomputed one. So this plugin only
-- observes and logs a diagnostic signal; it does not, and cannot today,
-- change anything live. A future version could feed this signal into
-- `config.toml`'s `[personality] min_recon_time/max_recon_time/
-- hop_recon_time` for the *next* restart, but that wiring doesn't exist
-- yet either.

local M = { name = "auto_tune", enabled = true }

local last_signal = nil

local function extract_number(json, field)
  local v = json:match('"' .. field .. '":(%d+)')
  return tonumber(v)
end

function on_epoch()
  if status_json == nil then
    return true
  end

  local aps_found = extract_number(status_json, "aps_found")
  local clients_seen = extract_number(status_json, "clients_seen")

  if aps_found == nil then
    return true
  end
  clients_seen = clients_seen or 0

  -- Rough heuristic: many APs/clients on the current channel means it's
  -- "busy" and productive to linger on; few or none means hopping faster
  -- would waste less time on a dead channel. Thresholds are arbitrary
  -- observation cutoffs, not tuned against real capture data.
  local signal
  if aps_found >= 5 or clients_seen >= 3 then
    signal = "busy"
  elseif aps_found == 0 and clients_seen == 0 then
    signal = "quiet"
  else
    signal = "moderate"
  end

  if signal ~= last_signal then
    if signal == "busy" then
      io.stderr:write("[auto_tune] epoch " .. tostring(epoch) ..
        ": channel busy (aps_found=" .. aps_found .. ", clients_seen=" .. clients_seen ..
        ") -- would benefit from longer dwell time (no live steering possible, AO is spawned once at startup)\n")
    elseif signal == "quiet" then
      io.stderr:write("[auto_tune] epoch " .. tostring(epoch) ..
        ": channel quiet (aps_found=0, clients_seen=0) -- could hop faster (no live steering possible, AO is spawned once at startup)\n")
    else
      io.stderr:write("[auto_tune] epoch " .. tostring(epoch) ..
        ": channel moderate (aps_found=" .. aps_found .. ", clients_seen=" .. clients_seen .. ")\n")
    end
    last_signal = signal
  end

  return true
end

-- Called when the plugin is first loaded.
function on_loaded()
  return true
end

return M
