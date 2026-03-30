use anyhow::{anyhow, Result};
use std::io::{Cursor, Read, Seek, SeekFrom};

#[derive(Debug, Clone)]
pub struct ActSprite {
    pub x: i32,
    pub y: i32,
    pub spr_id: i32,
    pub flags: u32,    // bit 0 = horizontal flip
    pub tint: [u8; 4], // [R, G, B, A], default 255,255,255,255
    pub x_scale: f32,
    pub y_scale: f32,
    pub rotation: i32, // degrees, clockwise
    pub spr_type: i32, // 0 = palette-indexed, 1 = RGBA
    #[allow(dead_code)]
    pub width: i32, // explicit (v2.5+), 0 if not stored
    #[allow(dead_code)]
    pub height: i32, // explicit (v2.5+), 0 if not stored
}

#[derive(Debug, Clone)]
pub struct AttachPoint {
    pub x: i32,
    pub y: i32,
    #[allow(dead_code)]
    pub attr: i32,
}

#[derive(Debug, Clone)]
pub struct ActFrame {
    pub event_id: i32,
    pub sprites: Vec<ActSprite>,
    pub attach_points: Vec<AttachPoint>,
}

#[derive(Debug, Clone)]
pub struct ActAction {
    pub interval: f32, // multiply by 24 to get milliseconds
    pub frames: Vec<ActFrame>,
}

impl ActAction {
    pub fn frame_ms(&self) -> u32 {
        (self.interval * 24.0).round() as u32
    }
}

pub struct ActFile {
    pub version: u16,
    pub actions: Vec<ActAction>,
    pub events: Vec<String>,
}

impl ActFile {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut c = Cursor::new(data);

        let mut sig = [0u8; 2];
        c.read_exact(&mut sig)?;
        if &sig != b"AC" {
            return Err(anyhow!("Invalid ACT signature"));
        }

        let version = ru16(&mut c)?;
        let action_count = ru16(&mut c)? as usize;
        c.seek(SeekFrom::Current(10))?; // 10 reserved bytes

        let mut actions = Vec::with_capacity(action_count);

        for _ in 0..action_count {
            let frame_count = ru32(&mut c)? as usize;
            let mut frames = Vec::with_capacity(frame_count);

            for _ in 0..frame_count {
                // Skip attackRange + fitRange (8 × uint = 32 bytes)
                c.seek(SeekFrom::Current(32))?;

                let sprite_count = ru32(&mut c)? as usize;
                let mut sprites = Vec::with_capacity(sprite_count);

                for _ in 0..sprite_count {
                    let x = ri32(&mut c)?;
                    let y = ri32(&mut c)?;
                    let spr_id = ri32(&mut c)?;
                    let flags = ru32(&mut c)?;
                    let mut tint = [0u8; 4]; // [R, G, B, A]
                    c.read_exact(&mut tint)?;
                    let x_scale = rf32(&mut c)?;
                    let y_scale = if version >= 0x204 {
                        rf32(&mut c)?
                    } else {
                        x_scale
                    };
                    let rotation = ri32(&mut c)?;
                    let spr_type = ri32(&mut c)?;
                    let (width, height) = if version >= 0x205 {
                        (ri32(&mut c)?, ri32(&mut c)?)
                    } else {
                        (0, 0)
                    };
                    sprites.push(ActSprite {
                        x,
                        y,
                        spr_id,
                        flags,
                        tint,
                        x_scale,
                        y_scale,
                        rotation,
                        spr_type,
                        width,
                        height,
                    });
                }

                let event_id = if version >= 0x200 { ri32(&mut c)? } else { -1 };

                let attach_points = if version >= 0x203 {
                    let count = ru32(&mut c)? as usize;
                    let mut pts = Vec::with_capacity(count);
                    for _ in 0..count {
                        c.seek(SeekFrom::Current(4))?; // reserved
                        let x = ri32(&mut c)?;
                        let y = ri32(&mut c)?;
                        let attr = ri32(&mut c)?;
                        pts.push(AttachPoint { x, y, attr });
                    }
                    pts
                } else {
                    vec![]
                };

                frames.push(ActFrame {
                    event_id,
                    sprites,
                    attach_points,
                });
            }

            actions.push(ActAction {
                interval: 4.0,
                frames,
            });
        }

        let events = if version >= 0x201 {
            let count = ru32(&mut c)? as usize;
            let mut evts = Vec::with_capacity(count);
            for _ in 0..count {
                let mut buf = [0u8; 40];
                c.read_exact(&mut buf)?;
                let s = std::str::from_utf8(&buf)
                    .unwrap_or("")
                    .trim_end_matches('\0')
                    .to_string();
                evts.push(s);
            }
            evts
        } else {
            vec![]
        };

        // Per-action frame intervals stored after the events block (v2.2+)
        if version >= 0x202 {
            for action in &mut actions {
                action.interval = rf32(&mut c)?;
            }
        }

        Ok(ActFile {
            version,
            actions,
            events,
        })
    }
}

fn ru16(c: &mut Cursor<&[u8]>) -> Result<u16> {
    let mut b = [0u8; 2];
    c.read_exact(&mut b)?;
    Ok(u16::from_le_bytes(b))
}

fn ru32(c: &mut Cursor<&[u8]>) -> Result<u32> {
    let mut b = [0u8; 4];
    c.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

fn ri32(c: &mut Cursor<&[u8]>) -> Result<i32> {
    let mut b = [0u8; 4];
    c.read_exact(&mut b)?;
    Ok(i32::from_le_bytes(b))
}

fn rf32(c: &mut Cursor<&[u8]>) -> Result<f32> {
    let mut b = [0u8; 4];
    c.read_exact(&mut b)?;
    Ok(f32::from_le_bytes(b))
}
