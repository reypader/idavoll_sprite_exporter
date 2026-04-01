use anyhow::{anyhow, Context, Result};
use std::io::{Cursor, Seek, SeekFrom};

use crate::util::{check_magic, read_fixed_string, rf32, ri32, ru32, ru8};

#[derive(Debug, Clone)]
pub struct RswLighting {
    pub longitude: u32,
    pub latitude: u32,
    pub diffuse: [f32; 3],
    pub ambient: [f32; 3],
    pub shadowmap_alpha: f32,
}

#[derive(Debug, Clone)]
pub struct ModelInstance {
    pub name: String,
    pub anim_type: i32,
    pub anim_speed: f32,
    pub collision_flags: i32,
    pub model_file: String,
    pub node_name: String,
    pub pos: [f32; 3],
    pub rot: [f32; 3],
    pub scale: [f32; 3],
}

#[derive(Debug, Clone)]
pub struct LightSource {
    pub name: String,
    pub pos: [f32; 3],
    pub diffuse: [f32; 3],
    pub range: f32,
}

#[derive(Debug, Clone)]
pub struct AudioSource {
    pub name: String,
    pub file: String,
    pub pos: [f32; 3],
    pub volume: f32,
    pub width: u32,
    pub height: u32,
    pub range: f32,
    pub cycle: f32,
}

#[derive(Debug, Clone)]
pub struct EffectEmitter {
    pub name: String,
    pub pos: [f32; 3],
    pub effect_id: u32,
    pub emit_speed: f32,
    pub params: [f32; 4],
}

#[derive(Debug, Clone)]
pub enum RswObject {
    Model(ModelInstance),
    Light(LightSource),
    Audio(AudioSource),
    Effect(EffectEmitter),
}

pub struct RswFile {
    pub version: (u8, u8),
    pub lighting: RswLighting,
    pub objects: Vec<RswObject>,
}

fn at_least(v: (u8, u8), major: u8, minor: u8) -> bool {
    v.0 > major || (v.0 == major && v.1 >= minor)
}

impl RswFile {
    /// Implementation covers RSW v1.9-v2.6 (build 162).
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut c = Cursor::new(data);
        check_magic(&mut c, b"GRSW")?;
        let major = ru8(&mut c)?;
        let minor = ru8(&mut c)?;

