use anyhow::{Context, Result};
use std::io::{Cursor, Read, Seek, SeekFrom};

use crate::util::{check_magic, rf32, ri16, ri32, ru8};

#[derive(Debug, Clone)]
pub struct GndLightmapSlice {
    /// 8×8 grayscale ambient occlusion (shadow) map.
    pub shadowmap: [u8; 64],
    /// 8×8 RGB baked light color.
    pub lightmap: [u8; 192],
}

/// A single textured surface (top or wall face of a cube). Indices into this list are stored
/// in `GndCube`.
#[derive(Debug, Clone)]
pub struct GndSurface {
    /// Diffuse U coordinates for the four corners in order [SW, SE, NW, NE].
    pub u: [f32; 4],
    /// Diffuse V coordinates for the four corners in order [SW, SE, NW, NE].
    pub v: [f32; 4],
    /// Index into `GndFile::texture_paths`. -1 means no texture.
    pub texture_id: i16,
    /// Index into `GndFile::lightmap_slices`.
    pub lightmap_id: i16,
    /// BGRA tile color (single per surface, not per-corner).
    pub color: [u8; 4],
}

/// One cell in the ground mesh grid. Each cube spans `scale` world units in X and Z.
#[derive(Debug, Clone)]
pub struct GndCube {
    /// Corner heights in order [SW, SE, NW, NE]. RO heights are Y-down; negate when converting
    /// to Bevy Y-up coordinates.
    pub heights: [f32; 4],
    /// Index into `GndFile::surfaces` for the top (horizontal) face. -1 = no surface.
    pub top_surface_id: i32,
    /// Index into `GndFile::surfaces` for the north (front) wall face. -1 = no surface.
    pub north_surface_id: i32,
    /// Index into `GndFile::surfaces` for the east (right) wall face. -1 = no surface.
    pub east_surface_id: i32,
}

#[derive(Debug, Clone)]
pub struct GndWaterPlane {
    pub level: f32,
    pub water_type: i32,
    pub wave_height: f32,
    pub wave_speed: f32,
    pub wave_pitch: f32,
    pub texture_cycling_interval: i32,
}

pub struct GndFile {
    pub version: (u8, u8),
    pub width: i32,
    pub height: i32,
    /// World units per cube edge. Always 10.0 in practice.
    pub scale: f32,
    /// Relative paths to diffuse textures (e.g. `"texture/유저인터페이스/map/...bmp"`).
    pub texture_paths: Vec<String>,
    /// Pre-computed lightmap slices baked into the map.
    pub lightmap_slices: Vec<GndLightmapSlice>,
    /// Surface definitions referenced by cube face IDs.
    pub surfaces: Vec<GndSurface>,
    /// Row-major grid of ground cubes. Index = row * width + col.
    pub cubes: Vec<GndCube>,
    /// Water plane configuration present in v1.8+.
    pub water: Option<GndWaterPlane>,
}

impl GndFile {
    /// Implementation covers GND v1.7-v1.9.
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut c = Cursor::new(data);

        check_magic(&mut c, b"GRGN")?;

        let major = ru8(&mut c)?;
        let minor = ru8(&mut c)?;

