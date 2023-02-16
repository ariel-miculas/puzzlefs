use std::fs;
use std::io;

mod noop;
pub use noop::Noop;

mod zstd_wrapper;
pub use zstd_wrapper::*;

// FIXME duplicate definition
const MAX_CHUNK_SIZE: u32 = 16 * 1024 * 1024;

pub trait Compressor: io::Write {
    // https://users.rust-lang.org/t/how-to-move-self-when-using-dyn-trait/50123
    fn end(self: Box<Self>) -> io::Result<()>;
}

pub trait Decompressor: io::Read + io::Seek + Send {
    fn get_uncompressed_length(&mut self) -> io::Result<u64>;
}

pub trait Compression {
    fn compress(dest: fs::File) -> io::Result<Box<dyn Compressor>>;
    fn decompress(source: fs::File) -> io::Result<Box<dyn Decompressor>>;
    fn append_extension(media_type: &str) -> String;
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    pub const TRUTH: &str = "meshuggah rocks";

    pub fn compress_decompress<C: Compression>() -> anyhow::Result<()> {
        let f = NamedTempFile::new()?;
        let mut compressed = C::compress(f.reopen()?)?;
        compressed.write_all(TRUTH.as_bytes())?;
        compressed.end()?;

        let mut buf = vec![0_u8; TRUTH.len()];
        let n = C::decompress(f.reopen()?)?.read(&mut buf)?;
        assert_eq!(n, TRUTH.len());

        assert_eq!(TRUTH.as_bytes(), buf);
        Ok(())
    }

    pub fn compression_is_seekable<C: Compression>() -> anyhow::Result<()> {
        let f = NamedTempFile::new()?;
        let mut compressed = C::compress(f.reopen()?)?;
        compressed.write_all(TRUTH.as_bytes())?;
        compressed.end()?;

        let mut buf = vec![0_u8; 1024];
        let mut decompressor = C::decompress(f.reopen()?)?;
        decompressor.seek(io::SeekFrom::Start("meshuggah ".len() as u64))?;
        let n = decompressor.read(&mut buf)?;
        assert_eq!(n, 5);

        assert_eq!("rocks".as_bytes(), &buf[0..5]);
        Ok(())
    }
}