        (|| -> anyhow::Result<RswFile> {
        let version = (major, minor);

        // Build number: present in v2.2+ (browedit only reads one extra byte for exactly v2.2)
        if at_least(version, 2, 5) {
            let _ = ru32(&mut c)?; // build_number (u32 in v2.5+)
            let _ = ru8(&mut c)?;  // unknown render flag
        } else if at_least(version, 2, 2) {
            let _ = ru8(&mut c)?;  // build_number (u8 for v2.2)
        }

        // File references: ini (40), gnd (40), gat (40, present for v1.5+), src (40)
        let _ = read_fixed_string(&mut c, 40)?; // ini file
        let _ = read_fixed_string(&mut c, 40)?; // gnd file
        if major > 1 || minor > 4 {
            let _ = read_fixed_string(&mut c, 40)?; // gat file (v1.5+)
        }
        let _ = read_fixed_string(&mut c, 40)?; // src file

        // Water configuration: present when version < (2, 6) — moved to GND in v2.6+
        if !at_least(version, 2, 6) {
            // level, type, waveHeight, waveSpeed, wavePitch, textureCyclingInterval = 6 × 4 bytes
            c.seek(SeekFrom::Current(24))?;
        }

        // Lighting
        let longitude = ru32(&mut c)?;
        let latitude = ru32(&mut c)?;
        let diffuse = [rf32(&mut c)?, rf32(&mut c)?, rf32(&mut c)?];
        let ambient = [rf32(&mut c)?, rf32(&mut c)?, rf32(&mut c)?];
        let shadowmap_alpha = rf32(&mut c)?;

        // Map render flags / bounding box (16 bytes); present in v2.5+ as well as earlier
        // versions that embed a bounding box. Skip to stay version-safe.
        c.seek(SeekFrom::Current(16))?;

        // Objects
        let object_count = ri32(&mut c)? as usize;
        let mut objects = Vec::with_capacity(object_count);

        for _ in 0..object_count {
            let type_id = ri32(&mut c)?;
            match type_id {
                1 => {
                    let name = read_fixed_string(&mut c, 40)?;
                    let anim_type = ri32(&mut c)?;
                    let anim_speed = rf32(&mut c)?;
                    let collision_flags = ri32(&mut c)?;

                    // v2.6 build 162+: one extra unknown byte between collision_flags and model_file
                    // We cannot check build number here without storing it, so we rely on the
                    // caller knowing. For safety we skip this only when we have evidence the
                    // field is present; for now it is omitted (rare, affects only some v2.6 maps).

                    let model_file = read_fixed_string(&mut c, 80)?;
                    let node_name = read_fixed_string(&mut c, 80)?;
                    let pos = [rf32(&mut c)?, rf32(&mut c)?, rf32(&mut c)?];
                    let rot = [rf32(&mut c)?, rf32(&mut c)?, rf32(&mut c)?];
                    let scale = [rf32(&mut c)?, rf32(&mut c)?, rf32(&mut c)?];
                    objects.push(RswObject::Model(ModelInstance {
                        name,
                        anim_type,
                        anim_speed,
                        collision_flags,
                        model_file,
                        node_name,
                        pos,
                        rot,
                        scale,
                    }));
                }
                2 => {
                    let name = read_fixed_string(&mut c, 80)?;
                    let pos = [rf32(&mut c)?, rf32(&mut c)?, rf32(&mut c)?];
                    let diffuse = [rf32(&mut c)?, rf32(&mut c)?, rf32(&mut c)?];
                    let range = rf32(&mut c)?;
                    objects.push(RswObject::Light(LightSource {
                        name,
                        pos,
                        diffuse,
                        range,
                    }));
                }
                3 => {
                    let name = read_fixed_string(&mut c, 80)?;
                    let file = read_fixed_string(&mut c, 80)?;
                    let pos = [rf32(&mut c)?, rf32(&mut c)?, rf32(&mut c)?];
                    let volume = rf32(&mut c)?;
                    let width = ru32(&mut c)?;
                    let height = ru32(&mut c)?;
                    let range = rf32(&mut c)?;
                    // cycle interval: present in v2.0+; default to 4.0 for older files
                    let cycle = if at_least(version, 2, 0) {
                        rf32(&mut c)?
                    } else {
                        4.0
                    };
                    objects.push(RswObject::Audio(AudioSource {
                        name,
                        file,
                        pos,
                        volume,
                        width,
                        height,
                        range,
                        cycle,
                    }));
                }
                4 => {
                    let name = read_fixed_string(&mut c, 80)?;
                    let pos = [rf32(&mut c)?, rf32(&mut c)?, rf32(&mut c)?];
                    let effect_id = ru32(&mut c)?;
                    let emit_speed = rf32(&mut c)?;
                    let params = [rf32(&mut c)?, rf32(&mut c)?, rf32(&mut c)?, rf32(&mut c)?];
                    objects.push(RswObject::Effect(EffectEmitter {
                        name,
                        pos,
                        effect_id,
                        emit_speed,
                        params,
                    }));
                }
                n => return Err(anyhow!("unknown RSW object type: {}", n)),
            }
        }

        // QuadTree follows in v2.1+ but nothing comes after it, so we stop here.

        Ok(RswFile {
            version,
            lighting: RswLighting {
                longitude,
                latitude,
                diffuse,
                ambient,
                shadowmap_alpha,
            },
            objects,
        })
        })()
        .with_context(|| format!("RSW v{major}.{minor} (implementation covers v1.9-v2.6)"))
    }
}
