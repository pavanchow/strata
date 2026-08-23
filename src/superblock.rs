use crate::error::{FsError, Result};
use crate::inode::INODE_SIZE;
use crate::layout::{BLOCK_SIZE, MAGIC, MAX_BLOCKS, MAX_INODES};

/// The first block of every image. Everything else in the file system is
/// found by walking the offsets stored here, so this block is validated hard
/// before anything else is trusted.
#[derive(Debug, Clone, Copy)]
pub struct SuperBlock {
    pub magic: u32,
    pub block_size: u32,
    pub total_blocks: u32,
    pub inode_count: u32,
    pub bitmap_start: u32,
    pub bitmap_blocks: u32,
    pub inode_table_start: u32,
    pub inode_table_blocks: u32,
    pub data_start: u32,
    pub root_inode: u32,
}

impl SuperBlock {
    pub fn to_bytes(&self) -> [u8; BLOCK_SIZE as usize] {
        let mut buf = [0u8; BLOCK_SIZE as usize];
        let fields = [
            self.magic,
            self.block_size,
            self.total_blocks,
            self.inode_count,
            self.bitmap_start,
            self.bitmap_blocks,
            self.inode_table_start,
            self.inode_table_blocks,
            self.data_start,
            self.root_inode,
        ];
        for (i, v) in fields.iter().enumerate() {
            buf[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        buf
    }

    /// Parse and validate a superblock from a raw block. This is the single
    /// choke point that decides whether an image is trustworthy. A wrong
    /// magic number, an inconsistent layout, or a block/inode count above the
    /// hard caps is rejected here with a typed error, never a panic and never
    /// a silent pass-through of an attacker-controlled length field.
    pub fn from_bytes(buf: &[u8]) -> Result<Self> {
        if buf.len() != BLOCK_SIZE as usize {
            return Err(FsError::CorruptSuperblock("short block"));
        }
        let read_u32 = |i: usize| -> u32 {
            u32::from_le_bytes([buf[i * 4], buf[i * 4 + 1], buf[i * 4 + 2], buf[i * 4 + 3]])
        };
        let sb = SuperBlock {
            magic: read_u32(0),
            block_size: read_u32(1),
            total_blocks: read_u32(2),
            inode_count: read_u32(3),
            bitmap_start: read_u32(4),
            bitmap_blocks: read_u32(5),
            inode_table_start: read_u32(6),
            inode_table_blocks: read_u32(7),
            data_start: read_u32(8),
            root_inode: read_u32(9),
        };
        sb.validate()?;
        Ok(sb)
    }

    fn validate(&self) -> Result<()> {
        if self.magic != MAGIC {
            return Err(FsError::BadMagic);
        }
        if self.block_size != BLOCK_SIZE {
            return Err(FsError::UnsupportedBlockSize);
        }
        if self.total_blocks == 0 || self.total_blocks > MAX_BLOCKS {
            return Err(FsError::CorruptSuperblock("total_blocks out of range"));
        }
        if self.inode_count == 0 || self.inode_count > MAX_INODES {
            return Err(FsError::CorruptSuperblock("inode_count out of range"));
        }
        // Every region must be laid out in order and fit inside total_blocks,
        // using checked arithmetic so a hostile pair of fields can't wrap
        // around and pass a naive comparison.
        let bitmap_end = self
            .bitmap_start
            .checked_add(self.bitmap_blocks)
            .ok_or(FsError::CorruptSuperblock("bitmap region overflows"))?;
        if self.bitmap_start == 0 || bitmap_end > self.total_blocks {
            return Err(FsError::CorruptSuperblock("bitmap region out of range"));
        }
        let inode_table_end = self
            .inode_table_start
            .checked_add(self.inode_table_blocks)
            .ok_or(FsError::CorruptSuperblock("inode table region overflows"))?;
        if self.inode_table_start < bitmap_end || inode_table_end > self.total_blocks {
            return Err(FsError::CorruptSuperblock("inode table region out of range"));
        }
        if self.data_start < inode_table_end || self.data_start >= self.total_blocks {
            return Err(FsError::CorruptSuperblock("data region out of range"));
        }
        // The inode table must actually be big enough to hold inode_count
        // records. Never trust inode_count on its own to index into the
        // table without checking it against the space reserved for it.
        let inode_table_capacity_bytes = (self.inode_table_blocks as u64) * BLOCK_SIZE as u64;
        let inode_table_needed_bytes = (self.inode_count as u64) * (INODE_SIZE as u64);
        if inode_table_needed_bytes > inode_table_capacity_bytes {
            return Err(FsError::CorruptSuperblock("inode table too small for inode_count"));
        }
        if self.root_inode >= self.inode_count {
            return Err(FsError::CorruptSuperblock("root inode out of range"));
        }
        Ok(())
    }
}
