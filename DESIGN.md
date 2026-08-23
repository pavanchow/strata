# Strata design

Strata is a virtual file system that lives inside one regular file, called the image. This document walks through the on-disk layout from the first byte to path resolution.

## Block device

Everything is built on `BlockDevice`, a thin wrapper around a `File` that reads and writes fixed 4096-byte blocks by index. Every read and write checks the block index against the declared block count before it seeks, so a bad pointer anywhere above this layer turns into a typed `OutOfBounds` error instead of a wild seek into unrelated bytes or a panic.

## Superblock

Block 0 of the image is the superblock. It is the only block whose location is fixed by convention rather than computed, because everything else is found by reading offsets out of it. It stores:

- a magic number, so an unrelated file is rejected immediately
- the block size the image was formatted with
- the total number of blocks in the image
- the number of inodes reserved
- where the bitmap starts and how many blocks it occupies
- where the inode table starts and how many blocks it occupies
- where the data region starts
- the root directory's inode number

Opening an image parses this block and validates it before trusting anything else: the magic must match, the block size must be the one Strata supports, the block and inode counts must sit under hard caps, and every region (bitmap, inode table, data) must fit inside the image without overlapping or overflowing, checked with saturating arithmetic so a hostile pair of fields can't wrap around and slip past a naive comparison. Any failure here is a typed error, never a panic, and never a buffer sized from the untrusted field itself.

## Bitmap allocator

Immediately after the superblock sits the block bitmap, one bit per block in the entire image, including the metadata blocks. At format time every metadata block (the superblock, the bitmap itself, and the inode table) is marked used up front, so the allocator can never hand one of them back out as data.

Allocating a block is a linear scan for the first clear bit at or after the start of the data region. Freeing a block clears its bit. The whole bitmap is loaded into memory on open and flushed back to its blocks after every change, which keeps the allocator logic simple at the scale this project targets.

## Inodes

After the bitmap comes the inode table, a flat array of fixed-size records. Each inode holds:

- a kind: free, file, or directory
- a size in bytes
- 64 direct block pointers

There are no indirect blocks. That is a deliberate simplification: the maximum file size is exactly `64 * 4096` bytes, a fact that falls directly out of the fixed pointer array rather than being an afterthought, and it is checked explicitly before any write that would exceed it.

Reading or writing a specific inode computes a byte offset into the inode table region and reads or writes exactly `INODE_SIZE` bytes at that offset, after checking the inode index against the inode count from the (already validated) superblock.

## Directories

A directory is an inode like any other, whose data blocks hold not file contents but a flat array of fixed-size directory entries: an inode number, a used flag, a name length, and a name buffer. Looking up a name in a directory means reading every entry across every block the directory owns and comparing names. Adding an entry looks for an empty slot in an existing block first, and only allocates a new block, bounded by the same 64-direct-pointer limit as a file, when every existing block is full. Removing an entry just clears its slot in place; the directory's blocks are not compacted or shrunk, the same tradeoff most real file systems make.

## Path resolution

A path like `/a/b/c` is split on `/` into components, each of which is checked against the maximum name length. Resolution starts at the root inode and, for each component, reads the current directory's entries, looks up the next name, and moves to the inode it points to, failing immediately with a typed error if a component is missing or if something that isn't a directory appears where one is expected.

## What "corrupt" means here

Because every region boundary, every inode index, and every block index is checked against a value from the validated superblock before it is used, the failure mode for a damaged image is a returned error, not a panic and not a read past the end of the file. The block and inode caps mean an attacker-controlled superblock can never make the allocator or bitmap loader try to size an in-memory buffer arbitrarily large either.