        (|| -> anyhow::Result<GndFile> {
            let version = (major, minor);

        let width = ri32(&mut c)?;
        let height = ri32(&mut c)?;
        let scale = rf32(&mut c)?;

        // Textures
        let texture_count = ri32(&mut c)? as usize;
        let texture_path_len = ri32(&mut c)? as usize;
        let mut texture_paths = Vec::with_capacity(texture_count);
        for i in 0..texture_count {
            let mut buf = vec![0u8; texture_path_len];
            c.read_exact(&mut buf).with_context(|| format!("texture {i}/{texture_count} (path_len={texture_path_len})"))?;
            let end = buf.iter().position(|&b| b == 0).unwrap_or(texture_path_len);
            texture_paths.push(String::from_utf8_lossy(&buf[..end]).into_owned());
        }

        // Lightmap slices — the pixel_format/width/height are a single global header
        // that precedes all slices, not repeated per slice. Each slice is 256 bytes.
        let lightmap_count = ri32(&mut c)? as usize;
        let _pixel_format = ri32(&mut c).with_context(|| "lightmap global header: pixel_format")?;
        let _lm_width = ri32(&mut c).with_context(|| "lightmap global header: width")?;
        let _lm_height = ri32(&mut c).with_context(|| "lightmap global header: height")?;
        let mut lightmap_slices = Vec::with_capacity(lightmap_count);
        for i in 0..lightmap_count {
            let mut shadowmap = [0u8; 64];
            c.read_exact(&mut shadowmap).with_context(|| format!("lightmap slice {i}/{lightmap_count}: shadowmap"))?;
            let mut lightmap = [0u8; 192];
            c.read_exact(&mut lightmap).with_context(|| format!("lightmap slice {i}/{lightmap_count}: lightmap"))?;
            lightmap_slices.push(GndLightmapSlice { shadowmap, lightmap });
        }

        // Surfaces (56 bytes each)
        let surface_count = ri32(&mut c).with_context(|| format!("surface count (after {} lightmap slices)", lightmap_count))? as usize;
        let mut surfaces = Vec::with_capacity(surface_count);
        for _ in 0..surface_count {
            let mut u = [0f32; 4];
            for v in u.iter_mut() {
                *v = rf32(&mut c)?;
            }
            let mut v = [0f32; 4];
            for vv in v.iter_mut() {
                *vv = rf32(&mut c)?;
            }
            // offset 32: texture_id (i16), offset 34: lightmap_id (i16) — no padding
            let texture_id = ri16(&mut c)?;
            let lightmap_id = ri16(&mut c)?;
            // offset 36: single BGRA tile color (4 bytes)
            let mut color = [0u8; 4];
            c.read_exact(&mut color)?;
            surfaces.push(GndSurface {
                u,
                v,
                texture_id,
                lightmap_id,
                color,
            });
        }

        // Cubes (28 bytes each)
        let cube_count = (width as usize)
            .checked_mul(height as usize)
            .unwrap_or(0);
        let mut cubes = Vec::with_capacity(cube_count);
        for _ in 0..cube_count {
            let mut heights = [0f32; 4];
            for h in heights.iter_mut() {
                *h = rf32(&mut c)?;
            }
            let top_surface_id = ri32(&mut c)?;
            let north_surface_id = ri32(&mut c)?;
            let east_surface_id = ri32(&mut c)?;
            cubes.push(GndCube {
                heights,
                top_surface_id,
                north_surface_id,
                east_surface_id,
            });
        }

        // v1.8+: water plane
        let water = if major > 1 || (major == 1 && minor >= 8) {
            let level = rf32(&mut c)?;
            let water_type = ri32(&mut c)?;
            let wave_height = rf32(&mut c)?;
            let wave_speed = rf32(&mut c)?;
            let wave_pitch = rf32(&mut c)?;
            let texture_cycling_interval = ri32(&mut c)?;

            // v1.9+: multiple water planes (u × v grid); read and discard the extra data
            if minor >= 9 {
                let planes_u = ri32(&mut c)?;
                let planes_v = ri32(&mut c)?;
                let extra = (planes_u * planes_v) as usize;
                c.seek(SeekFrom::Current((extra * 4) as i64))?; // per-plane level floats
            }

            Some(GndWaterPlane {
                level,
                water_type,
                wave_height,
                wave_speed,
                wave_pitch,
                texture_cycling_interval,
            })
        } else {
            None
        };

        Ok(GndFile {
            version,
            width,
            height,
            scale,
            texture_paths,
            lightmap_slices,
            surfaces,
            cubes,
            water,
        })
        })()
        .with_context(|| format!("GND v{major}.{minor} (implementation covers v1.7-v1.9)"))
    }

    pub fn cube(&self, col: i32, row: i32) -> Option<&GndCube> {
        if col >= 0 && row >= 0 && col < self.width && row < self.height {
            self.cubes.get((row * self.width + col) as usize)
        } else {
            None
        }
    }
}
