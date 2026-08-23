use crate::error::{FsError, Result};
use crate::layout::BLOCK_SIZE;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};

/// A block device backed by one regular file. Every read and write is
/// bounds-checked against `total_blocks` before it touches the file, so a
/// corrupt on-disk pointer can never turn into an out-of-range seek.
pub struct BlockDevice {
    file: File,
    total_blocks: u32,
}

impl BlockDevice {
    pub fn new(file: File, total_blocks: u32) -> Self {
        BlockDevice { file, total_blocks }
    }

    pub fn total_blocks(&self) -> u32 {
        self.total_blocks
    }

    /// Consumes the device and returns the underlying file, used when
    /// `open` needs to re-wrap the same handle once the real block count is
    /// known from the validated superblock.
    pub(crate) fn into_file(self) -> File {
        self.file
    }

    fn check_block(&self, block: u32) -> Result<()> {
        if block >= self.total_blocks {
            return Err(FsError::OutOfBounds);
        }
        Ok(())
    }

    pub fn read_block(&mut self, block: u32) -> Result<Vec<u8>> {
        self.check_block(block)?;
        let mut buf = vec![0u8; BLOCK_SIZE as usize];
        let offset = block as u64 * BLOCK_SIZE as u64;
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.read_exact(&mut buf)?;
        Ok(buf)
    }

    pub fn write_block(&mut self, block: u32, data: &[u8]) -> Result<()> {
        self.check_block(block)?;
        if data.len() != BLOCK_SIZE as usize {
            return Err(FsError::OutOfBounds);
        }
        let offset = block as u64 * BLOCK_SIZE as u64;
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.write_all(data)?;
        Ok(())
    }

    pub fn flush(&mut self) -> Result<()> {
        self.file.flush()?;
        Ok(())
    }

    fn total_bytes(&self) -> u64 {
        self.total_blocks as u64 * BLOCK_SIZE as u64
    }

    /// Read an arbitrary byte range, bounds-checked against the declared
    /// image size before any seek happens. Used for records (inodes,
    /// directory entries) that are smaller than a block and may straddle a
    /// block boundary.
    pub fn read_at(&mut self, offset: u64, len: usize) -> Result<Vec<u8>> {
        let end = offset
            .checked_add(len as u64)
            .ok_or(FsError::OutOfBounds)?;
        if end > self.total_bytes() {
            return Err(FsError::OutOfBounds);
        }
        let mut buf = vec![0u8; len];
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.read_exact(&mut buf)?;
        Ok(buf)
    }

    pub fn write_at(&mut self, offset: u64, data: &[u8]) -> Result<()> {
        let end = offset
            .checked_add(data.len() as u64)
            .ok_or(FsError::OutOfBounds)?;
        if end > self.total_bytes() {
            return Err(FsError::OutOfBounds);
        }
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.write_all(data)?;
        Ok(())
    }
}
