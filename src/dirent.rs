use crate::error::{FsError, Result};
use crate::layout::MAX_NAME;

/// A fixed-size directory entry: an inode number, a used flag, a name
/// length, and a name buffer. Directory contents are just a flat array of
/// these records packed into the directory's data blocks.
#[derive(Debug, Clone, Copy)]
pub struct DirEntry {
    pub inode: u32,
    pub used: bool,
    pub name_len: u8,
    pub name: [u8; MAX_NAME],
}

/// inode(4) + used(1) + name_len(1) + name(MAX_NAME)
pub const DIRENT_SIZE: usize = 4 + 1 + 1 + MAX_NAME;

impl DirEntry {
    pub fn empty() -> Self {
        DirEntry {
            inode: 0,
            used: false,
            name_len: 0,
            name: [0; MAX_NAME],
        }
    }

    pub fn new(inode: u32, name: &str) -> Result<Self> {
        let bytes = name.as_bytes();
        if bytes.is_empty() || bytes.len() > MAX_NAME {
            return Err(FsError::NameTooLong);
        }
        let mut buf = [0u8; MAX_NAME];
        buf[..bytes.len()].copy_from_slice(bytes);
        Ok(DirEntry {
            inode,
            used: true,
            name_len: bytes.len() as u8,
            name: buf,
        })
    }

    /// Reads the name back as a `&str`. The name length is always clamped to
    /// `MAX_NAME` by construction (`new` rejects anything longer, and this
    /// is the only way to build a `DirEntry` with `used = true`), so this
    /// never slices past the fixed buffer even from a corrupt-but-parseable
    /// record.
    pub fn name_str(&self) -> &str {
        let len = (self.name_len as usize).min(MAX_NAME);
        std::str::from_utf8(&self.name[..len]).unwrap_or("")
    }

    pub fn to_bytes(&self) -> [u8; DIRENT_SIZE] {
        let mut buf = [0u8; DIRENT_SIZE];
        buf[0..4].copy_from_slice(&self.inode.to_le_bytes());
        buf[4] = self.used as u8;
        buf[5] = self.name_len;
        buf[6..6 + MAX_NAME].copy_from_slice(&self.name);
        buf
    }

    pub fn from_bytes(buf: &[u8]) -> Result<Self> {
        if buf.len() != DIRENT_SIZE {
            return Err(FsError::CorruptSuperblock("short dirent record"));
        }
        let inode = u32::from_le_bytes(buf[0..4].try_into().unwrap());
        let used = buf[4] != 0;
        let name_len = buf[5].min(MAX_NAME as u8);
        let mut name = [0u8; MAX_NAME];
        name.copy_from_slice(&buf[6..6 + MAX_NAME]);
        Ok(DirEntry {
            inode,
            used,
            name_len,
            name,
        })
    }
}
