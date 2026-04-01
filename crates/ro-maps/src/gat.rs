use anyhow::{Context, Result};
use std::io::Cursor;

use crate::util::{check_magic, rf32, ru32, ru8};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerrainType {
    Walkable,
    Blocked,
    /// Cliff / impassable snipeable.
    Snipeable,
    Unknown(u8),
}

#[derive(Debug, Clone, Copy)]
pub struct GatTile {
    pub altitude_sw: f32,
    pub altitude_se: f32,
    pub altitude_nw: f32,
    pub altitude_ne: f32,
    pub terrain_type: TerrainType,
    /// v1.3+: tile is flagged as water (bit 0x80 of the high byte in the raw terrain u32).
    pub is_water: bool,
}

pub struct GatFile {
    pub version: (u8, u8),
    pub width: u32,
    pub height: u32,
    /// Row-major: index = row * width + col.
    pub tiles: Vec<GatTile>,
}

impl GatFile {
    /// Implementation covers GAT v1.2-v1.3.
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut c = Cursor::new(data);
        check_magic(&mut c, b"GRAT")?;
        let major = ru8(&mut c)?;
        let minor = ru8(&mut c)?;

        (|| -> anyhow::Result<GatFile> {
            let width = ru32(&mut c)?;
            let height = ru32(&mut c)?;

            let count = (width as usize)
                .checked_mul(height as usize)
                .unwrap_or(0);
            let mut tiles = Vec::with_capacity(count);

            for _ in 0..count {
                let altitude_sw = rf32(&mut c)?;
                let altitude_se = rf32(&mut c)?;
                let altitude_nw = rf32(&mut c)?;
                let altitude_ne = rf32(&mut c)?;
                let raw = ru32(&mut c)?;

                let is_water = minor >= 3 && (raw & 0x8000_0000 != 0);
                let type_bits = raw & 0x7FFF_FFFF;
                let terrain_type = match type_bits {
                    0 => TerrainType::Walkable,
                    1 => TerrainType::Blocked,
                    5 => TerrainType::Snipeable,
                    n => TerrainType::Unknown(n as u8),
                };

                tiles.push(GatTile {
                    altitude_sw,
                    altitude_se,
                    altitude_nw,
                    altitude_ne,
                    terrain_type,
                    is_water,
                });
            }

            Ok(GatFile {
                version: (major, minor),
                width,
                height,
                tiles,
            })
        })()
        .with_context(|| {
            format!("GAT v{major}.{minor} (implementation covers v1.2-v1.3)")
        })
    }

    pub fn tile(&self, col: u32, row: u32) -> Option<&GatTile> {
        if col < self.width && row < self.height {
            self.tiles.get((row * self.width + col) as usize)
        } else {
            None
        }
    }
}
