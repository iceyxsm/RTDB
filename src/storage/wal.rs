//! Write-Ahead Log (WAL) implementation
//! 
//! Provides crash recovery through append-only logs
//! with checksum verification.

use crate::{into_storage_error, Result, RTDBError};
use bytes::{Buf, BufMut, BytesMut};
use crc32c::crc32c;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// Magic number for WAL files
const WAL_MAGIC: u32 = 0x57414C21; // "WAL!"
/// WAL version
const WAL_VERSION: u16 = 1;
/// Header size
const HEADER_SIZE: usize = 16;
/// Max record size (16MB)
const MAX_RECORD_SIZE: usize = 16 * 1024 * 1024;

/// WAL entry types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EntryType {
    /// Full record fits in one chunk
    Full = 1,
    /// First chunk of a multi-chunk record
    First = 2,
    /// Middle chunk
    Middle = 3,
    /// Last chunk
    Last = 4,
}

impl EntryType {
    fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(EntryType::Full),
            2 => Some(EntryType::First),
            3 => Some(EntryType::Middle),
            4 => Some(EntryType::Last),
            _ => None,
        }
    }
}

/// WAL entry header
#[derive(Debug, Clone)]
struct EntryHeader {
    /// CRC32C checksum of data
    crc: u32,
    /// Entry type
    entry_type: EntryType,
    /// Data length
    length: u32,
}

impl EntryHeader {
    fn encode(&self) -> [u8; 8] {
        let mut buf = [0u8; 8];
        buf[0..4].copy_from_slice(&self.crc.to_le_bytes());
        buf[4] = self.entry_type as u8;
        buf[5..8].copy_from_slice(&self.length.to_le_bytes()[0..3]);
        buf
    }

    fn decode(data: &[u8]) -> Result<Self> {
        if data.len() < 8 {
            return Err(RTDBError::Storage(
                "Invalid header size".to_string()
            ));
        }

        let crc = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let entry_type = EntryType::from_u8(data[4])
            .ok_or_else(|| RTDBError::Storage(
                format!("Invalid entry type: {}", data[4])
            ))?;
        let length = u32::from_le_bytes([data[5], data[6], data[7], 0]);

        Ok(EntryHeader {
            crc,
            entry_type,
            length,
        })
    }
}

/// WAL entry
#[derive(Debug, Clone)]
pub struct WALEntry {
    /// Entry data
    pub data: Vec<u8>,
}

/// Write-Ahead Log
pub struct WAL {
    /// Current log file
    current_file: File,
    /// Log directory
    path: PathBuf,
    /// Current file number
    current_file_no: u64,
    /// Max file size
    max_file_size: u64,
    /// Current file size
    current_size: u64,
    /// Write buffer
    buffer: BytesMut,
}

impl WAL {
    /// Create or open WAL at path
    pub fn open(path: impl AsRef<Path>, max_file_size: u64) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        std::fs::create_dir_all(&path).map_err(into_storage_error)?;

        // Find the latest WAL file
        let mut max_file_no: u64 = 0;
        for entry in std::fs::read_dir(&path).map_err(into_storage_error)? {
            let entry = entry.map_err(into_storage_error)?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            
            if name.starts_with("wal-") && name.ends_with(".log") {
                let num_str = &name[4..name.len()-4];
                if let Ok(num) = num_str.parse::<u64>() {
                    max_file_no = max_file_no.max(num);
                }
            }
        }

        // Open or create the WAL file
        let file_path = path.join(format!("wal-{:08}.log", max_file_no));
        let (file, current_size) = if file_path.exists() {
            let mut file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&file_path)
                .map_err(into_storage_error)?;
            
            let size = file.seek(SeekFrom::End(0))
                .map_err(into_storage_error)?;
            
