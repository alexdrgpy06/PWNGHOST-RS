//! bettercap REST client -- the real capture engine for PWNGHOST-RS.
//!
//! Replaces AngryOxide (Phase 1 of the rework plan): AngryOxide manages
//! monitor mode itself via netlink, which is incompatible with the Pi's
//! internal Broadcom/nexmon FullMAC chip (confirmed on real hardware: it
//! never receives a single frame -- `Frames: 0`, high `ERs`, `NetworkDown`,
//! `os error 132`/ERFKILL). bettercap is what real pwnagotchi uses on this
//! exact hardware, and it gives this project two things AngryOxide never
//! could: a live, observable AP/client list (the agent's "eyes") and a real
//! command channel (`wifi.deauth`/`wifi.assoc`/`wifi.recon.channel`, the
//! agent's "hands") -- see `client::BettercapClient`.

pub mod client;
pub mod session;

pub use client::BettercapClient;
pub use session::{BettercapAp, BettercapStation, WifiSession};
