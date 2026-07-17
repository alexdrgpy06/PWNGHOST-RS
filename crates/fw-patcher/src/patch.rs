//! Patch parsing and application for CoderFX inplace-v7.txt format

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::Path;
use tempfile::NamedTempFile;
use tracing::{debug, info};

/// A single patch line from inplace-v7.txt
/// Format: offset_hex | old_bytes_hex | new_bytes_hex | layer | description
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchLine {
    pub offset: u64,
    pub old_bytes: Vec<u8>,
    pub new_bytes: Vec<u8>,
    pub layer: u8,
    pub description: String,
}

impl PatchLine {
    /// Parse a single line from inplace-v7.txt
    /// Format: offset_hex | old_bytes_hex | new_bytes_hex | layer | description
    pub fn parse(line: &str) -> Result<Self> {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            bail!("Empty or comment line");
        }

        let parts: Vec<&str> = line.split('|').map(|s| s.trim()).collect();
        if parts.len() != 5 {
            bail!("Expected 5 fields (offset | old | new | layer | desc), got {}", parts.len());
        }

        // Parse offset (0x-prefixed hex)
        let offset_str = parts[0].strip_prefix("0x").unwrap_or(parts[0]);
        let offset = u64::from_str_radix(offset_str, 16)
            .with_context(|| format!("Invalid offset hex: {}", parts[0]))?;

        // Parse old bytes
        let old_bytes = hex::decode(parts[1])
            .with_context(|| format!("Invalid old_bytes hex: {}", parts[1]))?;

        // Parse new bytes
        let new_bytes = hex::decode(parts[2])
            .with_context(|| format!("Invalid new_bytes hex: {}", parts[2]))?;

        // Lengths must match
        if old_bytes.len() != new_bytes.len() {
            bail!(
                "old_bytes ({} bytes) != new_bytes ({} bytes)",
                old_bytes.len(),
                new_bytes.len()
            );
        }

        // Parse layer (0-7 for CoderFX 8 layers)
        let layer = parts[3].parse::<u8>()
            .with_context(|| format!("Invalid layer: {}", parts[3]))?;
        if layer > 7 {
            bail!("Layer must be 0-7, got {}", layer);
        }

        let description = parts[4].to_string();

        Ok(PatchLine {
            offset,
            old_bytes,
            new_bytes,
            layer,
            description,
        })
    }

    /// Apply this patch line to a file
    pub fn apply<P: AsRef<Path>>(&self, file_path: P) -> Result<()> {
        let path = file_path.as_ref();
        let mut file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .with_context(|| format!("Failed to open firmware for patching: {}", path.display()))?;

        // Seek to offset
        file.seek(SeekFrom::Start(self.offset))
            .with_context(|| format!("Seek to offset 0x{:x} failed", self.offset))?;

        // Read and verify old bytes
        let mut buf = vec![0u8; self.old_bytes.len()];
        file.read_exact(&mut buf)
            .with_context(|| format!("Read at offset 0x{:x} failed", self.offset))?;

        if buf != self.old_bytes {
            bail!(
                "Byte mismatch at offset 0x{:x}: expected {:02x?}, got {:02x?}",
                self.offset,
                self.old_bytes,
                buf
            );
        }

        // Write new bytes
        file.seek(SeekFrom::Start(self.offset))
            .with_context(|| format!("Seek back to offset 0x{:x} failed", self.offset))?;
        file.write_all(&self.new_bytes)
            .with_context(|| format!("Write at offset 0x{:x} failed", self.offset))?;
        file.flush()
            .context("Flush after write failed")?;

        debug!(
            "Applied patch at 0x{:x} (layer {}): {}",
            self.offset, self.layer, self.description
        );
        Ok(())
    }
}

