use crate::error::Result;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::Path;

// AI-FUNC-SUMMARY:
// Purpose: Compute the SHA-256 of a byte slice.
// Inputs: the bytes.
// Returns: lowercase hexadecimal digest, 64 characters.
// Side effects: None.
// Notes: Used for content identity in the placement record and run report.
pub fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex_digest(&hasher.finalize())
}

// AI-FUNC-SUMMARY:
// Purpose: Stream a file through SHA-256 without loading it whole.
// Inputs: path to the file.
// Returns: (lowercase hexadecimal digest, file length in bytes).
// Side effects: Reads from disk.
// Notes: Errors with Io when the file cannot be opened or read; the length is the number of bytes hashed.
pub fn sha256_file(path: &Path) -> Result<(String, u64)> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    let mut total: u64 = 0;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        total += read as u64;
    }
    Ok((hex_digest(&hasher.finalize()), total))
}

// AI-FUNC-SUMMARY: Render a digest as lowercase hex; returns the hex string; side effects: none.
fn hex_digest(digest: &[u8]) -> String {
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

pub(crate) struct HashingReader<R> {
    reader: R,
    hasher: Sha256,
    bytes: u64,
}

impl<R> HashingReader<R> {
    // AI-FUNC-SUMMARY: Wrap a forward reader with incremental SHA-256 state without allocating a file-sized buffer.
    pub(crate) fn new(reader: R) -> Self {
        Self {
            reader,
            hasher: Sha256::new(),
            bytes: 0,
        }
    }

    // AI-FUNC-SUMMARY: Consume the reader wrapper and return the digest and count of bytes actually delivered to its consumer.
    pub(crate) fn finish(self) -> (String, u64) {
        (hex_digest(&self.hasher.finalize()), self.bytes)
    }
}

impl<R: Read> Read for HashingReader<R> {
    // AI-FUNC-SUMMARY: Read and hash each successfully delivered byte exactly once; propagate the underlying read error.
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let count = self.reader.read(buffer)?;
        self.hasher.update(&buffer[..count]);
        self.bytes += count as u64;
        Ok(count)
    }
}
