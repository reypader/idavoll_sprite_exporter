use anyhow::{Context, Result};
use std::io::{Cursor, Read};

fn read_f32(cursor: &mut Cursor<&[u8]>) -> Result<f32> {
    let mut buf = [0u8; 4];
    cursor.read_exact(&mut buf)?;
    Ok(f32::from_le_bytes(buf))
}

fn read_i32(cursor: &mut Cursor<&[u8]>) -> Result<i32> {
    let mut buf = [0u8; 4];
    cursor.read_exact(&mut buf)?;
    Ok(i32::from_le_bytes(buf))
}

fn read_u32(cursor: &mut Cursor<&[u8]>) -> Result<u32> {
    let mut buf = [0u8; 4];
    cursor.read_exact(&mut buf)?;
    Ok(u32::from_le_bytes(buf))
}

/// Parsed IMF file.
///
/// IMF stores per-frame draw priorities for each layer of a body sprite. The primary use
/// is determining whether the head sprite should render behind the body: if
/// `priority(layer=1, action, frame) == 1`, the head renders before the body.
///
/// Each frame also contains cx/cy fields, confirmed via debugging to always be zero —
/// they appear to be unused/reserved. ACT attach points are the authoritative source
/// for compositing offsets.
pub struct ImfFile {
    pub version: f32,
    // layers[layer][action][frame] = priority
    layers: Vec<Vec<Vec<i32>>>,
}

impl ImfFile {
    /// Implementation has no version branches; covers all known IMF versions.
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut cursor = Cursor::new(data);

        let version = read_f32(&mut cursor)?;

        (|| -> anyhow::Result<ImfFile> {
        let _checksum = read_i32(&mut cursor)?;
        let max_layer = read_u32(&mut cursor)? as usize;

        let mut layers = Vec::with_capacity(max_layer + 1);
        for _ in 0..=max_layer {
            let num_actions = read_u32(&mut cursor)? as usize;
            let mut actions = Vec::with_capacity(num_actions);
            for _ in 0..num_actions {
                let num_frames = read_u32(&mut cursor)? as usize;
                let mut frames = Vec::with_capacity(num_frames);
                for _ in 0..num_frames {
                    let priority = read_i32(&mut cursor)?;
                    let _cx = read_i32(&mut cursor)?;
                    let _cy = read_i32(&mut cursor)?;
                    frames.push(priority);
                }
                actions.push(frames);
            }
            layers.push(actions);
        }

        Ok(ImfFile { version, layers })
        })()
        .with_context(|| format!("IMF v{version}"))
    }

    /// Returns the priority for the given layer/action/frame, or `None` if out of bounds.
    pub fn priority(&self, layer: usize, action: usize, frame: usize) -> Option<i32> {
        self.layers
            .get(layer)
            .and_then(|a| a.get(action))
            .and_then(|f| f.get(frame))
            .copied()
    }
}