/// Parse the inplace-v7.txt patch file
pub fn parse_patch_file<P: AsRef<Path>>(path: P) -> Result<Vec<PatchLine>> {
    let path = path.as_ref();
    let file = fs::File::open(path)
        .with_context(|| format!("Failed to open patch file: {}", path.display()))?;
    let reader = BufReader::new(file);

    let mut lines = Vec::new();
    for (line_num, line) in reader.lines().enumerate() {
        let line = line.with_context(|| format!("Read error at line {}", line_num + 1))?;
        if let Ok(patch) = PatchLine::parse(&line) {
            lines.push(patch);
        }
        // Skip empty/comment lines silently
    }

    info!("Parsed {} patch lines from {}", lines.len(), path.display());
    Ok(lines)
}

/// Apply all patches to a firmware file (in-place, atomic via temp file)
pub fn apply_patches<P: AsRef<Path>>(
    firmware_path: P,
    patch_file_path: P,
    expected_output_sha256: &str,
) -> Result<()> {
    let firmware_path = firmware_path.as_ref();
    let patch_file_path = patch_file_path.as_ref();

    info!("Applying patches from {} to {}", patch_file_path.display(), firmware_path.display());

    // Read and parse patch file
    let patch_lines = parse_patch_file(patch_file_path)?;
    info!("Parsed {} patch lines", patch_lines.len());

    // Create a temporary copy of the firmware
    let temp_file = NamedTempFile::new()
        .context("Failed to create temporary file for patching")?;
    let temp_path = temp_file.path().to_path_buf();

    // Copy firmware to temp file
    fs::copy(firmware_path, &temp_path)
        .with_context(|| format!("Failed to copy firmware to temp: {}", firmware_path.display()))?;

    // Apply each patch
    for (i, line) in patch_lines.iter().enumerate() {
        debug!("Applying patch line {}: layer {} - {}", i + 1, line.layer, line.description);
        line.apply(&temp_path)
            .with_context(|| format!("Failed to apply patch line {} at offset 0x{:x}", i + 1, line.offset))?;
    }

    // Verify output hash
    let output_hash = file_sha256(&temp_path)?;
    if output_hash != expected_output_sha256 {
        bail!(
            "Output hash mismatch: expected {}, got {}",
            expected_output_sha256,
            output_hash
        );
    }

    info!("All patches applied successfully, output hash verified: {}", output_hash);

    // Atomic rename
    fs::rename(&temp_path, firmware_path)
        .with_context(|| format!("Failed to replace firmware with patched version: {}", firmware_path.display()))?;

    info!("Firmware patched successfully: {}", firmware_path.display());
    Ok(())
}

/// Apply patches with full verification (used by installer)
pub fn apply_patches_verified<P: AsRef<Path>>(
    firmware_path: P,
    patch_lines: &[PatchLine],
    expected_output_sha256: &str,
) -> Result<()> {
    let firmware_path = firmware_path.as_ref();

    info!("Applying {} patches with verification", patch_lines.len());

    // Create a temporary copy
    let temp_file = NamedTempFile::new()
        .context("Failed to create temporary file for patching")?;
    let temp_path = temp_file.path().to_path_buf();

    fs::copy(firmware_path, &temp_path)
        .with_context(|| format!("Failed to copy firmware to temp: {}", firmware_path.display()))?;

    // Apply each patch
    for (i, line) in patch_lines.iter().enumerate() {
        debug!("Applying patch line {}: layer {} - {}", i + 1, line.layer, line.description);
        line.apply(&temp_path)
            .with_context(|| format!("Failed to apply patch line {} at offset 0x{:x}", i + 1, line.offset))?;
    }

    // Verify output hash
    let output_hash = file_sha256(&temp_path)?;
    if output_hash != expected_output_sha256 {
        bail!(
            "Output hash mismatch: expected {}, got {}",
            expected_output_sha256,
            output_hash
        );
    }

    info!("All patches applied and verified: {}", output_hash);

    // Atomic rename
    fs::rename(&temp_path, firmware_path)
        .with_context(|| format!("Failed to replace firmware with patched version: {}", firmware_path.display()))?;

    info!("Firmware patched successfully: {}", firmware_path.display());
    Ok(())
}

