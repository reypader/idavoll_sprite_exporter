use anyhow::{anyhow, Result};
use std::io::{Cursor, Read};

pub(crate) fn ru8(c: &mut Cursor<&[u8]>) -> Result<u8> {
    let mut buf = [0u8; 1];
    c.read_exact(&mut buf)?;
    Ok(buf[0])
}

#[allow(dead_code)]
pub(crate) fn ru16(c: &mut Cursor<&[u8]>) -> Result<u16> {
    let mut buf = [0u8; 2];
    c.read_exact(&mut buf)?;
    Ok(u16::from_le_bytes(buf))
}

pub(crate) fn ru32(c: &mut Cursor<&[u8]>) -> Result<u32> {
    let mut buf = [0u8; 4];
    c.read_exact(&mut buf)?;
    Ok(u32::from_le_bytes(buf))
}

pub(crate) fn ri16(c: &mut Cursor<&[u8]>) -> Result<i16> {
    let mut buf = [0u8; 2];
    c.read_exact(&mut buf)?;
    Ok(i16::from_le_bytes(buf))
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

/// Reads exactly `len` bytes, strips trailing NUL bytes, and returns the result as a lossy
/// UTF-8 string.
pub(crate) fn read_fixed_string(c: &mut Cursor<&[u8]>, len: usize) -> Result<String> {
    let mut buf = vec![0u8; len];
    c.read_exact(&mut buf)?;
    let end = buf.iter().position(|&b| b == 0).unwrap_or(len);
    Ok(String::from_utf8_lossy(&buf[..end]).into_owned())
}

/// Reads exactly 4 bytes and checks they match `expected`, returning an error if they do not.
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
