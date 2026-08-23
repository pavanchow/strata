use std::fmt;

/// Every failure mode the file system can hit, typed so callers can match on it
/// instead of parsing a string. Nothing in this crate panics on bad input or a
/// corrupt image; a bad path always comes back as one of these.
#[derive(Debug)]
pub enum FsError {
    Io(std::io::Error),
    BadMagic,
    CorruptSuperblock(&'static str),
    UnsupportedBlockSize,
    TooManyBlocks,
    ImageTooSmall,
    OutOfBounds,
    NoSpace,
    NoFreeInode,
    NotFound,
    AlreadyExists,
    NotADirectory,
    IsADirectory,
    NotEmpty,
    NameTooLong,
    InvalidPath,
    FileTooLarge,
}

impl fmt::Display for FsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FsError::Io(e) => write!(f, "io error: {e}"),
            FsError::BadMagic => write!(f, "bad superblock magic, not a strata image"),
            FsError::CorruptSuperblock(why) => write!(f, "corrupt superblock: {why}"),
            FsError::UnsupportedBlockSize => write!(f, "unsupported block size"),
            FsError::TooManyBlocks => write!(f, "block count exceeds the supported cap"),
            FsError::ImageTooSmall => write!(f, "image file is smaller than the superblock claims"),
            FsError::OutOfBounds => write!(f, "block index out of bounds"),
            FsError::NoSpace => write!(f, "no free blocks left"),
            FsError::NoFreeInode => write!(f, "no free inodes left"),
            FsError::NotFound => write!(f, "no such file or directory"),
            FsError::AlreadyExists => write!(f, "already exists"),
            FsError::NotADirectory => write!(f, "not a directory"),
            FsError::IsADirectory => write!(f, "is a directory"),
            FsError::NotEmpty => write!(f, "directory not empty"),
            FsError::NameTooLong => write!(f, "name too long"),
            FsError::InvalidPath => write!(f, "invalid path"),
            FsError::FileTooLarge => write!(f, "file exceeds the maximum size"),
        }
    }
}

impl std::error::Error for FsError {}

impl From<std::io::Error> for FsError {
    fn from(e: std::io::Error) -> Self {
        FsError::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, FsError>;
