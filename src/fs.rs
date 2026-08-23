use crate::bitmap::Bitmap;
use crate::device::BlockDevice;
use crate::dirent::{DirEntry, DIRENT_SIZE};
use crate::error::{FsError, Result};
use crate::inode::{Inode, INODE_SIZE};
use crate::layout::{
    BLOCK_SIZE, MAGIC, MAX_BLOCKS, MAX_FILE_SIZE, MAX_INODES, MAX_NAME, NUM_DIRECT, ROOT_INODE,
};
use crate::superblock::SuperBlock;
use std::fs::{File, OpenOptions};
use std::path::Path;

/// What a directory listing entry describes.
#[derive(Debug, Clone)]
pub struct ListEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

/// A mounted Strata image: superblock, allocator, and the underlying device,
/// all validated on open. Every public method resolves paths against this
/// state and never trusts an on-disk value without a bounds check first.
pub struct Strata {
    device: BlockDevice,
    sb: SuperBlock,
    bitmap: Bitmap,
}

impl Strata {
    /// Create a brand new image at `path` sized to hold `total_blocks`
    /// blocks and `inode_count` inodes, and format it: superblock, an empty
    /// bitmap with metadata reserved, an empty inode table, and a root
    /// directory.
    pub fn format<P: AsRef<Path>>(path: P, total_blocks: u32, inode_count: u32) -> Result<Self> {
        if total_blocks == 0 || total_blocks > MAX_BLOCKS {
            return Err(FsError::TooManyBlocks);
        }
        if inode_count == 0 || inode_count > MAX_INODES {
            return Err(FsError::CorruptSuperblock("inode_count out of range"));
        }

        let bitmap_bits_needed = total_blocks as u64;
        let bitmap_blocks = bitmap_bits_needed.div_ceil(8).div_ceil(BLOCK_SIZE as u64) as u32;
        let bitmap_blocks = bitmap_blocks.max(1);

        let inode_table_bytes = inode_count as u64 * INODE_SIZE as u64;
        let inode_table_blocks = inode_table_bytes.div_ceil(BLOCK_SIZE as u64) as u32;
        let inode_table_blocks = inode_table_blocks.max(1);

        let bitmap_start = 1u32;
        let inode_table_start = bitmap_start + bitmap_blocks;
        let data_start = inode_table_start + inode_table_blocks;

        if data_start >= total_blocks {
            return Err(FsError::TooManyBlocks);
        }

        let sb = SuperBlock {
            magic: MAGIC,
            block_size: BLOCK_SIZE,
            total_blocks,
            inode_count,
            bitmap_start,
            bitmap_blocks,
            inode_table_start,
            inode_table_blocks,
            data_start,
            root_inode: ROOT_INODE,
        };

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;
        file.set_len(total_blocks as u64 * BLOCK_SIZE as u64)?;

        let mut device = BlockDevice::new(file, total_blocks);
        device.write_block(0, &sb.to_bytes())?;

        let zero_block = vec![0u8; BLOCK_SIZE as usize];
        for b in bitmap_start..data_start {
            device.write_block(b, &zero_block)?;
        }

        let mut bitmap = Bitmap::load(&mut device, bitmap_start, bitmap_blocks, data_start, total_blocks)?;
        bitmap.reserve_metadata();
        bitmap.flush(&mut device)?;

        let mut fs = Strata { device, sb, bitmap };
        fs.write_inode(ROOT_INODE, &Inode::new_dir())?;
        fs.device.flush()?;
        Ok(fs)
    }

    /// Open an existing image, validating the superblock before trusting
    /// anything else in the file. A wrong-magic or inconsistent superblock
    /// comes back as a typed error, never a panic.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file: File = OpenOptions::new().read(true).write(true).open(path)?;
        let file_len = file.metadata()?.len();

        // Read the superblock's raw bytes directly; we don't yet know the
        // declared block count so we can't use BlockDevice's bounds check.
        // The file must at least hold one full block for this to be valid.
        if file_len < BLOCK_SIZE as u64 {
            return Err(FsError::ImageTooSmall);
        }
        let mut probe = BlockDevice::new(file, (file_len / BLOCK_SIZE as u64) as u32);
        let raw = probe.read_block(0)?;
        let sb = SuperBlock::from_bytes(&raw)?;

