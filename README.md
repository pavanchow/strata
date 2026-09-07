<img src="docs/logo.svg" alt="Strata logo" width="96">

**Strata is a file system in Rust that lives entirely inside one container file.**

A superblock, a bitmap block allocator, fixed-size inodes with direct block pointers, and directory entries mapping names to inodes, all inside a single image file you can open, read end to end, and understand in one sitting.

File systems feel like magic from the outside. Strata is small enough that it isn't. Format an image, make directories, write and read files, and every byte of that lives in one file on your disk, laid out the same way a real file system lays out a disk partition, just at a scale you can hold in your head.

## What it is

- A block device abstraction backed by a single file, with every read and write bounds-checked before it touches disk.
- A superblock that stores the layout of the image and is validated on every open: wrong magic, an inconsistent region layout, or a corrupt field is rejected with a typed error, never a panic.
- A bitmap allocator, one bit per block, that hands out and reclaims data blocks.
- Fixed-size inodes holding a type, a size, and direct block pointers, no indirect blocks, so the maximum file size is a direct, explicit consequence of the design.
- Directory entries, fixed-size records mapping a name to an inode number, stored in a directory's own data blocks.
- Full path resolution (`/a/b/c`), walking one directory at a time from the root.

## Usage

```
strata --img fs.img format --blocks 4096 --inodes 512
strata --img fs.img mkdir /docs
strata --img fs.img put ./notes.txt /docs/notes.txt
strata --img fs.img ls /docs
strata --img fs.img get /docs/notes.txt ./notes-copy.txt
strata --img fs.img rm /docs/notes.txt
```

Everything written to the image persists across reopening it, the same as a real disk.

## Build and test

```
cargo build
cargo test
```

## Robustness

Strata is written to reject bad input rather than crash on it. A corrupt or wrong-magic superblock comes back as a typed error. The number of blocks and the maximum file size are both capped, and no buffer is ever sized from an on-disk length field without bounding it against those caps first.

See `DESIGN.md` for the on-disk layout in detail.

By Pavan Nallamothu.
