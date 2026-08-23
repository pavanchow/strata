//! Constants that define the on-disk layout. Changing any of these changes the
//! image format, so they live in one place.

/// Fixed block size. Strata only ever reads and writes whole blocks.
pub const BLOCK_SIZE: u32 = 4096;

/// Magic bytes stamped into every superblock: "STRA" in ASCII.
pub const MAGIC: u32 = 0x53_54_52_41;

/// Hard cap on the number of blocks an image may declare. This exists so that
/// a corrupt or hostile superblock can never make the allocator or bitmap
/// reader try to size a buffer from an attacker-controlled number. 1,048,576
/// blocks at 4 KiB each is a 4 GiB image, which is already generous for a
/// teaching file system.
pub const MAX_BLOCKS: u32 = 1_048_576;

/// Hard cap on the number of inodes an image may declare, for the same
/// reason: never trust an on-disk length field to size an allocation.
pub const MAX_INODES: u32 = 65_536;

/// Direct block pointers per inode. No indirect blocks, this is a from-scratch
/// teaching file system, not a production one, so the size limit is a direct
/// consequence of the design and is enforced explicitly.
pub const NUM_DIRECT: usize = 64;

/// Largest a single file may be: every direct pointer, completely full.
pub const MAX_FILE_SIZE: u64 = NUM_DIRECT as u64 * BLOCK_SIZE as u64;

/// Longest a path component (file or directory name) may be.
pub const MAX_NAME: usize = 60;

pub const INODE_KIND_FREE: u8 = 0;
pub const INODE_KIND_FILE: u8 = 1;
pub const INODE_KIND_DIR: u8 = 2;

/// Inode 0 is always the root directory.
pub const ROOT_INODE: u32 = 0;