            (file, size)
        } else {
            let mut file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .open(&file_path)
                .map_err(into_storage_error)?;
            
            // Write WAL header
            let mut header = Vec::new();
            header.put_u32_le(WAL_MAGIC);
            header.put_u16_le(WAL_VERSION);
            header.put_u64_le(0); // Reserved
            
            file.write_all(&header).map_err(into_storage_error)?;
            file.sync_all().map_err(into_storage_error)?;
            
            (file, HEADER_SIZE as u64)
        };

        Ok(WAL {
            current_file: file,
            path,
            current_file_no: max_file_no,
            max_file_size,
            current_size,
            buffer: BytesMut::with_capacity(64 * 1024),
        })
    }

    /// Append entry to WAL
    pub fn append(&mut self, data: &[u8]) -> Result<u64> {
        if data.len() > MAX_RECORD_SIZE {
            return Err(RTDBError::Storage(
                format!("Record too large: {} > {}", data.len(), MAX_RECORD_SIZE)
            ));
        }

        // Check if we need to rotate
        let record_size = 8 + data.len(); // header + data
        if self.current_size + record_size as u64 > self.max_file_size {
            self.rotate()?;
        }

        // Calculate position before write
        let position = self.current_file_no * self.max_file_size + self.current_size;

        // Build entry
        let crc = crc32c(data);
        let header = EntryHeader {
            crc,
            entry_type: EntryType::Full,
            length: data.len() as u32,
        };

        // Write to file directly (for durability)
        self.current_file.write_all(&header.encode())
            .map_err(into_storage_error)?;
        self.current_file.write_all(data)
            .map_err(into_storage_error)?;
        
        // Sync to disk
        self.current_file.sync_all().map_err(into_storage_error)?;

        self.current_size += 8 + data.len() as u64;

        Ok(position)
    }

    /// Batch append multiple entries
    pub fn append_batch(&mut self, entries: &[Vec<u8>]) -> Result<Vec<u64>> {
        let mut positions = Vec::with_capacity(entries.len());

        for entry in entries {
            let pos = self.append(entry)?;
            positions.push(pos);
        }

        Ok(positions)
    }

    /// Rotate to new WAL file
    fn rotate(&mut self) -> Result<()> {
        // Sync current file
        self.current_file.sync_all().map_err(into_storage_error)?;

        // Create new file
        self.current_file_no += 1;
        let new_path = self.path.join(format!("wal-{:08}.log", self.current_file_no));
        
        let mut new_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&new_path)
            .map_err(into_storage_error)?;

        // Write header
        let mut header = Vec::new();
        header.put_u32_le(WAL_MAGIC);
        header.put_u16_le(WAL_VERSION);
        header.put_u64_le(0); // Reserved
        
        new_file.write_all(&header).map_err(into_storage_error)?;
        new_file.sync_all().map_err(into_storage_error)?;

        self.current_file = new_file;
        self.current_size = HEADER_SIZE as u64;

        Ok(())
    }

    /// Read entries from WAL
    pub fn read_entries(&self) -> Result<Vec<WALEntry>> {
        let mut entries = Vec::new();

        // Read all WAL files
        let mut file_nos: Vec<u64> = Vec::new();
        for entry in std::fs::read_dir(&self.path).map_err(into_storage_error)? {
            let entry = entry.map_err(into_storage_error)?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            
            if name.starts_with("wal-") && name.ends_with(".log") {
                let num_str = &name[4..name.len()-4];
                if let Ok(num) = num_str.parse::<u64>() {
                    file_nos.push(num);
                }
            }
        }

        file_nos.sort_unstable();

        for file_no in file_nos {
            let file_path = self.path.join(format!("wal-{:08}.log", file_no));
            let file_entries = Self::read_file(&file_path)?;
            entries.extend(file_entries);
        }

        Ok(entries)
    }

    /// Read entries from a single WAL file
    fn read_file(path: &Path) -> Result<Vec<WALEntry>> {
        let mut file = File::open(path).map_err(into_storage_error)?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).map_err(into_storage_error)?;

        if buf.len() < HEADER_SIZE {
            return Ok(Vec::new());
        }

        // Verify magic
        let magic = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
        if magic != WAL_MAGIC {
            return Err(RTDBError::Storage(
                format!("Invalid WAL magic: {:08x}", magic)
            ));
        }

        let mut entries = Vec::new();
        let mut offset = HEADER_SIZE;

        while offset + 8 <= buf.len() {
            let header = EntryHeader::decode(&buf[offset..offset+8])?;
            let data_start = offset + 8;
            let data_end = data_start + header.length as usize;

            if data_end > buf.len() {
                // Truncated entry, stop here
                break;
            }

            let data = &buf[data_start..data_end];

            // Verify checksum
            let computed_crc = crc32c(data);
            if computed_crc != header.crc {
                return Err(RTDBError::Storage(
                    format!("CRC mismatch at offset {}", offset)
                ));
            }

            entries.push(WALEntry {
                data: data.to_vec(),
            });

            offset = data_end;
        }

        Ok(entries)
    }

    /// Sync WAL to disk
    pub fn sync(&mut self) -> Result<()> {
        self.current_file.sync_all().map_err(into_storage_error)
    }

    /// Get current WAL size
    pub fn size(&self) -> u64 {
        self.current_file_no * self.max_file_size + self.current_size
    }

    /// Flush and close WAL
    pub fn close(mut self) -> Result<()> {
        self.current_file.sync_all().map_err(into_storage_error)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_wal_append_and_read() {
        let temp_dir = TempDir::new().unwrap();
        let mut wal = WAL::open(temp_dir.path(), 1024 * 1024).unwrap();

        // Append entries
        let data1 = b"Hello, WAL!".to_vec();
        let data2 = b"Second entry".to_vec();

        wal.append(&data1).unwrap();
        wal.append(&data2).unwrap();

        // Read back
        let entries = wal.read_entries().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].data, data1);
        assert_eq!(entries[1].data, data2);
    }

    #[test]
    fn test_wal_rotation() {
        let temp_dir = TempDir::new().unwrap();
        let mut wal = WAL::open(temp_dir.path(), 100).unwrap();

        // Write enough data to trigger rotation
        for i in 0..10 {
            let data = format!("Entry {}", i);
            wal.append(data.as_bytes()).unwrap();
        }

        // Should have created multiple files
        let entries = wal.read_entries().unwrap();
        assert_eq!(entries.len(), 10);
    }

    #[test]
    fn test_wal_persists() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().to_path_buf();

        // Write
        {
            let mut wal = WAL::open(&path, 1024 * 1024).unwrap();
            wal.append(b"test data").unwrap();
        }

        // Read
        {
            let wal = WAL::open(&path, 1024 * 1024).unwrap();
            let entries = wal.read_entries().unwrap();
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].data, b"test data");
        }
    }
}