/// Verify patches can be applied (dry run)
pub fn verify_patches<P: AsRef<Path>>(
    firmware_path: P,
    patch_file_path: P,
    expected_output_sha256: &str,
) -> Result<()> {
    let firmware_path = firmware_path.as_ref();
    let patch_file_path = patch_file_path.as_ref();

    let patch_lines = parse_patch_file(patch_file_path)?;
    
    // Read firmware into memory for verification
    let mut firmware = fs::read(firmware_path)
        .with_context(|| format!("Failed to read firmware: {}", firmware_path.display()))?;

    // Apply patches in memory
    for (i, line) in patch_lines.iter().enumerate() {
        let offset = line.offset as usize;
        let len = line.old_bytes.len();

        if offset + len > firmware.len() {
            bail!(
                "Patch line {} exceeds firmware size: offset 0x{:x} + {} > {}",
                i + 1,
                offset,
                len,
                firmware.len()
            );
        }

        // Verify old bytes match
        if &firmware[offset..offset + len] != line.old_bytes.as_slice() {
            bail!(
                "Dry run verification failed at line {}: old bytes don't match at offset 0x{:x}",
                i + 1,
                offset
            );
        }

        // Apply in memory
        firmware[offset..offset + len].copy_from_slice(&line.new_bytes);
    }

    // Verify output hash
    let output_hash = bytes_sha256(&firmware);
    if output_hash != expected_output_sha256 {
        bail!(
            "Dry run output hash mismatch: expected {}, got {}",
            expected_output_sha256,
            output_hash
        );
    }

    info!("Dry run verification passed: {} patches, output hash verified", patch_lines.len());
    Ok(())
}

