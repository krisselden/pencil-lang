//! Fingerprints the core's source, so `compiled` can tell a figure written by this build from one
//! written by any other. Every `.rs` file under `src/` and `Cargo.toml` go into one FNV-1a hash,
//! by path and then contents, in sorted order so the same tree gives the same number on any
//! machine. Nothing is kept by hand, so there is no version to forget to bump.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const PRIME: u64 = 0x0000_0100_0000_01b3;

fn feed(hash: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *hash ^= u64::from(*byte);
        *hash = hash.wrapping_mul(PRIME);
    }
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            rust_files(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

fn main() -> io::Result<()> {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets this"));
    let mut files = Vec::new();
    rust_files(&root.join("src"), &mut files)?;
    files.push(root.join("Cargo.toml"));
    files.sort();
    let mut hash = OFFSET;
    for file in &files {
        let name = file.strip_prefix(&root).unwrap_or(file);
        // A path and a length before each file, so moving text between files changes the hash.
        let text = fs::read(file)?;
        feed(
            &mut hash,
            name.to_string_lossy().replace('\\', "/").as_bytes(),
        );
        feed(&mut hash, &(text.len() as u64).to_le_bytes());
        feed(&mut hash, &text);
    }
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("cargo sets this"));
    fs::write(
        out.join("fingerprint.rs"),
        format!("pub const FINGERPRINT: u64 = {hash:#018x};\n"),
    )?;
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=build.rs");
    Ok(())
}
