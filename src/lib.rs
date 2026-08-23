//! Strata: a from-scratch virtual file system living inside a single
//! container file. A superblock, a bitmap block allocator, fixed-size
//! inodes with direct block pointers, and directory entries mapping names
//! to inodes, all readable end to end.

pub mod bitmap;
pub mod device;
pub mod dirent;
pub mod error;
pub mod fs;
pub mod inode;
pub mod layout;
pub mod superblock;

pub use error::{FsError, Result};
pub use fs::{ListEntry, Strata};
