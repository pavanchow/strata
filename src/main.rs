use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;
use strata::Strata;

/// A small virtual file system inside one container file.
#[derive(Parser)]
#[command(name = "strata", version, about)]
struct Cli {
    /// Path to the image file.
    #[arg(long = "img")]
    img: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a new, empty image.
    Format {
        /// Total number of blocks in the image (4 KiB each).
        #[arg(long, default_value_t = 4096)]
        blocks: u32,
        /// Number of inodes to reserve.
        #[arg(long, default_value_t = 512)]
        inodes: u32,
    },
    /// Create a directory.
    Mkdir { path: String },
    /// Copy a local file into the image.
    Put { localfile: PathBuf, path: String },
    /// Copy a file out of the image.
    Get { path: String, localfile: PathBuf },
    /// List a directory.
    Ls { path: String },
    /// Delete a file or empty directory.
    Rm { path: String },
}

fn run() -> strata::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Format { blocks, inodes } => {
            Strata::format(&cli.img, blocks, inodes)?;
            println!("formatted {} ({} blocks, {} inodes)", cli.img.display(), blocks, inodes);
        }
        Command::Mkdir { path } => {
            let mut fs = Strata::open(&cli.img)?;
            fs.mkdir(&path)?;
            println!("created {path}");
        }
        Command::Put { localfile, path } => {
            let data = std::fs::read(&localfile)?;
            let mut fs = Strata::open(&cli.img)?;
            fs.write_file(&path, &data)?;
            println!("wrote {} bytes to {path}", data.len());
        }
        Command::Get { path, localfile } => {
            let mut fs = Strata::open(&cli.img)?;
            let data = fs.read_file(&path)?;
            std::fs::write(&localfile, &data)?;
            println!("read {} bytes from {path}", data.len());
        }
        Command::Ls { path } => {
            let mut fs = Strata::open(&cli.img)?;
            for entry in fs.list_dir(&path)? {
                let kind = if entry.is_dir { "dir " } else { "file" };
                println!("{kind}  {:>8}  {}", entry.size, entry.name);
            }
        }
        Command::Rm { path } => {
            let mut fs = Strata::open(&cli.img)?;
            fs.remove(&path)?;
            println!("removed {path}");
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("strata: {e}");
            ExitCode::FAILURE
        }
    }
}
