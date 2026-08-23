use strata::{FsError, Strata};

fn img_path() -> std::path::PathBuf {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.img");
    // Leak the tempdir so the file survives for the life of the test; the OS
    // cleans /tmp eventually and each test uses its own directory.
    std::mem::forget(dir);
    path
}

#[test]
fn format_creates_root_directory() {
    let path = img_path();
    let mut fs = Strata::format(&path, 512, 64).unwrap();
    let listing = fs.list_dir("/").unwrap();
    assert!(listing.is_empty());
}

#[test]
fn write_read_roundtrips_exact_bytes() {
    let path = img_path();
    let mut fs = Strata::format(&path, 512, 64).unwrap();
    let payload: Vec<u8> = (0..20000u32).map(|i| (i % 251) as u8).collect();
    fs.write_file("/hello.bin", &payload).unwrap();
    let back = fs.read_file("/hello.bin").unwrap();
    assert_eq!(back, payload);
}

#[test]
fn delete_frees_blocks_in_bitmap() {
    let path = img_path();
    let mut fs = Strata::format(&path, 512, 64).unwrap();

    // Warm up the root directory's own data block first, so the baseline
    // measurement below isn't skewed by the one-time cost of the directory
    // allocating storage for its first entry. Directories, like real file
    // systems, don't shrink back down when the last entry in a block is
    // removed, only the file's own data blocks do.
    fs.write_file("/warm.bin", b"x").unwrap();
    fs.remove("/warm.bin").unwrap();
    let free_before = fs.free_blocks();

    let payload = vec![0xABu8; 30000];
    fs.write_file("/big.bin", &payload).unwrap();
    let free_after_write = fs.free_blocks();
    assert!(free_after_write < free_before, "writing should consume free blocks");

    fs.remove("/big.bin").unwrap();
    let free_after_delete = fs.free_blocks();
    assert_eq!(
        free_after_delete, free_before,
        "deleting should return every block the file held"
    );
}

#[test]
fn nested_directories_and_path_resolution() {
    let path = img_path();
    let mut fs = Strata::format(&path, 512, 64).unwrap();
    fs.mkdir("/a").unwrap();
    fs.mkdir("/a/b").unwrap();
    fs.mkdir("/a/b/c").unwrap();
    fs.write_file("/a/b/c/leaf.txt", b"deep file").unwrap();

    let listing = fs.list_dir("/a/b/c").unwrap();
    assert_eq!(listing.len(), 1);
    assert_eq!(listing[0].name, "leaf.txt");

    let data = fs.read_file("/a/b/c/leaf.txt").unwrap();
    assert_eq!(data, b"deep file");

    let top = fs.list_dir("/").unwrap();
    assert_eq!(top.len(), 1);
    assert_eq!(top[0].name, "a");
    assert!(top[0].is_dir);
}

#[test]
fn persistence_survives_reopen() {
    let path = img_path();
    {
        let mut fs = Strata::format(&path, 512, 64).unwrap();
        fs.mkdir("/docs").unwrap();
        fs.write_file("/docs/note.txt", b"remember this").unwrap();
    }
    {
        let mut fs = Strata::open(&path).unwrap();
        let data = fs.read_file("/docs/note.txt").unwrap();
        assert_eq!(data, b"remember this");
        let listing = fs.list_dir("/docs").unwrap();
        assert_eq!(listing[0].name, "note.txt");
    }
}

#[test]
fn corrupt_superblock_is_rejected_without_panic() {
    let path = img_path();
    {
        Strata::format(&path, 512, 64).unwrap();
    }
    // Stomp the magic bytes at the start of the image.
    {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
        f.write_all(&[0xDE, 0xAD, 0xBE, 0xEF]).unwrap();
    }
    let result = Strata::open(&path);
    assert!(matches!(result, Err(FsError::BadMagic)));
}

#[test]
fn empty_or_garbage_file_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty.img");
    std::fs::write(&path, b"not a strata image").unwrap();
    let result = Strata::open(&path);
    assert!(result.is_err());
}

#[test]
fn oversized_file_is_rejected() {
    let path = img_path();
    let mut fs = Strata::format(&path, 512, 64).unwrap();
    // 64 direct pointers * 4096 bytes = 262144 max file size.
    let too_big = vec![0u8; 262145];
    let result = fs.write_file("/oversized.bin", &too_big);
    assert!(matches!(result, Err(FsError::FileTooLarge)));
}

#[test]
fn removing_nonempty_directory_fails() {
    let path = img_path();
    let mut fs = Strata::format(&path, 512, 64).unwrap();
    fs.mkdir("/full").unwrap();
    fs.write_file("/full/x.txt", b"x").unwrap();
    let result = fs.remove("/full");
    assert!(matches!(result, Err(FsError::NotEmpty)));
}
