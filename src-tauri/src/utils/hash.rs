use std::fs::File;
use std::io::{Read, Result as IoResult};
use std::path::Path;

use blake3::Hasher as Blake3;
use md5::Context;

use crate::error::Result;

const BUFFER_SIZE: usize = 64 * 1024;

pub fn md5_file(path: &Path) -> Result<String> {
    digest(path, HashAlgorithm::Md5)
}

pub fn blake3_file(path: &Path) -> Result<String> {
    digest(path, HashAlgorithm::Blake3)
}

pub enum HashAlgorithm {
    Md5,
    Blake3,
}

pub fn digest(path: &Path, algorithm: HashAlgorithm) -> Result<String> {
    let mut file = File::open(path)?;
    match algorithm {
        HashAlgorithm::Md5 => md5_digest(&mut file),
        HashAlgorithm::Blake3 => blake3_digest(&mut file),
    }
}

fn md5_digest(reader: &mut File) -> Result<String> {
    let mut context = Context::new();
    read_in_chunks(reader, |chunk| {
        context.consume(chunk);
        Ok(())
    })?;
    let digest = context.compute();
    Ok(format!("{:x}", digest))
}

fn blake3_digest(reader: &mut File) -> Result<String> {
    let mut hasher = Blake3::new();
    read_in_chunks(reader, |chunk| {
        hasher.update(chunk);
        Ok(())
    })?;
    Ok(hasher.finalize().to_hex().to_string())
}

fn read_in_chunks<F>(reader: &mut File, mut f: F) -> Result<()>
where
    F: FnMut(&[u8]) -> IoResult<()>,
{
    let mut buffer = vec![0_u8; BUFFER_SIZE];
    loop {
        let bytes = reader.read(&mut buffer)?;
        if bytes == 0 {
            break;
        }
        f(&buffer[..bytes])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn md5_matches_known_value() -> Result<()> {
        let mut file = NamedTempFile::new()?;
        write!(file, "hello world")?;
        let digest = md5_file(file.path())?;
        assert_eq!(digest, "5eb63bbbe01eeed093cb22bb8f5acdc3");
        Ok(())
    }
}
