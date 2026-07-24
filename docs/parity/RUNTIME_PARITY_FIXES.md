# Runtime Behavioral Parity: "Boots but never pwns" — Root Causes & Fixes

**Date:** 2026-07-24 · **Reference:** jayofelony clone `a15ae8fc` (v2.9.5.5)

## Symptom (reported on real hardware)

A freshly-flashed device "just cycles the looking-left/right face, no real message
change, no face change, and no pwning / actual action." The stats bar draws, but
the agent never mounts an attack.

We **reproduced this in a container** (mock bettercap feeding a real AP timeline):
`mood: LookL, face: (☉_☉ ), phrase: "...", aps: 0→3, handshakes: 0`, and **zero**
`Deauthing`/`Associating` commands over many epochs — even with a client-bearing AP
on the agent's current channel. The behavior is not a crash; the loop runs, but the
agent idles because it never *decides* to attack.

---

## Root causes (confirmed) and fixes

### 1. [BUG · HIGH] RL bandit (100% random) overrode the deterministic attacker
`init_agent` with no trained model returned a `BanditPolicy` with **`epsilon = 1.0`**
(100% uniform-random exploration) and all-zero q-values (`rl-agent/src/policy.rs`).
`Agent::select_action` consulted the RL policy **first** and returned its choice
unconditionally for Hop/Wait/Sleep — so on every fresh device the deterministic
`select_action_heuristic` (which *does* deauth/associate viable targets) was almost
never reached. Per epoch the random policy picks ~13/16 Hop, ~1/16 Deauth, ~1/16
Associate → the device flails and essentially never sustains a capture campaign.
Real pwnagotchi is a deterministic threshold model that attacks every viable target
each recon cycle.

**Fix:** `Agent::rl_drives_actions` flag (default **false**). The RL policy only
drives action selection when a genuinely trained model is loaded
(`main.rs`: `agent.rl_drives_actions = using_model`). Otherwise the deterministic
heuristic drives (matching the original); the bandit still learns in the background
so it's ready if a model is added. — `crates/agent/src/lib.rs` `select_action`,
`crates/pwnghost-rs/src/main.rs` RL init.

### 2. [BUG · HIGH] Heuristic gated attacking on cosmetic mood
`select_action_heuristic` evaluated deauth/associate targets **only** in the `_`
arm of a `match self.current_mood`; the `Bored`/`Sad`/`Lonely` arms returned `Hop`
and `Angry`/`Broken` returned `Sleep`. So a bored/sad/lonely device would hop right
past a perfectly good target and never pwn. In real pwnagotchi mood is cosmetic
(face + status line); recon/deauth/assoc runs every cycle regardless.

**Fix:** attack-first, independent of mood. Only `Broken` (firmware-watchdog
escalation) still backs off; every other mood evaluates targets first, then hops on
the recon timer. — `crates/agent/src/lib.rs` `select_action_heuristic`.
Locked by regression test `test_fresh_device_attacks_client_bearing_ap_even_when_bored`.

### 3. [BUG · HIGH] `wifi.recon` permanent-disable race (the likely on-hardware cause)
The daemon sent `wifi.recon on` **only once**, in a startup bootstrap window
(10 retries). bettercap runs as its own `Restart=always` systemd unit; if it starts
late or restarts *after* that window, recon stays **off forever** → `/api/session/wifi`
returns empty → 0 APs → idle → no pwning, silently, for the rest of uptime. Found by
the hardware boot audit (`HARDWARE_BOOT_AUDIT.md`).

**Fix:** the bootstrap command is hoisted to loop scope and **re-asserted on
reconnect** — whenever a poll succeeds after a failure (bettercap just (re)started),
the daemon re-runs the bootstrap and re-enables recon. — `crates/pwnghost-rs/src/main.rs`
(`bettercap_reachable` flag + re-assert in the poll loop).

### 4. [LOGIC-GAP · MED] Mood/interaction thresholds 3–4× off from the original
Verified against pwnagotchi's `defaults.toml`: ours had `bored_num_epochs=50`
(orig 15), `sad_num_epochs=100` (orig 25), `bond_encounters_factor=1.0` (orig 20000),
`max_interactions=10` (orig 3). Consequences: mood changed 3–4× too slowly; a single
mesh peer instantly flipped every negative mood to "grateful" (skipping bored/sad/
lonely); and — critically for pwning — the agent would re-attack the *same* AP up to
10 times, fixating on one clientless AP instead of moving on after 3 tries.

