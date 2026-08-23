use crate::error::{FsError, Result};
use crate::layout::{INODE_KIND_DIR, INODE_KIND_FILE, INODE_KIND_FREE, NUM_DIRECT};

/// Fixed-size on-disk inode: a type tag, a byte size, and a flat array of
/// direct block pointers. No indirect blocks, the maximum file size is the
/// direct explicit consequence of `NUM_DIRECT` and is enforced in `fs.rs`.
#[derive(Debug, Clone, Copy)]
pub struct Inode {
    pub kind: u8,
    pub size: u64,
    pub direct: [u32; NUM_DIRECT],
}

/// kind(1) + size(8) + direct(4 * NUM_DIRECT)
pub const INODE_SIZE: usize = 1 + 8 + 4 * NUM_DIRECT;

impl Inode {
    pub fn free() -> Self {
        Inode {
            kind: INODE_KIND_FREE,
            size: 0,
            direct: [0; NUM_DIRECT],
        }
    }

    pub fn new_dir() -> Self {
        Inode {
            kind: INODE_KIND_DIR,
            size: 0,
            direct: [0; NUM_DIRECT],
        }
    }

    pub fn new_file() -> Self {
        Inode {
            kind: INODE_KIND_FILE,
            size: 0,
            direct: [0; NUM_DIRECT],
        }
    }

    pub fn is_free(&self) -> bool {
        self.kind == INODE_KIND_FREE
    }

    pub fn is_dir(&self) -> bool {
        self.kind == INODE_KIND_DIR
    }

    pub fn is_file(&self) -> bool {
        self.kind == INODE_KIND_FILE
    }

    /// Number of direct pointers currently holding a block, derived from
    /// size, never trusted as a raw stored field.
    pub fn block_count(&self, block_size: u32) -> usize {
        self.size.div_ceil(block_size as u64) as usize
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(INODE_SIZE);
        buf.push(self.kind);
        buf.extend_from_slice(&self.size.to_le_bytes());
        for p in &self.direct {
            buf.extend_from_slice(&p.to_le_bytes());
        }
        buf
    }

    /// Parse an inode from an exact `INODE_SIZE` byte slice. Bounds are
    /// checked by construction (the caller always hands us a slice of the
    /// right length carved out of a full block), so this never reads past
    /// the buffer regardless of what values are stored inside it.
    pub fn from_bytes(buf: &[u8]) -> Result<Self> {
        if buf.len() != INODE_SIZE {
            return Err(FsError::CorruptSuperblock("short inode record"));
        }
        let kind = buf[0];
        if kind != INODE_KIND_FREE && kind != INODE_KIND_FILE && kind != INODE_KIND_DIR {
            return Err(FsError::CorruptSuperblock("invalid inode kind"));
        }
        let size = u64::from_le_bytes(buf[1..9].try_into().unwrap());
        let mut direct = [0u32; NUM_DIRECT];
        for (i, d) in direct.iter_mut().enumerate() {
            let off = 9 + i * 4;
            *d = u32::from_le_bytes(buf[off..off + 4].try_into().unwrap());
        }
        Ok(Inode { kind, size, direct })
    }
}
