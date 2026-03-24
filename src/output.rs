use std::path::Path;

use anyhow::Result;
use image::RgbaImage;
use serde::Serialize;

use crate::act::ActFile;
use crate::composite::{compute_bounds, render_frame};
use crate::imf::ImfFile;
use crate::spr::SprFile;
use crate::zorder::{z_order, SpriteKind};

// ---------------------------------------------------------------------------
// Aseprite JSON types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct Rect {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

#[derive(Serialize)]
struct AseFrame {
    filename: String,
    frame: Rect,
    rotated: bool,
    trimmed: bool,
    #[serde(rename = "spriteSourceSize")]
    sprite_source_size: Rect,
    #[serde(rename = "sourceSize")]
    source_size: AseSize,
    duration: u32, // milliseconds
    #[serde(rename = "zOrder", skip_serializing_if = "Option::is_none")]
    z_order: Option<i32>,
}

#[derive(Serialize)]
struct AseSize {
    w: u32,
    h: u32,
}

#[derive(Serialize)]
struct FrameTag {
    name: String,
    from: u32,
    to: u32,
    direction: String,
}

#[derive(Serialize)]
struct AseMeta {
    app: String,
    version: String,
    image: String,
    format: String,
    size: AseSize,
    scale: String,
    #[serde(rename = "frameTags")]
    frame_tags: Vec<FrameTag>,
}

#[derive(Serialize)]
struct AseSheet {
    frames: Vec<AseFrame>,
    meta: AseMeta,
}

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
    // Where sprite (0,0) lands on the canvas
    let origin_x = pad - min_x;
    let origin_y = pad - min_y;

    // Count total frames
    let total_frames: usize = action_indices
        .iter()
        .map(|&i| act.actions[i].frames.len())
        .sum();

    if total_frames == 0 {
        println!("No frames found.");
        return Ok(());
    }

    // Build the full spritesheet (single horizontal strip)
    let sheet_w = canvas_w * total_frames as u32;
    let sheet_h = canvas_h;
    let mut sheet = RgbaImage::new(sheet_w, sheet_h);

    let mut ase_frames: Vec<AseFrame> = Vec::with_capacity(total_frames);
    let mut frame_tags: Vec<FrameTag> = Vec::new();
    let mut global_frame_idx: u32 = 0;

    for &action_idx in &action_indices {
        let action = &act.actions[action_idx];
        let tag_start = global_frame_idx;

        for (frame_idx, frame) in action.frames.iter().enumerate() {
            let rendered = render_frame(spr, frame, canvas_w, canvas_h, origin_x, origin_y);

            // Blit rendered frame into spritesheet
            let x_offset = global_frame_idx * canvas_w;
            for y in 0..canvas_h {
                for x in 0..canvas_w {
                    sheet.put_pixel(x_offset + x, y, *rendered.get_pixel(x, y));
                }
            }

            let frame_z_order = sprite_kind
                .map(|kind| z_order(kind, action_idx, frame_idx, imf));

            let label = format!("{base_name}_a{action_idx:03} {frame_idx}");
            ase_frames.push(AseFrame {
                filename: label,
                frame: Rect { x: x_offset, y: 0, w: canvas_w, h: canvas_h },
                rotated: false,
                trimmed: false,
                sprite_source_size: Rect { x: 0, y: 0, w: canvas_w, h: canvas_h },
                source_size: AseSize { w: canvas_w, h: canvas_h },
                duration: action.frame_ms().max(1),
                z_order: frame_z_order,
            });

            global_frame_idx += 1;
        }

        let tag_end = global_frame_idx - 1;
        frame_tags.push(FrameTag {
            name: action_label(action_idx, act.actions.len()),
            from: tag_start,
            to: tag_end,
            direction: "forward".to_string(),
        });
    }

    // Save spritesheet PNG
    let png_name = format!("{base_name}.png");
    let png_path = out_dir.join(&png_name);
    sheet.save(&png_path)?;
    println!("Wrote {}", png_path.display());

    // Save Aseprite JSON
    let ase_sheet = AseSheet {
        frames: ase_frames,
        meta: AseMeta {
            app: "act-spr-convert".to_string(),
            version: "1.0".to_string(),
            image: png_name,
            format: "RGBA8888".to_string(),
            size: AseSize { w: sheet_w, h: sheet_h },
            scale: "1".to_string(),
            frame_tags,
        },
    };

    let json_path = out_dir.join(format!("{base_name}.json"));
    let json = serde_json::to_string_pretty(&ase_sheet)?;
    std::fs::write(&json_path, json)?;
    println!("Wrote {}", json_path.display());

    println!(
        "Canvas: {canvas_w}×{canvas_h}px, origin at ({origin_x},{origin_y}), \
         {total_frames} frames across {} actions",
        action_indices.len()
    );

    Ok(())
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