**Fix:** aligned all three sources (`config/defaults.toml`, `agent/personality.rs`
`Default`, `config/schema.rs` default fns) to pwnagotchi's values. The
`max_interactions=3` change directly restores channel progression: after 3 attempts
the agent stops targeting an AP and hops on to find better targets.

---

## Verification (live, container)

With fixes 1–2 built into the image, driven by the mock's real AP timeline:

```
phrase: "Associating ..."        # status message now changes (was frozen "...")
INFO Associating with 66:77:88:99:aa:bb   # agent autonomously attacks
```

Before the fix: **0** attack commands over 60s. After: the agent associates on its
own, *despite* being in the LookL idle mood (proving the mood-gating removal). With
`max_interactions=3` (fix 4) it then stops fixating, hops through channels
(`6→11→2→…→1`), and reaches the client-bearing AP to deauth + capture.

`cargo test --workspace`: **205 passing, 0 failing** (incl. the new regression test).

---

## Session 2 — "better version" increment + verification corrections

Several items flagged by the earlier auto-generated diffs **did not survive
verification against the real pwnagotchi source** — recording them so the docs
stay honest (the auto-generated `GAP_MATRIX.md` / `VOICE_MOOD_FACE_DIFF.md`
overstate gaps, same as the fabricated `/api/status` finding):

- **Voice/idle-line "cycling" — NOT a bug.** `pwnagotchi/voice.py::on_normal()`
  returns `random.choice(['', '...'])` — the original shows blank or `"..."` during
  normal recon. Our recon lines (`["...", "Looking around ..."]`) are already *more*
  expressive. The "no message change" symptom was the pwning stall (fixed); the
  status now changes on real events (`Deauthing…`/`Associating…`). **No change.**
- **Mood hooks — already wired.** `PluginManager::fire_mood_hook` *is* called, from
  `agent::Agent::tick()` (`lib.rs:181`) on every mood transition. The "implemented
  but never called" claim was wrong. **No change.**
- **Gaze animation — deliberately limited for e-ink.** `main.rs::animated_face` is a
  stub by design: continuously alternating LookR/LookL would force an e-ink partial
  refresh every ~1s (the original's `view.wait()` does this), wearing the panel out
  in weeks — which is exactly why pwnagotchi e-ink setups commonly run `fps=0`. The
  face still changes per-epoch (LookR↔LookL) and per-event, which is appropriate for
  this hardware. Left as-is intentionally; revisit only with an `ui.fps` knob.

**Fixes actually applied this session:**

- **[BUG · HIGH] WebUI had no authentication.** Real pwnagotchi wraps every web route
  in `with_auth` (`ui/web/handler.py:38+`); ours enforced nothing, so anyone on the
  network could read captured handshakes/cracked passwords, rewrite config, and
  reboot the unit. Added HTTP Basic auth middleware over the sensitive routes
  (`/`, `/ui`, all `/api/*`) using `ui.web.username`/`password` (already in config,
  never enforced — the "config exists, never wired" pattern). `/ws` + `/static` stay
  open (browsers can't attach Basic auth to a WS handshake). Empty username opts out.
  — `crates/ui/web/src/server.rs` (`basic_auth`, tests for 401/200/opt-out).
- **[LOGIC-GAP · LOW] `min_rssi` fallback** `-80` → `-200` in `personality.rs`
  `Default`, matching pwnagotchi (`defaults.toml` already used -200).

`cargo test --workspace`: **208 passing, 0 failing.**

## Still open

- **Hardware confirmation.** On the Pi, run the commands in `HARDWARE_BOOT_AUDIT.md`
  (`iw dev`, `systemctl status bettercap`, `journalctl -u bettercap -u pwnghost-rs`,
  and `curl -su pwnghost:pwnghost http://127.0.0.1:8081/api/session/wifi`) to confirm
  whether the recon-race (fix 3) or an interface/firmware issue was the on-device
  cause of the original "boots but never pwns".
- **Matrix reliability.** `GAP_MATRIX.md` remaining rows should each be re-verified
  against source before acting — several claims have not held up.
```
