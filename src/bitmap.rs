use crate::device::BlockDevice;
use crate::error::{FsError, Result};
use crate::layout::BLOCK_SIZE;

/// One bit per block in the whole image, including the superblock, the
/// bitmap itself, and the inode table. Metadata blocks are marked used at
/// format time and are never handed out by `alloc`.
pub struct Bitmap {
    bits: Vec<u8>,
    bitmap_start: u32,
    bitmap_blocks: u32,
    data_start: u32,
    total_blocks: u32,
}

impl Bitmap {
    pub fn load(
        device: &mut BlockDevice,
        bitmap_start: u32,
        bitmap_blocks: u32,
        data_start: u32,
        total_blocks: u32,
    ) -> Result<Self> {
        let mut bits = Vec::with_capacity(bitmap_blocks as usize * BLOCK_SIZE as usize);
        for i in 0..bitmap_blocks {
            bits.extend_from_slice(&device.read_block(bitmap_start + i)?);
        }
        Ok(Bitmap {
            bits,
            bitmap_start,
            bitmap_blocks,
            data_start,
            total_blocks,
        })
    }

    fn get(&self, block: u32) -> bool {
        let byte = (block / 8) as usize;
        let bit = block % 8;
        byte < self.bits.len() && (self.bits[byte] & (1 << bit)) != 0
    }

    fn set(&mut self, block: u32, used: bool) {
        let byte = (block / 8) as usize;
        let bit = block % 8;
        if byte >= self.bits.len() {
            return;
        }
        if used {
            self.bits[byte] |= 1 << bit;
        } else {
            self.bits[byte] &= !(1 << bit);
        }
    }

    /// Mark every metadata block (superblock, bitmap, inode table) used so
    /// the allocator can never hand one out. Called once at format time.
    pub fn reserve_metadata(&mut self) {
        for b in 0..self.data_start {
            self.set(b, true);
        }
    }

    pub fn alloc(&mut self) -> Result<u32> {
        for block in self.data_start..self.total_blocks {
            if !self.get(block) {
                self.set(block, true);
                return Ok(block);
            }
        }
        Err(FsError::NoSpace)
    }

    pub fn free(&mut self, block: u32) -> Result<()> {
        if block < self.data_start || block >= self.total_blocks {
            return Err(FsError::OutOfBounds);
        }
        self.set(block, false);
        Ok(())
    }

    pub fn is_used(&self, block: u32) -> bool {
        self.get(block)
    }

    pub fn used_data_blocks(&self) -> u32 {
        (self.data_start..self.total_blocks)
            .filter(|&b| self.get(b))
            .count() as u32
    }

    pub fn free_data_blocks(&self) -> u32 {
        self.total_blocks - self.data_start - self.used_data_blocks()
    }

    pub fn flush(&self, device: &mut BlockDevice) -> Result<()> {
        for i in 0..self.bitmap_blocks {
            let start = i as usize * BLOCK_SIZE as usize;
            let end = start + BLOCK_SIZE as usize;
            device.write_block(self.bitmap_start + i, &self.bits[start..end])?;
        }
        Ok(())
    }
}
