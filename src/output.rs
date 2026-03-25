use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::Path;

use anyhow::Result;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use image::RgbaImage;

use crate::act::ActFile;
use crate::composite::{compute_bounds, render_frame};
use crate::imf::ImfFile;
use crate::spr::SprFile;
use crate::zorder::{z_order, SpriteKind};

// ---------------------------------------------------------------------------
// ASE chunk/frame constants
// ---------------------------------------------------------------------------

const ASE_MAGIC: u16 = 0xA5E0;
const FRAME_MAGIC: u16 = 0xF1FA;
const CHUNK_LAYER: u16 = 0x2004;
const CHUNK_CEL: u16 = 0x2005;
const CHUNK_COLOR_PROF: u16 = 0x2007;
const CHUNK_TAGS: u16 = 0x2018;
const CHUNK_USERDATA: u16 = 0x2020;

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

pub fn export(
    spr: &SprFile,
    act: &ActFile,
    base_name: &str,
    out_dir: &Path,
    action_filter: Option<&[usize]>,
    sprite_kind: Option<&SpriteKind>,
    imf: Option<&ImfFile>,
) -> Result<()> {
    // Determine which actions to export
    let action_indices: Vec<usize> = match action_filter {
        Some(filter) => filter
            .iter()
            .copied()
            .filter(|&i| i < act.actions.len())
            .collect(),
        None => (0..act.actions.len()).collect(),
    };

    if action_indices.is_empty() {
        println!("No actions to export.");
        return Ok(());
    }

    // Compute a shared canvas size from the bounding box across all exported actions
    let selected_actions: Vec<_> = action_indices.iter().map(|&i| &act.actions[i]).collect();
    let all_actions_slice: Vec<_> = selected_actions.iter().map(|a| (*a).clone()).collect();
    let (min_x, min_y, max_x, max_y) = compute_bounds(spr, &all_actions_slice);

    let pad = 4i32;
    let canvas_w = ((max_x - min_x) + pad * 2).max(1) as u32;
    let canvas_h = ((max_y - min_y) + pad * 2).max(1) as u32;
    let origin_x = pad - min_x;
    let origin_y = pad - min_y;

    // Count total logical frames
    let total_frames: usize = action_indices
        .iter()
        .map(|&i| act.actions[i].frames.len())
        .sum();

    if total_frames == 0 {
        println!("No frames found.");
        return Ok(());
    }

    // Pass 1: render every logical frame and deduplicate by pixel content.
    // Also collect per-frame duration, z-order, and the first ASE frame index
    // where each unique render first appears (for LinkedCel references).
    let mut unique_renders: Vec<RgbaImage> = Vec::new();
    let mut hash_to_strip: HashMap<u64, u32> = HashMap::new();
    let mut strip_indices: Vec<u32> = Vec::with_capacity(total_frames);
    let mut frame_durations: Vec<u32> = Vec::with_capacity(total_frames);
    let mut frame_z_orders: Vec<Option<i32>> = Vec::with_capacity(total_frames);
    // strip_idx → first logical frame index that uses it
    let mut first_ase_frame_for_strip: Vec<u32> = Vec::new();

    for &action_idx in &action_indices {
        let action = &act.actions[action_idx];
        for (frame_idx, frame) in action.frames.iter().enumerate() {
            let flat_idx = strip_indices.len();
            let rendered = render_frame(spr, frame, canvas_w, canvas_h, origin_x, origin_y);
            let h = hash_pixels(rendered.as_raw());
            let strip_idx = if let Some(&idx) = hash_to_strip.get(&h) {
                idx
            } else {
                let idx = unique_renders.len() as u32;
                hash_to_strip.insert(h, idx);
                first_ase_frame_for_strip.push(flat_idx as u32);
                unique_renders.push(rendered);
                idx
            };
            strip_indices.push(strip_idx);
            frame_durations.push(action.frame_ms().max(1));
            frame_z_orders.push(sprite_kind.map(|kind| z_order(kind, action_idx, frame_idx, imf)));
        }
    }

    // Pass 2: build action tags and ASE frame bytes.
    let mut ase_tags: Vec<(String, u32, u32)> = Vec::new(); // (name, from, to)
    let mut global_frame_idx: u32 = 0;

    for &action_idx in &action_indices {
        let action = &act.actions[action_idx];
        let tag_start = global_frame_idx;
        global_frame_idx += action.frames.len() as u32;
        ase_tags.push((
            action_label(action_idx, act.actions.len()),
            tag_start,
            global_frame_idx - 1,
        ));
    }

    let unique_count = unique_renders.len() as u32;

    // Pass 3: build ASE frame bytes (one per logical frame).
    let mut ase_frames: Vec<Vec<u8>> = Vec::with_capacity(total_frames);

    for (i, &strip_idx) in strip_indices.iter().enumerate() {
        let mut chunks: Vec<Vec<u8>> = Vec::new();

        // Frame 0 carries the layer, color profile, and tags setup chunks.
        if i == 0 {
            chunks.push(format_chunk(CHUNK_LAYER, build_layer_chunk_data()));
            chunks.push(format_chunk(CHUNK_COLOR_PROF, build_color_profile_chunk_data()));
            chunks.push(format_chunk(CHUNK_TAGS, build_tags_chunk_data(&ase_tags)));
        }

        // Cel: CompressedImage on first appearance, LinkedCel for duplicates.
        let first_frame = first_ase_frame_for_strip[strip_idx as usize];
        if first_frame == i as u32 {
            let pixels = unique_renders[strip_idx as usize].as_raw();
            chunks.push(format_chunk(
                CHUNK_CEL,
                build_cel_compressed_data(canvas_w as u16, canvas_h as u16, pixels)?,
            ));
        } else {
            chunks.push(format_chunk(
                CHUNK_CEL,
                build_cel_linked_data(first_frame as u16),
            ));
        }

        // UserData for z-order (only when sprite_kind was provided).
        if let Some(z) = frame_z_orders[i] {
            chunks.push(format_chunk(
                CHUNK_USERDATA,
                build_userdata_text_data(&format!("zOrder:{z}")),
            ));
        }

        ase_frames.push(build_ase_frame(&chunks, frame_durations[i] as u16));
    }

    // Write .aseprite file
    let ase_path = out_dir.join(format!("{base_name}.aseprite"));
    write_aseprite_file(&ase_path, canvas_w as u16, canvas_h as u16, total_frames as u16, &ase_frames)?;
    println!("Wrote {}", ase_path.display());

    println!(
        "Canvas: {canvas_w}×{canvas_h}px, origin at ({origin_x},{origin_y}), \
         {} actions, {total_frames} logical frames → {unique_count} unique (linked cels)",
        action_indices.len()
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// ASE file-level writer
// ---------------------------------------------------------------------------

fn write_aseprite_file(
    path: &Path,
    canvas_w: u16,
    canvas_h: u16,
    total_frames: u16,
    frames: &[Vec<u8>],
) -> Result<()> {
    let frames_total: usize = frames.iter().map(|f| f.len()).sum();
    let file_size = 128u32 + frames_total as u32;

    let mut buf = Vec::with_capacity(file_size as usize);

    // Header (128 bytes)
    push_u32(&mut buf, file_size);        // file size
    push_u16(&mut buf, ASE_MAGIC);        // magic
    push_u16(&mut buf, total_frames);     // frame count
    push_u16(&mut buf, canvas_w);         // canvas width
    push_u16(&mut buf, canvas_h);         // canvas height
    push_u16(&mut buf, 32);               // color depth = RGBA
    push_u32(&mut buf, 1);                // flags = 1 (layer opacity valid)
    push_u16(&mut buf, 100);              // speed (deprecated)
    push_u32(&mut buf, 0);               // reserved
    push_u32(&mut buf, 0);               // reserved
    buf.push(0);                          // transparent palette entry
    buf.extend_from_slice(&[0u8; 3]);     // ignored
    push_u16(&mut buf, 0);               // num_colors (0 = 256 for indexed; N/A for RGBA)
    buf.push(1);                          // pixel_width
    buf.push(1);                          // pixel_height
    push_i16(&mut buf, 0);               // grid_x
    push_i16(&mut buf, 0);               // grid_y
    push_u16(&mut buf, 0);               // grid_width
    push_u16(&mut buf, 0);               // grid_height
    buf.extend_from_slice(&[0u8; 84]);    // reserved (pads header to 128 bytes)

    for frame in frames {
        buf.extend_from_slice(frame);
    }

    std::fs::write(path, &buf)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Frame builder
// ---------------------------------------------------------------------------

fn build_ase_frame(chunks: &[Vec<u8>], duration_ms: u16) -> Vec<u8> {
    let chunks_total: usize = chunks.iter().map(|c| c.len()).sum();
    let frame_size = 16u32 + chunks_total as u32;
    let num_chunks = chunks.len() as u16;

    let mut buf = Vec::with_capacity(frame_size as usize);
    push_u32(&mut buf, frame_size);   // byte count
    push_u16(&mut buf, FRAME_MAGIC);  // magic
    push_u16(&mut buf, num_chunks);   // old chunk count field
    push_u16(&mut buf, duration_ms);  // frame duration
    buf.extend_from_slice(&[0u8; 2]); // reserved
    push_u32(&mut buf, 0);            // new chunk count (0 = use old field)
    for chunk in chunks {
        buf.extend_from_slice(chunk);
    }
    buf
}

// ---------------------------------------------------------------------------
// Chunk wrapper
// ---------------------------------------------------------------------------

fn format_chunk(chunk_type: u16, data: Vec<u8>) -> Vec<u8> {
    let size = 6u32 + data.len() as u32;
    let mut buf = Vec::with_capacity(size as usize);
    push_u32(&mut buf, size);
    push_u16(&mut buf, chunk_type);
    buf.extend_from_slice(&data);
    buf
}

// ---------------------------------------------------------------------------
// Chunk payload builders
// ---------------------------------------------------------------------------

/// Layer chunk (0x2004) — single "Sprite" layer, Normal blend, full opacity.
fn build_layer_chunk_data() -> Vec<u8> {
    let mut d = Vec::new();
    push_u16(&mut d, 3);    // flags: Visible=1 | Editable=2
    push_u16(&mut d, 0);    // layer type = Normal
    push_u16(&mut d, 0);    // child level
    push_u16(&mut d, 0);    // default width (ignored)
    push_u16(&mut d, 0);    // default height (ignored)
    push_u16(&mut d, 0);    // blend mode = Normal
    d.push(255);             // opacity
    d.extend_from_slice(&[0u8; 3]); // reserved
    push_string(&mut d, "Sprite");
    d
}

/// Color profile chunk (0x2007) — sRGB, no fixed gamma.
fn build_color_profile_chunk_data() -> Vec<u8> {
    let mut d = Vec::new();
    push_u16(&mut d, 1); // type = sRGB
    push_u16(&mut d, 0); // flags (0 = no fixed gamma)
    push_u32(&mut d, 0); // fixed gamma (unused when flags=0)
    d.extend_from_slice(&[0u8; 8]); // reserved
    d
}

/// FrameTags chunk (0x2018).
fn build_tags_chunk_data(tags: &[(String, u32, u32)]) -> Vec<u8> {
    let mut d = Vec::new();
    push_u16(&mut d, tags.len() as u16);
    d.extend_from_slice(&[0u8; 8]); // reserved
    for (name, from, to) in tags {
        push_u16(&mut d, *from as u16);
        push_u16(&mut d, *to as u16);
        d.push(0);                          // loop direction = Forward
        push_u16(&mut d, 0);               // repeat (0 = loop forever)
        d.extend_from_slice(&[0u8; 6]);     // reserved
        d.extend_from_slice(&[0u8; 3]);     // deprecated RGB color
        d.push(0);                          // extra byte
        push_string(&mut d, name);
    }
    d
}

/// Cel chunk payload — CompressedImage (type 2).
fn build_cel_compressed_data(width: u16, height: u16, pixels: &[u8]) -> Result<Vec<u8>> {
    let mut d = Vec::new();
    // Cel chunk header (16 bytes)
    push_u16(&mut d, 0);   // layer index
    push_i16(&mut d, 0);   // x position
    push_i16(&mut d, 0);   // y position
    d.push(255);            // opacity
    push_u16(&mut d, 2);   // cel type = CompressedImage
    push_i16(&mut d, 0);   // z-index (Aseprite-internal; not our z-order)
    d.extend_from_slice(&[0u8; 5]); // reserved
    // Image dimensions
    push_u16(&mut d, width);
    push_u16(&mut d, height);
    // zlib-compressed RGBA pixels (row-major, top-to-bottom, R G B A per pixel)
    let mut enc = ZlibEncoder::new(Vec::new(), Compression::new(6));
    enc.write_all(pixels)?;
    let compressed = enc.finish()?;
    d.extend_from_slice(&compressed);
    Ok(d)
}

/// Cel chunk payload — LinkedCel (type 1).
fn build_cel_linked_data(link_frame: u16) -> Vec<u8> {
    let mut d = Vec::new();
    // Cel chunk header (16 bytes)
    push_u16(&mut d, 0);   // layer index
    push_i16(&mut d, 0);   // x position
    push_i16(&mut d, 0);   // y position
    d.push(255);            // opacity
    push_u16(&mut d, 1);   // cel type = LinkedCel
    push_i16(&mut d, 0);   // z-index
    d.extend_from_slice(&[0u8; 5]); // reserved
    // Linked cel data
    push_u16(&mut d, link_frame);
    d
}

/// UserData chunk payload (0x2020) — text only.
fn build_userdata_text_data(text: &str) -> Vec<u8> {
    let mut d = Vec::new();
    push_u32(&mut d, 1); // flags = 1 (has text)
    push_string(&mut d, text);
    d
}

// ---------------------------------------------------------------------------
// Low-level byte helpers
// ---------------------------------------------------------------------------

#[inline]
fn push_u16(buf: &mut Vec<u8>, v: u16) {
    buf.extend_from_slice(&v.to_le_bytes());
}

#[inline]
fn push_i16(buf: &mut Vec<u8>, v: i16) {
    buf.extend_from_slice(&v.to_le_bytes());
}

#[inline]
fn push_u32(buf: &mut Vec<u8>, v: u32) {
    buf.extend_from_slice(&v.to_le_bytes());
}

/// ASE STRING = WORD (byte length) + UTF-8 bytes (no null terminator).
fn push_string(buf: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    push_u16(buf, bytes.len() as u16);
    buf.extend_from_slice(bytes);
}

// ---------------------------------------------------------------------------
// Pixel hashing + action label (unchanged)
// ---------------------------------------------------------------------------

fn hash_pixels(bytes: &[u8]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

/// Human-readable action label. Uses monster labels for ACTs with ≤40 actions (multiples of 8),
/// otherwise falls back to player labels.
fn action_label(idx: usize, total_actions: usize) -> String {
    const PLAYER_BASES: &[(usize, &str)] = &[
        (0, "stand"),
        (8, "walk"),
        (16, "sit"),
        (24, "pickup"),
        (32, "atk_wait"),
        (40, "attack"),
        (48, "damage"),
        (56, "damage2"),
        (64, "dead"),
        (72, "unk"),
        (80, "attack2"),
        (88, "attack3"),
        (96, "skill"),
    ];
    const MONSTER_BASES: &[(usize, &str)] = &[
        (0, "stand"),
        (8, "move"),
        (16, "attack"),
        (24, "damage"),
        (32, "dead"),
    ];
    const DIRS: &[&str] = &["s", "sw", "w", "nw", "n", "ne", "e", "se"];

    let base = idx - (idx % 8);
    let dir = idx % 8;
    let bases: &[(usize, &str)] = if total_actions != 104 && total_actions.is_multiple_of(8)
        && total_actions <= 40
    {
        MONSTER_BASES
    } else {
        PLAYER_BASES
    };

    if let Some(&(_, name)) = bases.iter().find(|&&(b, _)| b == base) {
        format!("{}_{}", name, DIRS[dir])
    } else {
        format!("action_{idx:03}_{}", DIRS[dir])
    }
}
