//! AngryOxide event parsing and output-directory watching.
//!
//! Real AngryOxide (Ragnt/AngryOxide) does not emit a structured JSON
//! protocol on stdout - that was a fabricated assumption in an earlier
//! version of this module. What AO actually gives us are two independent,
//! honest signal sources:
//!
//! 1. **Authoritative**: AO writes its capture output (`.pcapng`,
//!    `.hc22000`/hashline files, a kismetdb, and a final gzipped tarball) to
//!    the directory/prefix passed via `-o`. [`watch_output_dir`] polls that
//!    directory and emits [`AngryOxideEvent::HandshakeFileWritten`] /
//!    [`AngryOxideEvent::CaptureFileWritten`] when new or modified files show
//!    up. This is the only signal we trust for "did we actually capture
//!    something" - the caller is expected to validate/parse those files
//!    itself (see `agent::capture`), not infer anything from the filename.
//! 2. **Best-effort**: AO's headless stdout emits lines shaped like
//!    `{timestamp} | {message_type:^8} | {content}` (colorized with ANSI
//!    escapes), where `message_type` is one of `Error|Warning|Info|Priority|
//!    Status` and `content` is free-text, version-fragile human prose.
//!    [`parse_status_line`] strips the ANSI codes and extracts the level +
//!    message for logging/UI display *only*. We deliberately do not attempt
//!    to reverse-engineer AP/handshake/channel semantics out of `content` -
//!    that's exactly the fragile fabricated-protocol trap this module used
//!    to fall into.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use tokio::sync::mpsc;

/// Alias used across the crate for the parsed AngryOxide event enum.
pub type AoEvent = AngryOxideEvent;

/// Honest AngryOxide event vocabulary.
///
/// Every variant here corresponds to something we can actually observe:
/// a file AO wrote, or a status line AO printed. Nothing is inferred beyond
/// that.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AngryOxideEvent {
    /// A new or modified hashcat-ready handshake/PMKID file (`.hc22000` /
    /// `.22000`) appeared in AO's output directory. The path is passed
    /// through as-is; the caller (the capture pipeline) is responsible for
    /// validating and extracting the real BSSID from the file's contents.
    HandshakeFileWritten(PathBuf),

    /// A new or modified `.pcapng` capture file appeared in AO's output
    /// directory. Indicates general capture activity; not on its own proof
    /// of a captured handshake.
    CaptureFileWritten(PathBuf),

    /// A best-effort status line parsed from AO's headless stdout, after
    /// stripping ANSI color codes. Informational only - `message` is
    /// free-text prose from AO, not a structured payload.
    StatusLine { level: StatusLevel, message: String },
}

/// Severity of a parsed AngryOxide status line, matching the `message_type`
/// field AO prints (`Error|Warning|Info|Priority|Status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusLevel {
    Error,
    Warning,
    Priority,
    Status,
    Info,
}

impl StatusLevel {
    fn parse(s: &str) -> Option<Self> {
        match s.trim() {
            "Error" => Some(Self::Error),
            "Warning" => Some(Self::Warning),
            "Priority" => Some(Self::Priority),
            "Status" => Some(Self::Status),
            "Info" => Some(Self::Info),
            _ => None,
        }
    }
}

/// Strip ANSI SGR/color escape sequences (`ESC [ ... final-byte`) from a
/// line. AngryOxide colorizes its headless status output, so this has to
/// run before the `{timestamp} | {type} | {content}` split.
pub fn strip_ansi_codes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next(); // consume '['
                          // Consume parameter/intermediate bytes up to and including the
                          // CSI final byte (0x40..=0x7e, e.g. 'm' for SGR).
            for c2 in chars.by_ref() {
                if ('\x40'..='\x7e').contains(&c2) {
                    break;
                }
            }
            continue;
        }
        out.push(c);
    }

    out
}

/// Parse one line of AngryOxide's headless stdout into a [`StatusLine`]
/// event. Returns `None` for blank lines or lines that don't match the
/// expected `{timestamp} | {type:^8} | {content}` shape (e.g. TUI artifacts,
/// panics, or anything else that isn't a status line) - callers should treat
/// a `None` as "not a recognized status line", not an error.
pub fn parse_status_line(raw: &str) -> Option<AoEvent> {
    let line = strip_ansi_codes(raw);
    let line = line.trim();
    if line.is_empty() {
        return None;
    }

    let mut parts = line.splitn(3, '|');
    let _timestamp = parts.next()?;
    let level_str = parts.next()?;
    let message = parts.next()?.trim().to_string();

    let level = StatusLevel::parse(level_str)?;
    Some(AoEvent::StatusLine { level, message })
}

