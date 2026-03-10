//! Backup and restore functionality

use crate::{into_storage_error, Result, RTDBError};
use std::fs::{self, File};

use std::path::{Path, PathBuf};
use tar::Builder;

/// Backup manager
pub struct BackupManager {
    /// Storage base path
    storage_path: PathBuf,
    /// Backup destination path
    backup_path: PathBuf,
}

impl BackupManager {
    /// Create new backup manager
    pub fn new(storage_path: impl AsRef<Path>, backup_path: impl AsRef<Path>) -> Self {
        Self {
            storage_path: storage_path.as_ref().to_path_buf(),
            backup_path: backup_path.as_ref().to_path_buf(),
        }
    }

    /// Create full backup
    pub fn create_backup(&self, name: &str) -> Result<PathBuf> {
        // Create backup directory
        fs::create_dir_all(&self.backup_path).map_err(into_storage_error)?;

        let backup_file = self.backup_path.join(format!("{}.tar.gz", name));
        let temp_dir = tempfile::tempdir().map_err(into_storage_error)?;
        let temp_backup = temp_dir.path().join("backup");

        // Copy storage to temp directory
        self.copy_dir_all(&self.storage_path, &temp_backup)?;

        // Create tar.gz archive
        let tar_file = File::create(&backup_file).map_err(into_storage_error)?;
        let enc = flate2::write::GzEncoder::new(tar_file, flate2::Compression::default());
        let mut tar = Builder::new(enc);

        tar.append_dir_all(".", &temp_backup)
            .map_err(into_storage_error)?;
        tar.finish().map_err(into_storage_error)?;

        Ok(backup_file)
    }

    /// Restore from backup
    pub fn restore_backup(&self, backup_file: impl AsRef<Path>) -> Result<()> {
        let backup_file = backup_file.as_ref();

        // Verify backup file exists
        if !backup_file.exists() {
            return Err(RTDBError::Storage(
                format!("Backup file not found: {:?}", backup_file)
            ));
        }

        // Create temp directory for extraction
        let temp_dir = tempfile::tempdir().map_err(into_storage_error)?;

        // Extract tar.gz
        let tar_file = File::open(backup_file).map_err(into_storage_error)?;
        let dec = flate2::read::GzDecoder::new(tar_file);
        let mut archive = tar::Archive::new(dec);

        archive.unpack(&temp_dir).map_err(into_storage_error)?;

        // Clear current storage
        if self.storage_path.exists() {
            fs::remove_dir_all(&self.storage_path).map_err(into_storage_error)?;
        }

        // Copy restored data
        let extracted = temp_dir.path();
        self.copy_dir_all(extracted, &self.storage_path)?;

        Ok(())
    }

    /// List available backups
    pub fn list_backups(&self) -> Result<Vec<BackupInfo>> {
        let mut backups = Vec::new();

        if !self.backup_path.exists() {
            return Ok(backups);
        }

        for entry in fs::read_dir(&self.backup_path).map_err(into_storage_error)? {
            let entry = entry.map_err(into_storage_error)?;
            let path = entry.path();

            if path.extension().map(|e| e == "gz").unwrap_or(false) {
                let metadata = entry.metadata().map_err(into_storage_error)?;
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                backups.push(BackupInfo {
                    name,
                    path,
                    size: metadata.len(),
                    created: metadata.created()
                        .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs())
                        .unwrap_or(0),
                });
            }
        }

        backups.sort_by(|a, b| b.created.cmp(&a.created));
        Ok(backups)
    }

    /// Copy directory recursively
    fn copy_dir_all(&self, src: impl AsRef<Path>, dst: impl AsRef<Path>) -> Result<()> {
        let src = src.as_ref();
        let dst = dst.as_ref();

        fs::create_dir_all(&dst).map_err(into_storage_error)?;

        for entry in fs::read_dir(src).map_err(into_storage_error)? {
            let entry = entry.map_err(into_storage_error)?;
            let path = entry.path();
            let dest = dst.join(entry.file_name());

            if path.is_dir() {
                self.copy_dir_all(&path, &dest)?;
            } else {
                fs::copy(&path, &dest).map_err(into_storage_error)?;
            }
        }

        Ok(())
    }
}

/// Backup information
#[derive(Debug, Clone)]
pub struct BackupInfo {
    /// Backup name
    pub name: String,
    /// Backup file path
    pub path: PathBuf,
    /// File size in bytes
    pub size: u64,
    /// Creation timestamp
    pub created: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_backup_restore() {
        let storage_dir = TempDir::new().unwrap();
        let backup_dir = TempDir::new().unwrap();

        // Create some test data
        fs::write(storage_dir.path().join("test.txt"), "test data").unwrap();

        // Create backup
        let manager = BackupManager::new(storage_dir.path(), backup_dir.path());
        let backup = manager.create_backup("test_backup").unwrap();

        assert!(backup.exists());

        // Delete original data
        fs::remove_file(storage_dir.path().join("test.txt")).unwrap();

        // Restore backup
        manager.restore_backup(&backup).unwrap();

        // Verify restored data
        let restored = fs::read_to_string(storage_dir.path().join("test.txt")).unwrap();
        assert_eq!(restored, "test data");
    }
}