        // The superblock claims total_blocks; the backing file must actually
        // be that large before we trust any offset computed from it.
        let required_len = sb.total_blocks as u64 * sb.block_size as u64;
        if file_len < required_len {
            return Err(FsError::ImageTooSmall);
        }

        let mut device = BlockDevice::new(probe.into_file(), sb.total_blocks);
        let bitmap = Bitmap::load(
            &mut device,
            sb.bitmap_start,
            sb.bitmap_blocks,
            sb.data_start,
            sb.total_blocks,
        )?;

        Ok(Strata { device, sb, bitmap })
    }

    // ---- inode table ----

    fn inode_offset(&self, idx: u32) -> Result<u64> {
        if idx >= self.sb.inode_count {
            return Err(FsError::OutOfBounds);
        }
        Ok(self.sb.inode_table_start as u64 * BLOCK_SIZE as u64 + idx as u64 * INODE_SIZE as u64)
    }

    fn read_inode(&mut self, idx: u32) -> Result<Inode> {
        let off = self.inode_offset(idx)?;
        let buf = self.device.read_at(off, INODE_SIZE)?;
        Inode::from_bytes(&buf)
    }

    fn write_inode(&mut self, idx: u32, inode: &Inode) -> Result<()> {
        let off = self.inode_offset(idx)?;
        self.device.write_at(off, &inode.to_bytes())
    }

    fn alloc_inode(&mut self) -> Result<u32> {
        for idx in 0..self.sb.inode_count {
            if self.read_inode(idx)?.is_free() {
                return Ok(idx);
            }
        }
        Err(FsError::NoFreeInode)
    }

    /// Free every data block owned by an inode, then mark the inode free.
    fn free_inode(&mut self, idx: u32) -> Result<()> {
        let inode = self.read_inode(idx)?;
        let used = inode.block_count(BLOCK_SIZE);
        for &b in inode.direct.iter().take(used) {
            if b != 0 {
                self.bitmap.free(b)?;
            }
        }
        self.write_inode(idx, &Inode::free())?;
        self.bitmap.flush(&mut self.device)
    }

    // ---- directory contents ----

    fn read_dir_entries(&mut self, dir_inode: u32) -> Result<Vec<DirEntry>> {
        let inode = self.read_inode(dir_inode)?;
        let block_count = inode.block_count(BLOCK_SIZE);
        let mut entries = Vec::new();
        for &b in inode.direct.iter().take(block_count) {
            if b == 0 {
                continue;
            }
            let data = self.device.read_block(b)?;
            for chunk in data.chunks(DIRENT_SIZE) {
                if chunk.len() < DIRENT_SIZE {
                    break;
                }
                let e = DirEntry::from_bytes(chunk)?;
                if e.used {
                    entries.push(e);
                }
            }
        }
        Ok(entries)
    }

    /// Append a directory entry, growing the directory by one block if every
    /// existing block is full and the directory still has a free direct
    /// pointer. Fails with `NoSpace` once `NUM_DIRECT` blocks are in use,
    /// same hard cap that bounds an ordinary file.
    fn add_dir_entry(&mut self, dir_inode_idx: u32, entry: DirEntry) -> Result<()> {
        let mut inode = self.read_inode(dir_inode_idx)?;
        let block_count = inode.block_count(BLOCK_SIZE);

        for &b in inode.direct.iter().take(block_count) {
            if b == 0 {
                continue;
            }
            let mut data = self.device.read_block(b)?;
            let mut slots = data.chunks_exact_mut(DIRENT_SIZE);
            for slot in &mut slots {
                let existing = DirEntry::from_bytes(slot)?;
                if !existing.used {
                    slot.copy_from_slice(&entry.to_bytes());
                    self.device.write_block(b, &data)?;
                    return Ok(());
                }
            }
        }

        // No free slot in an existing block, allocate a new one.
        if block_count >= NUM_DIRECT {
            return Err(FsError::NoSpace);
        }
        let new_block = self.bitmap.alloc()?;
        self.bitmap.flush(&mut self.device)?;

        let mut data = vec![0u8; BLOCK_SIZE as usize];
        data[0..DIRENT_SIZE].copy_from_slice(&entry.to_bytes());
        self.device.write_block(new_block, &data)?;

        inode.direct[block_count] = new_block;
        inode.size = (block_count as u64 + 1) * BLOCK_SIZE as u64;
        self.write_inode(dir_inode_idx, &inode)?;
        Ok(())
    }

    fn remove_dir_entry(&mut self, dir_inode_idx: u32, name: &str) -> Result<u32> {
        let inode = self.read_inode(dir_inode_idx)?;
        let block_count = inode.block_count(BLOCK_SIZE);
        for &b in inode.direct.iter().take(block_count) {
            if b == 0 {
                continue;
            }
            let mut data = self.device.read_block(b)?;
            let mut slots = data.chunks_exact_mut(DIRENT_SIZE);
            for slot in &mut slots {
                let existing = DirEntry::from_bytes(slot)?;
                if existing.used && existing.name_str() == name {
                    let found = existing.inode;
                    slot.copy_from_slice(&DirEntry::empty().to_bytes());
                    self.device.write_block(b, &data)?;
                    return Ok(found);
                }
            }
        }
        Err(FsError::NotFound)
    }

    fn lookup_in_dir(&mut self, dir_inode: u32, name: &str) -> Result<Option<u32>> {
        for e in self.read_dir_entries(dir_inode)? {
            if e.name_str() == name {
                return Ok(Some(e.inode));
            }
        }
        Ok(None)
    }

    // ---- path resolution ----

    fn split_path(path: &str) -> Result<Vec<&str>> {
        if !path.starts_with('/') {
            return Err(FsError::InvalidPath);
        }
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        for p in &parts {
            if p.len() > MAX_NAME || *p == "." || *p == ".." {
                return Err(FsError::InvalidPath);
            }
        }
        Ok(parts)
    }

    /// Resolve a path to an inode number. `/` resolves to the root.
    fn resolve(&mut self, path: &str) -> Result<u32> {
        let parts = Self::split_path(path)?;
        let mut current = self.sb.root_inode;
        for part in parts {
            let inode = self.read_inode(current)?;
            if !inode.is_dir() {
                return Err(FsError::NotADirectory);
            }
            match self.lookup_in_dir(current, part)? {
                Some(next) => current = next,
                None => return Err(FsError::NotFound),
            }
        }
        Ok(current)
    }

    /// Resolve the parent directory and final component name of a path.
    fn resolve_parent<'a>(&mut self, path: &'a str) -> Result<(u32, &'a str)> {
        let parts = Self::split_path(path)?;
        let (name, dir_parts) = parts.split_last().ok_or(FsError::InvalidPath)?;
        let mut current = self.sb.root_inode;
        for part in dir_parts {
            let inode = self.read_inode(current)?;
            if !inode.is_dir() {
                return Err(FsError::NotADirectory);
            }
            match self.lookup_in_dir(current, part)? {
                Some(next) => current = next,
                None => return Err(FsError::NotFound),
            }
        }
        Ok((current, name))
    }

    // ---- public operations ----

    pub fn mkdir(&mut self, path: &str) -> Result<()> {
        let (parent, name) = self.resolve_parent(path)?;
        let parent_inode = self.read_inode(parent)?;
        if !parent_inode.is_dir() {
            return Err(FsError::NotADirectory);
        }
        if self.lookup_in_dir(parent, name)?.is_some() {
            return Err(FsError::AlreadyExists);
        }
        let new_idx = self.alloc_inode()?;
        self.write_inode(new_idx, &Inode::new_dir())?;
        let entry = DirEntry::new(new_idx, name)?;
        if let Err(e) = self.add_dir_entry(parent, entry) {
            self.write_inode(new_idx, &Inode::free())?;
            return Err(e);
        }
        self.device.flush()
    }

    pub fn write_file(&mut self, path: &str, contents: &[u8]) -> Result<()> {
        if contents.len() as u64 > MAX_FILE_SIZE {
            return Err(FsError::FileTooLarge);
        }
        let (parent, name) = self.resolve_parent(path)?;
        let parent_inode = self.read_inode(parent)?;
        if !parent_inode.is_dir() {
            return Err(FsError::NotADirectory);
        }

        let file_idx = match self.lookup_in_dir(parent, name)? {
            Some(existing) => {
                let existing_inode = self.read_inode(existing)?;
                if existing_inode.is_dir() {
                    return Err(FsError::IsADirectory);
                }
                // Overwrite in place: free the old blocks first.
                self.free_data_blocks_of(existing)?;
                existing
            }
            None => {
                let new_idx = self.alloc_inode()?;
                self.write_inode(new_idx, &Inode::new_file())?;
                let entry = DirEntry::new(new_idx, name)?;
                if let Err(e) = self.add_dir_entry(parent, entry) {
                    self.write_inode(new_idx, &Inode::free())?;
                    return Err(e);
                }
                new_idx
            }
        };

        let mut inode = self.read_inode(file_idx)?;
        let needed_blocks = contents.len().div_ceil(BLOCK_SIZE as usize).max(if contents.is_empty() { 0 } else { 1 });
        if needed_blocks > NUM_DIRECT {
            return Err(FsError::FileTooLarge);
        }

        for i in 0..needed_blocks {
            let block = self.bitmap.alloc()?;
            self.bitmap.flush(&mut self.device)?;
            let start = i * BLOCK_SIZE as usize;
            let end = (start + BLOCK_SIZE as usize).min(contents.len());
            let mut data = vec![0u8; BLOCK_SIZE as usize];
            data[..end - start].copy_from_slice(&contents[start..end]);
            self.device.write_block(block, &data)?;
            inode.direct[i] = block;
        }
        inode.size = contents.len() as u64;
        self.write_inode(file_idx, &inode)?;
        self.device.flush()
    }

    fn free_data_blocks_of(&mut self, idx: u32) -> Result<()> {
        let mut inode = self.read_inode(idx)?;
        let used = inode.block_count(BLOCK_SIZE);
        for slot in inode.direct.iter_mut().take(used) {
            if *slot != 0 {
                self.bitmap.free(*slot)?;
                *slot = 0;
            }
        }
        inode.size = 0;
        self.write_inode(idx, &inode)?;
        self.bitmap.flush(&mut self.device)
    }

    pub fn read_file(&mut self, path: &str) -> Result<Vec<u8>> {
        let idx = self.resolve(path)?;
        let inode = self.read_inode(idx)?;
        if inode.is_dir() {
            return Err(FsError::IsADirectory);
        }
        let block_count = inode.block_count(BLOCK_SIZE);
        let mut out = Vec::with_capacity(inode.size as usize);
        for &b in inode.direct.iter().take(block_count) {
            out.extend_from_slice(&self.device.read_block(b)?);
        }
        out.truncate(inode.size as usize);
        Ok(out)
    }

    pub fn list_dir(&mut self, path: &str) -> Result<Vec<ListEntry>> {
        let idx = self.resolve(path)?;
        let inode = self.read_inode(idx)?;
        if !inode.is_dir() {
            return Err(FsError::NotADirectory);
        }
        let mut out = Vec::new();
        for e in self.read_dir_entries(idx)? {
            let child = self.read_inode(e.inode)?;
            out.push(ListEntry {
                name: e.name_str().to_string(),
                is_dir: child.is_dir(),
                size: child.size,
            });
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    pub fn remove(&mut self, path: &str) -> Result<()> {
        let (parent, name) = self.resolve_parent(path)?;
        let target = self
            .lookup_in_dir(parent, name)?
            .ok_or(FsError::NotFound)?;
        let inode = self.read_inode(target)?;
        if inode.is_dir() && !self.read_dir_entries(target)?.is_empty() {
            return Err(FsError::NotEmpty);
        }
        self.remove_dir_entry(parent, name)?;
        self.free_inode(target)?;
        self.device.flush()
    }

    pub fn free_blocks(&self) -> u32 {
        self.bitmap.free_data_blocks()
    }

    pub fn used_blocks(&self) -> u32 {
        self.bitmap.used_data_blocks()
    }

    pub fn total_blocks(&self) -> u32 {
        self.sb.total_blocks
    }
}