/// Poll `dir` for new or modified capture-related files written by
/// AngryOxide, emitting [`AngryOxideEvent::HandshakeFileWritten`] /
/// [`AngryOxideEvent::CaptureFileWritten`] as they appear.
///
/// AngryOxide has no push-based file-change notification, and adding a
/// dependency like `notify` (inotify-backed) buys little here: the interval
/// is generous, the directory is small, and polling degrades gracefully
/// across the tmpfs/network-mount configurations this runs under. So this
/// intentionally avoids a new crate dependency in favor of a plain
/// `tokio::time::interval` + mtime diff, mirroring the same approach already
/// used by `agent::capture::CaptureManager::scan_new_captures`.
///
/// Runs until the event channel's receiver is dropped. Safe to spawn once
/// per [`crate::spawn::AngryOxideHandle`] and leave running for the process
/// lifetime - it does not need to be restarted when the AO child process
/// itself crashes/restarts, since the output directory persists across
/// those restarts.
pub async fn watch_output_dir(dir: PathBuf, event_tx: mpsc::UnboundedSender<AoEvent>) {
    let mut seen: HashMap<PathBuf, SystemTime> = HashMap::new();
    let mut interval = tokio::time::interval(Duration::from_secs(2));

    loop {
        interval.tick().await;

        let mut entries = match tokio::fs::read_dir(&dir).await {
            Ok(entries) => entries,
            Err(_) => {
                // Directory may not exist yet (AO creates it lazily); just
                // retry on the next tick.
                continue;
            }
        };

        loop {
            let entry = match entries.next_entry().await {
                Ok(Some(entry)) => entry,
                Ok(None) => break,
                Err(_) => break,
            };

            let path = entry.path();
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or_default();
            let is_handshake = matches!(ext, "hc22000" | "22000");
            let is_capture = ext == "pcapng";
            if !is_handshake && !is_capture {
                continue;
            }

            let modified = match entry.metadata().await.and_then(|m| m.modified()) {
                Ok(m) => m,
                Err(_) => continue,
            };

            let changed = match seen.get(&path) {
                Some(prev) => *prev != modified,
                None => true,
            };
            if !changed {
                continue;
            }
            seen.insert(path.clone(), modified);

            let event = if is_handshake {
                AoEvent::HandshakeFileWritten(path)
            } else {
                AoEvent::CaptureFileWritten(path)
            };

            if event_tx.send(event).is_err() {
                return; // Receiver dropped; nothing left to do.
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_ansi_codes() {
        let input =
            "\u{1b}[32m2024-01-01 00:00:00 UTC\u{1b}[0m | \u{1b}[1mStatus\u{1b}[0m  | hello";
        let stripped = strip_ansi_codes(input);
        assert_eq!(stripped, "2024-01-01 00:00:00 UTC | Status  | hello");
    }

    #[test]
    fn test_strip_ansi_codes_no_codes() {
        assert_eq!(strip_ansi_codes("plain text"), "plain text");
    }

    #[test]
    fn test_parse_status_line_basic() {
        let line = "2024-01-01 00:00:00 UTC |  Status  | Channel hop to 6";
        let event = parse_status_line(line).unwrap();
        match event {
            AoEvent::StatusLine { level, message } => {
                assert_eq!(level, StatusLevel::Status);
                assert_eq!(message, "Channel hop to 6");
            }
            _ => panic!("expected StatusLine event"),
        }
    }

    #[test]
    fn test_parse_status_line_with_ansi() {
        let line =
            "\u{1b}[31m2024-01-01 00:00:00 UTC\u{1b}[0m | \u{1b}[31m Error  \u{1b}[0m | Something broke";
        let event = parse_status_line(line).unwrap();
        match event {
            AoEvent::StatusLine { level, message } => {
                assert_eq!(level, StatusLevel::Error);
                assert_eq!(message, "Something broke");
            }
            _ => panic!("expected StatusLine event"),
        }
    }

    #[test]
    fn test_parse_status_line_all_levels() {
        for level_str in ["Error", "Warning", "Priority", "Status", "Info"] {
            let line = format!("2024-01-01 00:00:00 UTC | {level_str} | msg");
            assert!(parse_status_line(&line).is_some(), "failed for {level_str}");
        }
    }

    #[test]
    fn test_parse_status_line_rejects_non_status_lines() {
        assert!(parse_status_line("").is_none());
        assert!(parse_status_line("not a status line at all").is_none());
        // A line with an unrecognized level token should not parse.
        assert!(parse_status_line("2024-01-01 | Bogus | message").is_none());
    }

    #[tokio::test]
    async fn test_watch_output_dir_detects_new_files() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_path_buf();

        let (tx, mut rx) = mpsc::unbounded_channel();
        let watch_dir = dir.clone();
        tokio::spawn(async move {
            watch_output_dir(watch_dir, tx).await;
        });

        // Give the watcher a tick to observe the empty directory first.
        tokio::time::sleep(Duration::from_millis(50)).await;

        tokio::fs::write(dir.join("capture.hc22000"), b"WPA*...")
            .await
            .unwrap();
        tokio::fs::write(dir.join("capture.pcapng"), b"pcap-bytes")
            .await
            .unwrap();

        let mut got_handshake = false;
        let mut got_capture = false;
        for _ in 0..10 {
            if let Ok(Some(event)) = tokio::time::timeout(Duration::from_secs(1), rx.recv()).await {
                match event {
                    AoEvent::HandshakeFileWritten(_) => got_handshake = true,
                    AoEvent::CaptureFileWritten(_) => got_capture = true,
                    _ => {}
                }
            }
            if got_handshake && got_capture {
                break;
            }
        }

        assert!(got_handshake, "expected a HandshakeFileWritten event");
        assert!(got_capture, "expected a CaptureFileWritten event");
    }
}
