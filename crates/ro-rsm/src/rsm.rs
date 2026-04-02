use anyhow::{anyhow, Context, Result};
use std::io::Cursor;

use crate::util::{
    check_magic, read_fixed_string, read_len_string, read_vec3, rf32, ri32, ru16, ru8, skip,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShadeType {
    None,
    Flat,
    Smooth,
    Black,
    Unknown(i32),
}

impl ShadeType {
    fn from_i32(v: i32) -> Self {
        match v {
            0 => Self::None,
            1 => Self::Flat,
            2 => Self::Smooth,
            3 => Self::Black,
            n => Self::Unknown(n),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RsmFrame {
    pub time: i32,
    /// Quaternion components stored as [x, y, z, w].
    pub quaternion: [f32; 4],
}

#[derive(Debug, Clone)]
pub struct RsmFace {
    pub vertex_ids: [u16; 3],
    pub texcoord_ids: [u16; 3],
    /// Index into this mesh's `texture_indices`.
    pub texture_id: u16,
    pub two_sided: bool,
    pub smooth_group: i32,
}

#[derive(Debug, Clone)]
pub struct RsmMesh {
    pub name: String,
    /// Empty string indicates this is the root mesh.
    pub parent_name: String,
    /// Indices into `RsmFile::textures`.
    pub texture_indices: Vec<i32>,
    /// 3×3 rotation matrix, column-major: `offset[col][row]`.
    pub offset: [[f32; 3]; 3],
    /// Secondary translation (RSM2: always zero).
    pub pos_: [f32; 3],
    pub pos: [f32; 3],
    /// Static rotation angle in radians (RSM2: always 0.0).
    pub rot_angle: f32,
    /// Axis for the static rotation (RSM2: always [0,1,0]).
    pub rot_axis: [f32; 3],
    /// Per-axis scale (RSM2: always [1,1,1]).
    pub scale: [f32; 3],
    pub vertices: Vec<[f32; 3]>,
    pub tex_coords: Vec<[f32; 2]>,
    pub faces: Vec<RsmFace>,
    /// Rotation keyframes — parsed but not applied for static rendering.
    pub frames: Vec<RsmFrame>,
}

#[derive(Debug, Clone)]
pub struct RsmFile {
    pub version: u16,
    pub anim_len: i32,
    pub shade_type: ShadeType,
    pub alpha: u8,
    /// Bare texture filenames (e.g. "lamp.bmp").
    pub textures: Vec<String>,
    pub root_node_name: String,
    pub meshes: Vec<RsmMesh>,
    /// Bounding box minimum in model space (before Y-flip).
    pub bbmin: [f32; 3],
    /// Bounding box maximum in model space (before Y-flip).
    pub bbmax: [f32; 3],
    /// `(bbmin + bbmax) / 2` — used as the model pivot point.
    pub bbrange: [f32; 3],
}

impl RsmFile {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut c = Cursor::new(data);
        check_magic(&mut c, b"GRSM")?;
        // Version is stored as [major, minor] (big-endian byte order).
        let major = ru8(&mut c)?;
        let minor = ru8(&mut c)?;
        let version: u16 = (major as u16) << 8 | minor as u16;

        if version >= 0x0200 {
            parse_rsm2(&mut c, version)
        } else {
            parse_rsm1(&mut c, version)
        }
        .with_context(|| {
            let (major, minor) = (version >> 8, version & 0xff);
            format!("RSM v{major}.{minor}")
        })
    }
}

// ---------------------------------------------------------------------------
// RSM1 parser
// ---------------------------------------------------------------------------

fn parse_rsm1(c: &mut Cursor<&[u8]>, version: u16) -> Result<RsmFile> {
    let anim_len = ri32(c)?;
    let shade_type = ShadeType::from_i32(ri32(c)?);
    let alpha = if version >= 0x0104 { ru8(c)? } else { 0xff };
    skip(c, 16)?; // unknown reserved bytes

    let texture_count = ri32(c)? as usize;
    let mut textures = Vec::with_capacity(texture_count);
    for _ in 0..texture_count {
        textures.push(read_fixed_string(c, 40)?);
    }

    let root_node_name = read_fixed_string(c, 40)?;

    let mesh_count = ri32(c)? as usize;
    let mut meshes = Vec::with_capacity(mesh_count);
    for _ in 0..mesh_count {
        meshes.push(parse_rsm1_mesh(c, version)?);
    }

    // Global translation keyframes (always 0 in practice).
    let keyframe_count = ri32(c)?;
    if keyframe_count != 0 {
        return Err(anyhow!("unexpected {} global keyframes", keyframe_count));
    }

    // Volume boxes — skip.
    let vol_count = ri32(c)?;
    if vol_count != 0 {
        log::warn!("[RoModel] {} volume box(es) in RSM1 file; skipping", vol_count);
    }

    let (bbmin, bbmax, bbrange) = compute_bounding_box(&meshes);

    let (major, minor) = (version >> 8, version & 0xff);
    log::info!(
        "[RoModel] RSM v{}.{} — {} mesh(es), {} texture(s), bb {:?}..{:?}",
        major, minor, meshes.len(), textures.len(), bbmin, bbmax
    );

    Ok(RsmFile {
        version,
        anim_len,
        shade_type,
        alpha,
        textures,
        root_node_name,
        meshes,
        bbmin,
        bbmax,
        bbrange,
    })
}

fn parse_rsm1_mesh(c: &mut Cursor<&[u8]>, version: u16) -> Result<RsmMesh> {
    let name = read_fixed_string(c, 40)?;
    let parent_name = read_fixed_string(c, 40)?;

    let tex_count = ri32(c)? as usize;
    let mut texture_indices = Vec::with_capacity(tex_count);
    for _ in 0..tex_count {
        texture_indices.push(ri32(c)?);
    }

    let offset = read_mat3(c)?;
    let pos_ = read_vec3(c)?;
    let pos = read_vec3(c)?;
    let rot_angle = rf32(c)?;
    let rot_axis = read_vec3(c)?;
    let scale = read_vec3(c)?;

    let vertex_count = ri32(c)? as usize;
    let mut vertices = Vec::with_capacity(vertex_count);
    for _ in 0..vertex_count {
        vertices.push(read_vec3(c)?);
    }

    let tc_count = ri32(c)? as usize;
    let mut tex_coords = Vec::with_capacity(tc_count);
    for _ in 0..tc_count {
        if version >= 0x0102 {
            skip(c, 4)?; // unused 3rd texture coordinate (always 0)
        }
        tex_coords.push([rf32(c)?, rf32(c)?]);
    }

    let face_count = ri32(c)? as usize;
    let mut faces = Vec::with_capacity(face_count);
    for _ in 0..face_count {
        faces.push(parse_rsm1_face(c)?);
    }

    let frame_count = ri32(c)? as usize;
    let mut frames = Vec::with_capacity(frame_count);
    for _ in 0..frame_count {
        let time = ri32(c)?;
        let x = rf32(c)?;
        let y = rf32(c)?;
        let z = rf32(c)?;
        let w = rf32(c)?;
        frames.push(RsmFrame { time, quaternion: [x, y, z, w] });
    }

    Ok(RsmMesh {
        name,
        parent_name,
        texture_indices,
        offset,
        pos_,
        pos,
        rot_angle,
        rot_axis,
        scale,
        vertices,
        tex_coords,
        faces,
        frames,
    })
}

fn parse_rsm1_face(c: &mut Cursor<&[u8]>) -> Result<RsmFace> {
    let vertex_ids = [ru16(c)?, ru16(c)?, ru16(c)?];
    let texcoord_ids = [ru16(c)?, ru16(c)?, ru16(c)?];
    let texture_id = ru16(c)?;
    let _padding = ru16(c)?;
    let two_sided = ri32(c)? != 0;
    let smooth_group = ri32(c)?;
    Ok(RsmFace { vertex_ids, texcoord_ids, texture_id, two_sided, smooth_group })
}

// ---------------------------------------------------------------------------
// RSM2 parser (v0x0203 only)
// ---------------------------------------------------------------------------

fn parse_rsm2(c: &mut Cursor<&[u8]>, version: u16) -> Result<RsmFile> {
    if version != 0x0203 {
        return Err(anyhow!("RSM2 v{:#06x} is not supported", version));
    }

    let anim_len = ri32(c)?;
    let shade_type = ShadeType::from_i32(ri32(c)?);
    let alpha = ru8(c)?;
    skip(c, 4)?; // unknown f32

    let root_mesh_count = ri32(c)?;
    if root_mesh_count != 1 {
        return Err(anyhow!("expected 1 root mesh, got {}", root_mesh_count));
    }
    let root_node_name = read_len_string(c)?;

    let mesh_count = ri32(c)? as usize;
    let mut textures: Vec<String> = Vec::new();
    let mut meshes = Vec::with_capacity(mesh_count);

    for _ in 0..mesh_count {
        meshes.push(parse_rsm2_mesh(c, &mut textures)?);
    }

    let (bbmin, bbmax, bbrange) = compute_bounding_box(&meshes);

    let (major, minor) = (version >> 8, version & 0xff);
    log::info!(
        "[RoModel] RSM v{}.{} — {} mesh(es), {} texture(s), bb {:?}..{:?}",
        major, minor, meshes.len(), textures.len(), bbmin, bbmax
    );

    Ok(RsmFile {
        version,
        anim_len,
        shade_type,
        alpha,
        textures,
        root_node_name,
        meshes,
        bbmin,
        bbmax,
        bbrange,
    })
}

fn parse_rsm2_mesh(c: &mut Cursor<&[u8]>, shared_textures: &mut Vec<String>) -> Result<RsmMesh> {
    let name = read_len_string(c)?;
    let parent_name = read_len_string(c)?;

    let tex_count = ri32(c)? as usize;
    let mut texture_indices = Vec::with_capacity(tex_count);
    for _ in 0..tex_count {
        let tex_name = read_len_string(c)?;
        let idx = match shared_textures.iter().position(|t| t == &tex_name) {
            Some(i) => i,
            None => {
                let i = shared_textures.len();
                shared_textures.push(tex_name);
                i
            }
        };
        texture_indices.push(idx as i32);
    }

    let offset = read_mat3(c)?;
    let pos = read_vec3(c)?;

    let vertex_count = ri32(c)? as usize;
    let mut vertices = Vec::with_capacity(vertex_count);
    for _ in 0..vertex_count {
        vertices.push(read_vec3(c)?);
    }

    // TexCoords stored as vec3; components y and z are u and v.
    let tc_count = ri32(c)? as usize;
    let mut tex_coords = Vec::with_capacity(tc_count);
    for _ in 0..tc_count {
        let _x = rf32(c)?;
        let u = rf32(c)?;
        let v = rf32(c)?;
        tex_coords.push([u, v]);
    }

    let face_count = ri32(c)? as usize;
    let mut faces = Vec::with_capacity(face_count);
    for _ in 0..face_count {
        let len = ri32(c)? as i64;
        let vertex_ids = [ru16(c)?, ru16(c)?, ru16(c)?];
        let texcoord_ids = [ru16(c)?, ru16(c)?, ru16(c)?];
        let texture_id = ru16(c)?;
        let _padding = ru16(c)?;
        let two_sided = ru16(c)? != 0;
        let smooth_group = ru16(c)? as i32;
        let _extra = ri32(c)?;
        // Skip any remaining bytes declared by len (24 bytes consumed above).
        let remaining = len - 24;
        if remaining > 0 {
            skip(c, remaining)?;
        }
        faces.push(RsmFace { vertex_ids, texcoord_ids, texture_id, two_sided, smooth_group });
    }

    // Position keyframes — skip.
    let pos_kf_count = ri32(c)? as usize;
    for _ in 0..pos_kf_count {
        skip(c, 4 + 12 + 4)?; // i32 frame + vec3 + i32 unknown
    }

    // Rotation keyframes — parse into frames (not used for static rendering).
    let rot_kf_count = ri32(c)? as usize;
    let mut frames = Vec::with_capacity(rot_kf_count);
    for _ in 0..rot_kf_count {
        let time = ri32(c)?;
        let x = rf32(c)?;
        let y = rf32(c)?;
        let z = rf32(c)?;
        let w = rf32(c)?;
        frames.push(RsmFrame { time, quaternion: [x, y, z, w] });
    }

    // Unknown section 1: count × 20 bytes.
    let unk1_count = ri32(c)? as usize;
    for _ in 0..unk1_count {
        skip(c, 20)?;
    }

    // Unknown section 2: nested structure.
    let unk2_outer = ri32(c)? as usize;
    for _ in 0..unk2_outer {
        skip(c, 4)?; // outer i32
        let unk2_inner = ri32(c)? as usize;
        for _ in 0..unk2_inner {
            skip(c, 4)?; // inner i32
            let unk2_leaf = ri32(c)? as usize;
            for _ in 0..unk2_leaf {
                skip(c, 4 + 4)?; // i32 + f32
            }
        }
    }

    Ok(RsmMesh {
        name,
        parent_name,
        texture_indices,
        offset,
        pos_: [0.0, 0.0, 0.0],
        pos,
        rot_angle: 0.0,
        rot_axis: [0.0, 1.0, 0.0],
        scale: [1.0, 1.0, 1.0],
        vertices,
        tex_coords,
        faces,
        frames,
    })
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Reads a 3×3 column-major matrix from the stream.
fn read_mat3(c: &mut Cursor<&[u8]>) -> Result<[[f32; 3]; 3]> {
    Ok([
        [rf32(c)?, rf32(c)?, rf32(c)?],
        [rf32(c)?, rf32(c)?, rf32(c)?],
        [rf32(c)?, rf32(c)?, rf32(c)?],
    ])
}

/// Applies the 3×3 column-major `offset` matrix to a vertex.
fn apply_mat3(m: &[[f32; 3]; 3], v: [f32; 3]) -> [f32; 3] {
    [
        m[0][0] * v[0] + m[1][0] * v[1] + m[2][0] * v[2],
        m[0][1] * v[0] + m[1][1] * v[1] + m[2][1] * v[2],
        m[0][2] * v[0] + m[1][2] * v[1] + m[2][2] * v[2],
    ]
}

/// Computes `bbmin`, `bbmax`, and `bbrange` in model space (before Y-flip).
///
/// For each mesh: apply `offset` to each vertex; if the mesh has a parent also add `pos + pos_`.
/// This matches browedit's `setBoundingBox` logic.
fn compute_bounding_box(meshes: &[RsmMesh]) -> ([f32; 3], [f32; 3], [f32; 3]) {
    let mut bbmin = [f32::MAX; 3];
    let mut bbmax = [f32::MIN; 3];

    for mesh in meshes {
        let has_parent = !mesh.parent_name.is_empty();
        for &v in &mesh.vertices {
            let mut p = apply_mat3(&mesh.offset, v);
            if has_parent {
                p[0] += mesh.pos[0] + mesh.pos_[0];
                p[1] += mesh.pos[1] + mesh.pos_[1];
                p[2] += mesh.pos[2] + mesh.pos_[2];
            }
            for i in 0..3 {
                bbmin[i] = bbmin[i].min(p[i]);
                bbmax[i] = bbmax[i].max(p[i]);
            }
        }
    }

    // Guard against empty models.
    if bbmin[0] == f32::MAX {
        bbmin = [0.0; 3];
        bbmax = [0.0; 3];
    }

    let bbrange = [
        (bbmin[0] + bbmax[0]) * 0.5,
        (bbmin[1] + bbmax[1]) * 0.5,
        (bbmin[2] + bbmax[2]) * 0.5,
    ];

    (bbmin, bbmax, bbrange)
}
