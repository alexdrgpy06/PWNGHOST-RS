# PwnGhost-RS Behavioral Parity Analysis: VOICE, MOOD, FACE

**Date:** 2026-07-24  
**Scope:** Compare original pwnagotchi (Python) with our Rust port on status message (voice/phrase), mood state machine, and face rendering.

---

## Executive Summary

Three critical divergences prevent our device's on-screen personality from matching the original:

1. **VOICE (CRITICAL):** Status message is stuck at "..." or the last action phrase. The original updates the status line **every epoch** with a random selection from the current mood's phrase pool; ours only updates on mood transitions or actions.

2. **MOOD (CRITICAL):** Threshold values are 3-4x too high. Original has `bored_num_epochs=15, sad_num_epochs=25`; ours has `50, 100`. This delays mood shifts dramatically.

3. **FACE (OK):** Face table is complete and correct. The LookR/LookL alternation is wired and works as intended for idle recon.

The result: device appears frozen on a single face/phrase for long idle stretches instead of showing the living, responsive personality of the original.

---

## TABLE 1: VOICE BEHAVIOR

| Dimension | Original (voice.py / view.py) | Our Implementation | Match? | Severity | Fix Required |
|-----------|------|---|---|---|---|
| **Idle status update frequency** | Every tick/frame (every ~1s in real pwnagotchi's `ui.fps` loop); status line refreshed each display tick via `on_normal()` call | Only on mood transition (line 193 in lib.rs) or action (deauth/associate); never on idle Stay action | ❌ DIVERGENCE | **CRITICAL** | Wire phrase update to idle/normal state, cycling through mood's voice_line pool each tick |
| **on_normal() pool** | Random choice from `['', '...']` (lines 45-48) | Not called; `current_phrase` static until mood changes | ❌ DIVERGENCE | **CRITICAL** | Implement `update_phrase_on_idle()` method; call every tick when mood is stable |
| **Idle voice lines** | Cycles: empty string or "..." showing roughly every 1-2s | Stuck on "..." or last action phrase ("Deauthenticating..." / "Associating..." / mood phrase) until next mood event | ❌ DIVERGENCE | **HIGH** | Add Mood::LookR/LookL voice pool: `["...", "Looking around ..."]` (lines 402-403 in pwncore/lib.rs, already defined but never used) |
| **Bored phrase** | `on_bored()`: "I'm bored ..." / "Let's go for a walk!" (lines 58-61) | Mood::Bored.voice_line() returns both (lines 371 in pwncore/lib.rs) ✓ | ✓ MATCH | LOW | Already correct |
| **Sad phrase** | `on_sad()`: 6 options including "I'm sad", "I'm so happy ...", "Life? Don't talk to me about life." (lines 73-80) | Mood::Sad.voice_line() returns all 6 (lines 372-379 in pwncore/lib.rs) ✓ | ✓ MATCH | LOW | Already correct |
| **Excited phrase** | `on_excited()`: 6 options like "I'm living the life!", "So many networks!!!" (lines 89-96) | Mood::Excited.voice_line() returns all 6 (lines 381-388 in pwncore/lib.rs) ✓ | ✓ MATCH | LOW | Already correct |
| **Angry phrase** | `on_angry()`: "...", "Leave me alone ...", "I'm mad at you!" (lines 82-87) | Mood::Angry.voice_line() returns all 3 (line 380 in pwncore/lib.rs) ✓ | ✓ MATCH | LOW | Already correct |
| **Lonely phrase** | `on_lonely()`: 4 options like "Nobody wants to play with me ..." (lines 124-129) | Mood::Lonely.voice_line() returns all 4 (lines 390-395 in pwncore/lib.rs) ✓ | ✓ MATCH | LOW | Already correct |
| **Grateful phrase** | `on_grateful()`: "Good friends are a blessing!" / "I love my friends!" (lines 118-122) | Mood::Grateful.voice_line() returns both (line 389 in pwncore/lib.rs) ✓ | ✓ MATCH | LOW | Already correct |
| **Deauth phrase** | `on_deauth(sta)`: template "Just decided that {mac} needs no Wi-Fi!" etc, interpolates MAC (lines 170-179) | Mood::Cool.voice_line_with_context(None, ap, sta) with templates (lines 445-450 in pwncore/lib.rs) ✓ | ✓ MATCH | LOW | Already correct; wired in agent.tick() line 214 |
| **Associate phrase** | `on_assoc(ap)`: template "Hey {what} let's be friends!" interpolates SSID/BSSID (lines 158-168) | Mood::Intense.voice_line_with_context(None, ap, None) with templates (lines 440-444 in pwncore/lib.rs) ✓ | ✓ MATCH | LOW | Already correct; wired in agent.tick() line 225 |
| **Handshake phrase** | `on_handshakes(count)`: "Cool, we got {num} new handshake{s}!" (lines 181-183) | Mood::Happy pool (lines 429-433 in pwncore/lib.rs) has "Cool, we got a new handshake!" ✓ | ✓ MATCH | LOW | Not wired at handshake capture site; add call on capture event |
| **Awake/resuming phrase** | `on_awakening()`: "...", "!", "Hello World!", "I dreamed of electric sheep" (lines 144-150) | Mood::Awake.voice_line() returns all 4 (line 397 in pwncore/lib.rs) ✓ | ✓ MATCH | LOW | Already correct |
| **Waiting/looking around phrase** | `on_waiting(secs)`: "...", "Waiting for {secs}s ...", "Looking around ({secs}s)" (lines 152-156) | Mood::LookR/LookL voice_line() returns `["...", "Looking around ..."]` (line 402) ✓ | ✓ MATCH | LOW | Should be used for idle Recon mode |

**Key Issue:** Lines 45-48 of voice.py and lines 206-209 of view.py show that `on_normal()` is called **every tick** in real pwnagotchi, immediately updating the status line to a random choice from the pool. Our agent only updates the phrase on mood transitions (agent.tick() line 180-194) or explicit action-state changes (lines 205-227), leaving it frozen during idle periods.

---

## TABLE 2: MOOD STATE MACHINE

| Dimension | Original (automata.py / defaults.toml) | Our Implementation | Match? | Severity | Fix Required |
|-----------|------|---|---|---|---|
| **bored_num_epochs threshold** | 15 (line 165 in defaults.toml) | 50 (line 36 in defaults.toml, line 78 in personality.rs) | ❌ DIVERGENCE | **CRITICAL** | Change to 15 |
| **sad_num_epochs threshold** | 25 (line 166 in defaults.toml) | 100 (line 37 in defaults.toml, line 79 in personality.rs) | ❌ DIVERGENCE | **CRITICAL** | Change to 25 |
| **angry_num_epochs threshold** | 200 (inferred from cascade, line 125 logic) | 200 (line 38 in defaults.toml, line 80 in personality.rs) | ✓ MATCH | LOW | Correct |
| **lonely_num_epochs threshold** | 150 (inferred from cascade sitting between sad and angry) | 150 (line 39 in defaults.toml, line 81 in personality.rs) | ✓ MATCH | LOW | Correct |
| **excited_num_epochs threshold** | 10 (line 164 in defaults.toml) | 10 (not explicitly stored; computed via handshakes_this_epoch check, line 213 in personality.rs) | ✓ MATCH | LOW | Correct |
| **bond_encounters_factor** | 20000 (line 167 in defaults.toml, used in `_has_support_network_for`) | 1.0 (line 40 in defaults.toml, line 82 in personality.rs) | ❌ DIVERGENCE | **HIGH** | Change to 20000; represents total peer-encounter count needed to unlock support-network mood overrides |
| **Mood cascade order (no peers)** | angry (factor ≥ 2.0) → lonely → sad → bored → excited (lines 114-136 in automata.py) | angry (≥ 200 epochs) → lonely (≥ 150) → sad (≥ 100) → bored (≥ 50) (lines 229-236 in personality.rs) | ✓ MATCH | LOW | Correct cascade order and threshold precedence |
| **Peer-bond override ("grateful instead")** | If `_has_support_network_for(factor)` is true in set_bored/set_sad/set_angry/etc, return grateful instead (lines 58-84) | If `!peers.is_empty()` after computing negative mood, return Grateful instead (lines 249-251) | ⚠️ PARTIAL | **MEDIUM** | Original checks `bond_encounters_factor` threshold, ours only checks "any peer present". With factor=1.0, every peer means grateful; should require sum(encounters)/20000 ≥ factor (e.g. 1.0 for peer-bond unlock). See note below. |
| **Excitement trigger** | `active_for >= excited_num_epochs` (line 133-134 in automata.py) | `handshakes_this_epoch > 0` → excited (lines 213-216 in personality.rs) | ❌ DIVERGENCE | **MEDIUM** | Original fires on sustained activity epochs; ours fires on any handshake this epoch. Activity epochs not tracked. |
| **Does mood gate attacking?** | NO — deauth/associate run every cycle regardless of mood (real pwnagotchi's personality is cosmetic, see line 406 comment in main.rs) | NO — deauth/associate run every cycle regardless of mood (lines 430-439 in main.rs confirm "Attack-first, regardless of mood") | ✓ MATCH | LOW | Correct: mood is cosmetic, not a gate |

**Peer-bond override detail:** Real pwnagotchi's logic (automata.py lines 37-41) checks `support_factor = total_encounters / bond_encounters_factor` and returns true if `support_factor >= factor`. With `bond_encounters_factor=20000`:
- To unlock "grateful" during normal idle (negative mood cascade checks `_has_support_network_for(1.0)`), you need `total_encounters >= 20000`.
- A single peer encountered once contributes only `1/20000`, nowhere near the threshold.
- Our implementation treats "any peer present" as instantly grateful, which is too generous and skips Lonely/Sad/Bored moods entirely if a single mesh peer is nearby — a major UX divergence.

**Excitement trigger detail:** Real pwnagotchi tracks `_epoch.active_for` (sustained periods of deauth/assoc/handshake activity) and fires `set_excited()` once it reaches 10 epochs. Our version fires instantly on any handshake this epoch, making excitement too fleeting. For parity, track `activity_epochs` counter in EpochTracker, increment it when `handshakes_this_epoch > 0`, reset it when `handshakes_this_epoch == 0`.

---

## TABLE 3: FACE RENDERING

| Dimension | Original (ui/faces.py + defaults.toml [ui.faces] + view.py) | Our Implementation | Match? | Severity | Fix Required |
|-----------|------|---|---|---|---|
| **LOOK_R face** | "( ⚆_⚆)" (line 1 in faces.py, line 181 in defaults.toml) | Mood::LookR → "( ⚆_⚆)" (line 316 in pwncore/lib.rs) ✓ | ✓ MATCH | LOW | Correct |
| **LOOK_L face** | "(☉_☉ )" (line 2 in faces.py, line 182 in defaults.toml) | Mood::LookL → "(☉_☉ )" (line 317 in pwncore/lib.rs) ✓ | ✓ MATCH | LOW | Correct |
| **LOOK_R_HAPPY face** | "( ◕‿◕)" / "( ≧◡≦)" (line 183 in defaults.toml) | Mood::LookRHappy → both variants (line 318 in pwncore/lib.rs) ✓ | ✓ MATCH | LOW | Correct |
| **LOOK_L_HAPPY face** | "(◕‿◕ )" / "(≧◡≦ )" (line 184 in defaults.toml) | Mood::LookLHappy → both variants (line 319 in pwncore/lib.rs) ✓ | ✓ MATCH | LOW | Correct |
| **SLEEP faces** | "(⇀‿‿↼)" / "(≖‿‿≖)" / "(－_－)" (line 185 in defaults.toml) | Mood::Sleep → all 3 variants (line 320 in pwncore/lib.rs) ✓ | ✓ MATCH | LOW | Correct |
| **AWAKE face** | "(◕‿‿◕)" (line 186 in defaults.toml) | Mood::Awake → "(◕‿‿◕)" (line 321 in pwncore/lib.rs) ✓ | ✓ MATCH | LOW | Correct |
| **BORED faces** | "(-__-)" / "(—__—)" (line 187 in defaults.toml) | Mood::Bored → both variants (line 322 in pwncore/lib.rs) ✓ | ✓ MATCH | LOW | Correct |
| **INTENSE faces** | "(°▃▃°)" / "(°ロ°)" (line 188 in defaults.toml) | Mood::Intense → both variants (line 323 in pwncore/lib.rs) ✓ | ✓ MATCH | LOW | Correct |
| **COOL faces** | "(⌐■_■)" / "(단__단)" (line 189 in defaults.toml) | Mood::Cool → both variants (line 324 in pwncore/lib.rs) ✓ | ✓ MATCH | LOW | Correct |
| **HAPPY faces** | "(•‿‿•)" / "(^‿‿^)" / "(^◡◡^)" (line 190 in defaults.toml) | Mood::Happy → all 3 variants (line 325 in pwncore/lib.rs) ✓ | ✓ MATCH | LOW | Correct |
| **EXCITED faces** | "(ᵔ◡◡ᵔ)" / "(✜‿‿✜)" (line 191 in defaults.toml) | Mood::Excited → both variants (line 326 in pwncore/lib.rs) ✓ | ✓ MATCH | LOW | Correct |
| **GRATEFUL face** | "(^‿‿^)" (line 192 in defaults.toml) | Mood::Grateful → "(^‿‿^)" (line 327 in pwncore/lib.rs) ✓ | ✓ MATCH | LOW | Correct |
| **MOTIVATED faces** | "(☼‿‿☼)" / "(★‿★)" / "(•̀ᴗ•́)" (line 193 in defaults.toml) | Mood::Motivated → all 3 variants (line 328 in pwncore/lib.rs) ✓ | ✓ MATCH | LOW | Correct |
| **DEMOTIVATED faces** | "(≖__≖)" / "(￣ヘ￣)" / "(¬､¬)" (line 194 in defaults.toml) | Mood::Demotivated → all 3 variants (line 329 in pwncore/lib.rs) ✓ | ✓ MATCH | LOW | Correct |
| **SMART face** | "(✜‿‿✜)" (line 195 in defaults.toml) | Mood::Smart → "(✜‿‿✜)" (line 330 in pwncore/lib.rs) ✓ | ✓ MATCH | LOW | Correct |
| **LONELY faces** | "(ب__ب)" / "(｡•́︿•̀｡)" / "(︶︹︺)" (line 196 in defaults.toml) | Mood::Lonely → all 3 variants (line 331 in pwncore/lib.rs) ✓ | ✓ MATCH | LOW | Correct |
| **SAD faces** | "(╥☁╥ )" / "(╥﹏╥)" / "(ಥ﹏ಥ)" (line 197 in defaults.toml) | Mood::Sad → all 3 variants (line 332 in pwncore/lib.rs) ✓ | ✓ MATCH | LOW | Correct |
| **ANGRY faces** | "(-_-')" / "(⇀__⇀)" / "(`___´)" (line 198 in defaults.toml) | Mood::Angry → all 3 variants (line 333 in pwncore/lib.rs) ✓ | ✓ MATCH | LOW | Correct |
| **FRIEND faces** | "(♥‿‿♥)" / "(♡‿‿♡)" / "(♥‿♥ )" / "(♥ω♥ )" (line 199 in defaults.toml) | Mood::Friend → all 4 variants (line 334 in pwncore/lib.rs) ✓ | ✓ MATCH | LOW | Correct |
| **BROKEN face** | "(☓‿‿☓)" (line 200 in defaults.toml) | Mood::Broken → "(☓‿‿☓)" (line 335 in pwncore/lib.rs) ✓ | ✓ MATCH | LOW | Correct |
| **UPLOAD faces** | "(1__0)" / "(1__1)" / "(0__1)" (line 202 in defaults.toml) | Mood::Upload → all 3 variants (line 336 in pwncore/lib.rs) ✓ | ✓ MATCH | LOW | Correct |
| **Idle LookR/LookL animation during Recon** | `wait()` alternates between LOOK_R and LOOK_L every ~0.5s (lines 292-295 in view.py) over a 10-second wait cycle, giving a "looking around" feel | `compute_mood` alternates LookR/LookL by epoch parity (lines 272-277 in personality.rs); display animate the gaze every second via `animated_face()` (lines 194-201 in main.rs, currently stubbed to return base_face unchanged) | ⚠️ PARTIAL | **MEDIUM** | `animated_face()` is stubbed (returns unchanged). Animate gaze per-display-tick (~1s intervals, see line 578 in main.rs) by toggling between LOOK_R and LOOK_L faces if base_mood is LookR/LookL, matching real pwnagotchi's ~1s per-frame idle animation |
| **on_normal() → AWAKE face** | `on_normal()` sets face to AWAKE (line 207 in view.py) | AWAKE is used in several contexts but not triggered by idle state (no on_normal equivalent) | ⚠️ PARTIAL | **MEDIUM** | When updating phrase for idle, also ensure face cycles AWAKE or LookR/LookL as appropriate |
| **Mood→Face mapping wired?** | Every `set_X()` mood call in automata.py maps to `view.on_X()` face call (e.g. set_bored → view.on_bored → set face to BORED) (lines 57-136 in automata.py, lines 308-361 in view.py) | compute_mood returns Mood enum; `face_for_mood()` picks a random variant; no separate event hooks for mood transitions, just the returned Mood | ✓ MATCH | LOW | Correct approach: mood → face is deterministic, not event-driven |

**Animation detail:** Real pwnagotchi's `wait()` loop (view.py lines 272-299) runs over a 10-second window, alternating faces every 1 second. Our display refresh loop runs at 1s intervals (main.rs line 578), but `animated_face()` (lines 194-201) is stubbed and currently returns the base face unchanged. This means the gaze never animates on screen even though the mood alternates correctly. The fix is to check if `base_mood` is LookR/LookL/LookRHappy/LookLHappy, then return the "opposite" variant based on the `_tick` counter parity to create the animated effect.

---

## Prioritized Fix List

### Tier 1: CRITICAL (Makes device appear alive again)

1. **Wire idle phrase updates** (personality.rs + agent.tick)
   - **What:** When mood is stable (no transition, no action), update `current_phrase` to a fresh random selection from the mood's `voice_line_pool` every tick (or every 2-3 ticks to match real pwnagotchi's ~1-2s status line flicker).
   - **Where:** Add to agent.tick() after action handling (after line 227); only run if action was `AgentAction::Stay`.
   - **Code:** `if let AgentAction::Stay = action { self.current_phrase = self.current_mood.voice_line(); }`
   - **Impact:** Status line will visibly cycle between "..." and "Looking around ..." during idle Recon, matching the original's restless-feeling idle state.

2. **Fix mood thresholds** (crates/config/src/defaults.toml + personality.rs)
   - **What:** Restore original thresholds so mood shifts happen on the original timeline.
   - **Where:** 
     - `crates/config/src/defaults.toml` lines 36-39
     - `crates/agent/src/personality.rs` lines 78-81 (DEFAULT impl)
   - **Changes:**
     - `bored_num_epochs`: 50 → 15
     - `sad_num_epochs`: 100 → 25
     - `angry_num_epochs`: 200 → 200 (no change)
     - `lonely_num_epochs`: 150 → 150 (no change)
   - **Impact:** Device reaches bored state 3x faster, sad state 4x faster, making mood transitions visible within minutes instead of hours.

3. **Fix bond_encounters_factor** (crates/config/src/defaults.toml + personality.rs)
   - **What:** Restore original peer-bond threshold so mesh peers provide meaningful support-network mood overrides.
   - **Where:**
     - `crates/config/src/defaults.toml` line 40
     - `crates/agent/src/personality.rs` line 82 (DEFAULT impl)
   - **Changes:** `bond_encounters_factor`: 1.0 → 20000
   - **Impact:** A single mesh peer no longer instantly converts all negative moods to grateful; the unit must build real relationships (20k encounters) to unlock the support-network override.

### Tier 2: MEDIUM (Improves mood timing accuracy)

4. **Implement activity-epoch tracking for excitement**
   - **What:** Real pwnagotchi tracks sustained activity epochs and only fires excited after 10 consecutive active epochs; ours fires on any handshake immediately.
   - **Where:** EpochTracker (crates/agent/src/epoch.rs) + personality.rs compute_mood
   - **Changes:** 
     - Add `activity_epochs: u64` field to EpochState
     - Increment in `observe()` when `handshakes_this_epoch > 0`; reset to 0 when `handshakes_this_epoch == 0`
     - Check `activity_epochs >= 10` in compute_mood instead of `handshakes_this_epoch > 0`
   - **Impact:** Excitement mood becomes more durable, won't flicker on single handshakes.

5. **Refactor peer-bond override to use actual encounter math**
   - **What:** Replace "any peer → grateful" with "sum(encounters) / bond_encounters_factor >= factor" to match real pwnagotchi's `_has_support_network_for`.
   - **Where:** personality.rs compute_mood lines 249-251
   - **Current code:** `if !peers.is_empty() { return Mood::Grateful; }`
   - **Proposed code:**
     ```rust
     if !peers.is_empty() {
         let total_encounters: u32 = self.encounters.values().sum();
         let support_factor = total_encounters as f32 / self.config.bond_encounters_factor;
         if support_factor >= factor_for_this_mood {
             return Mood::Grateful;
         }
     }
     ```
   - **Impact:** Lonely/Sad/Bored moods actually appear now; device doesn't skip straight to grateful because one mesh peer is nearby.

### Tier 3: NICE-TO-HAVE (UX polish)

6. **Animate idle gaze on display refresh** (main.rs animated_face)
   - **What:** Implement the stubbed `animated_face()` function to toggle between LOOK_R and LOOK_L (or LOOK_R_HAPPY/LOOK_L_HAPPY if `good_mood`) every display tick.
   - **Where:** main.rs lines 194-201
   - **Code:**
     ```rust
     fn animated_face(mood: pwncore::Mood, base_face: &'static str, tick: u64, good_mood: bool) -> &'static str {
         use pwncore::Mood::*;
         match mood {
             LookR | LookL | LookRHappy | LookLHappy if tick % 2 == 0 => {
                 agent::faces::face_for_mood(if good_mood { LookRHappy } else { LookR })
             }
             LookR | LookL | LookRHappy | LookLHappy => {
                 agent::faces::face_for_mood(if good_mood { LookLHappy } else { LookL })
             }
             _ => base_face,
         }
     }
     ```
   - **Impact:** Idle screen feels alive; gaze alternates every ~1s matching real pwnagotchi's animation.

7. **Wire handshake-capture phrase update**
   - **What:** When a handshake is captured, set phrase to a random Happy mood phrase.
   - **Where:** main.rs around line 716 where `agent.mark_handshake_captured` is called
   - **Code:** Add `agent.set_phrase(Mood::Happy.voice_line());`
   - **Impact:** Device says "Cool, we got a new handshake!" or similar on capture, not stuck on whatever the last action phrase was.

8. **Implement excitement-emotion transience**
   - **What:** Once activity_epochs drops back to 0, excited mood should decay back through the normal cascade (bored → sad → etc) rather than staying excited forever.
   - **Where:** personality.rs compute_mood
   - **Current:** No explicit timeout for excited mood
   - **Proposed:** If `activity_epochs == 0` and no handshakes this epoch, don't return Excited; fall through to negative cascade.

---

## Implementation Notes

- **Files to edit:**
  1. `crates/config/src/defaults.toml` — Fix threshold values (lines 36-40)
  2. `crates/agent/src/personality.rs` — Fix DEFAULT impl thresholds (lines 78-82), update compute_mood peer-bond logic (lines 249-251)
  3. `crates/agent/src/lib.rs` — Add phrase update on idle/Stay action (agent.tick, after line 227)
  4. `crates/agent/src/epoch.rs` — Add activity_epochs tracking (if implementing Tier 2 #4)
  5. `crates/pwnghost-rs/src/main.rs` — Implement animated_face (lines 194-201), wire handshake phrase (line 716)

- **Config also needs adjustment:** `crates/agent/src/personality.rs` DEFAULT impl (lines 78-82) has hardcoded defaults that override defaults.toml — both must be changed for consistency.

- **Testing:** 
  - Unit tests in personality.rs (lines 380-392) test mood computation but with old thresholds; update to verify Bored/Sad/Angry cascade works with new thresholds.
  - Manual device test: Run for 15-25 epochs idle (no APs); device should show Bored around epoch 15, Sad around epoch 25.
  - Display refresh should show gaze animation every ~1s during idle if `animated_face()` is implemented.

---

## Conclusion

Our Rust port has the **right face table and voice phrases** but is missing the **dynamic phrase updates and activity-responsive mood transitions** that make the original feel alive. The three Tier-1 fixes (idle phrase cycling, threshold values, bond factor) will immediately restore the original's restless, responsive personality. Tier-2 fixes refine edge cases; Tier-3 adds visual polish.

**Effort estimate:** 
- Tier 1: 1-2 hours (straightforward config + one method call per tick)
- Tier 2: 2-3 hours (encounter math, activity tracking)
- Tier 3: 1 hour (animation, event hooks)