/// Compute SHA256 of a file
pub fn file_sha256<P: AsRef<Path>>(path: P) -> Result<String> {
    let path = path.as_ref();
    let mut file = fs::File::open(path)
        .with_context(|| format!("Failed to open file for hashing: {}", path.display()))?;
    
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    
    loop {
        let n = file.read(&mut buf)
            .context("Read failed during hashing")?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    
    Ok(hex::encode(hasher.finalize()))
}

/// Compute SHA256 of bytes
pub fn bytes_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

/// Backup original firmware before patching
pub fn backup_firmware<P: AsRef<Path>>(firmware_path: P, backup_dir: P) -> Result<std::path::PathBuf> {
    let firmware_path = firmware_path.as_ref();
    let backup_dir = backup_dir.as_ref();
    
    fs::create_dir_all(backup_dir)
        .context("Failed to create backup directory")?;

    let firmware_name = firmware_path.file_name()
        .context("Firmware path has no filename")?;
    
    let hash = file_sha256(firmware_path)?;
    let backup_name = format!("{}.{}.bak", firmware_name.to_string_lossy(), &hash[..16]);
    let backup_path = backup_dir.join(backup_name);

    fs::copy(firmware_path, &backup_path)
        .with_context(|| format!("Failed to backup firmware to {}", backup_path.display()))?;

    info!("Firmware backed up to: {}", backup_path.display());
    Ok(backup_path)
}

/// Restore firmware from backup
pub fn restore_firmware<P: AsRef<Path>>(backup_path: P, target_path: P) -> Result<()> {
    let backup_path = backup_path.as_ref();
    let target_path = target_path.as_ref();

    if !backup_path.exists() {
        bail!("Backup file does not exist: {}", backup_path.display());
    }

    fs::copy(backup_path, target_path)
        .with_context(|| format!("Failed to restore firmware from backup"))?;

    info!("Firmware restored from backup: {}", backup_path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::io::Write;

    fn create_test_firmware() -> (NamedTempFile, Vec<u8>) {
        let mut f = NamedTempFile::new().unwrap();
        let data = vec![0xAA; 1000]; // 1KB of 0xAA
        f.write_all(&data).unwrap();
        f.flush().unwrap();
        (f, data)
    }

    fn create_test_patch_file() -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        // Patch offset 0x100: change 4 bytes from 0xAA to 0xBB
        writeln!(f, "0x100 | aaaaaaaa | bbbbbbbb | 0 | test patch").unwrap();
        f.flush().unwrap();
        f
    }

    #[test]
    fn test_parse_patch_line() {
        let line = "0x100 | aaaaaaaa | bbbbbbbb | 0 | test patch";
        let patch = PatchLine::parse(line).unwrap();
        
        assert_eq!(patch.offset, 0x100);
        assert_eq!(patch.old_bytes, vec![0xAA, 0xAA, 0xAA, 0xAA]);
        assert_eq!(patch.new_bytes, vec![0xBB, 0xBB, 0xBB, 0xBB]);
        assert_eq!(patch.layer, 0);
        assert_eq!(patch.description, "test patch");
    }

    #[test]
    fn test_parse_patch_line_length_mismatch() {
        let line = "0x100 | aa | bbbbbbbb | 0 | test";
        assert!(PatchLine::parse(line).is_err());
    }

    #[test]
    fn test_parse_patch_line_invalid_layer() {
        let line = "0x100 | aaaaaaaa | bbbbbbbb | 9 | test";
        assert!(PatchLine::parse(line).is_err());
    }

    #[test]
    fn test_parse_patch_file() {
        let patch_file = create_test_patch_file();
        let lines = parse_patch_file(patch_file.path()).unwrap();
        
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].offset, 0x100);
        assert_eq!(lines[0].old_bytes, vec![0xAA, 0xAA, 0xAA, 0xAA]);
        assert_eq!(lines[0].new_bytes, vec![0xBB, 0xBB, 0xBB, 0xBB]);
    }

    #[test]
    fn test_parse_patch_file_skips_comments() {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, "# This is a comment").unwrap();
        writeln!(f, "").unwrap();
        writeln!(f, "0x200 | aaaaaaaa | cccccccc | 1 | another patch").unwrap();
        f.flush().unwrap();
        
        let lines = parse_patch_file(f.path()).unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].offset, 0x200);
    }

    #[test]
    fn test_bytes_sha256() {
        let data = b"hello world";
        let hash = bytes_sha256(data);
        assert_eq!(hash.len(), 64);
    }

    #[test]
    fn test_file_sha256() {
        let (f, _) = create_test_firmware();
        let hash = file_sha256(f.path()).unwrap();
        assert_eq!(hash.len(), 64);
    }

    #[test]
    fn test_apply_patches() {
        let (firmware, original) = create_test_firmware();
        let patch_file = create_test_patch_file();
        
        // Expected hash after patching (0xBB at offset 0x100-0x103)
        let mut expected = original.clone();
        expected[0x100..0x104].copy_from_slice(&[0xBB, 0xBB, 0xBB, 0xBB]);
        let expected_hash = bytes_sha256(&expected);

        apply_patches(firmware.path(), patch_file.path(), &expected_hash).unwrap();
        
        // Verify the file was modified
        let modified = fs::read(firmware.path()).unwrap();
        assert_eq!(&modified[0x100..0x104], &[0xBB, 0xBB, 0xBB, 0xBB]);
    }

    #[test]
    fn test_verify_patches_dry_run() {
        let (firmware, original) = create_test_firmware();
        let patch_file = create_test_patch_file();
        
        let mut expected = original.clone();
        expected[0x100..0x104].copy_from_slice(&[0xBB, 0xBB, 0xBB, 0xBB]);
        let expected_hash = bytes_sha256(&expected);

        verify_patches(firmware.path(), patch_file.path(), &expected_hash).unwrap();
    }

    #[test]
    fn test_verify_patches_fails_on_mismatch() {
        let (firmware, _) = create_test_firmware();
        let patch_file = create_test_patch_file();
        
        // Wrong expected hash
        verify_patches(firmware.path(), patch_file.path(), &"0".repeat(64)).unwrap_err();
    }
}