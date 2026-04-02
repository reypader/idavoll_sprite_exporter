use anyhow::{anyhow, Result};
use std::io::{Cursor, Read, Seek, SeekFrom};

pub(crate) fn ru8(c: &mut Cursor<&[u8]>) -> Result<u8> {
    let mut buf = [0u8; 1];
    c.read_exact(&mut buf)?;
    Ok(buf[0])
}

pub(crate) fn ru16(c: &mut Cursor<&[u8]>) -> Result<u16> {
    let mut buf = [0u8; 2];
    c.read_exact(&mut buf)?;
    Ok(u16::from_le_bytes(buf))
}

pub(crate) fn ri32(c: &mut Cursor<&[u8]>) -> Result<i32> {
    let mut buf = [0u8; 4];
    c.read_exact(&mut buf)?;
    Ok(i32::from_le_bytes(buf))
}

pub(crate) fn rf32(c: &mut Cursor<&[u8]>) -> Result<f32> {
    let mut buf = [0u8; 4];
    c.read_exact(&mut buf)?;
    Ok(f32::from_le_bytes(buf))
}

pub(crate) fn read_vec3(c: &mut Cursor<&[u8]>) -> Result<[f32; 3]> {
    Ok([rf32(c)?, rf32(c)?, rf32(c)?])
}

/// Reads exactly `len` bytes, strips trailing NUL bytes, returns lossy UTF-8.
pub(crate) fn read_fixed_string(c: &mut Cursor<&[u8]>, len: usize) -> Result<String> {
    let mut buf = vec![0u8; len];
    c.read_exact(&mut buf)?;
    let end = buf.iter().position(|&b| b == 0).unwrap_or(len);
    Ok(String::from_utf8_lossy(&buf[..end]).into_owned())
}

/// Reads a length-prefixed string (i32 length + N bytes). Used by RSM2.
pub(crate) fn read_len_string(c: &mut Cursor<&[u8]>) -> Result<String> {
    let len = ri32(c)? as usize;
    let mut buf = vec![0u8; len];
    c.read_exact(&mut buf)?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

pub(crate) fn check_magic(c: &mut Cursor<&[u8]>, expected: &[u8; 4]) -> Result<()> {
    let mut buf = [0u8; 4];
    c.read_exact(&mut buf)?;
    if &buf != expected {
        return Err(anyhow!(
            "invalid magic: expected {:?}, got {:?}",
            expected,
            buf
        ));
    }
    Ok(())
}

pub(crate) fn skip(c: &mut Cursor<&[u8]>, n: i64) -> Result<()> {
    c.seek(SeekFrom::Current(n))?;
    Ok(())
}
