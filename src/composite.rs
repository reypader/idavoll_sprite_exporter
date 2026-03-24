use crate::act::{ActFrame, ActSprite};
use crate::spr::SprFile;
use image::{Rgba, RgbaImage};

/// Composite all sprite layers of a frame onto a canvas.
/// `origin_x/y` is where sprite coordinate (0,0) maps to in the canvas.
pub fn render_frame(
    spr: &SprFile,
    frame: &ActFrame,
    canvas_w: u32,
    canvas_h: u32,
    origin_x: i32,
    origin_y: i32,
) -> RgbaImage {
    let mut canvas = RgbaImage::new(canvas_w, canvas_h);

    for layer in &frame.sprites {
        if layer.spr_id < 0 {
            continue;
        }
        let Some(img) = spr.get_image(layer.spr_id, layer.spr_type) else {
            continue;
        };
        if img.width == 0 || img.height == 0 {
            continue;
        }

        let rgba_bytes: Vec<u8> = img
            .pixels
            .iter()
            .flat_map(|c| [c.r, c.g, c.b, c.a])
            .collect();

        let Some(src) = RgbaImage::from_raw(img.width as u32, img.height as u32, rgba_bytes)
        else {
            continue;
        };

        blit_transformed(&mut canvas, &src, layer, origin_x, origin_y);
    }

    canvas
}

/// Compute the bounding box (min_x, min_y, max_x, max_y) in sprite-space
/// that covers all sprite layers across all frames in all actions.
/// Returns (min_x, min_y, max_x, max_y) or (0,0,0,0) if no sprites.
pub fn compute_bounds(spr: &SprFile, actions: &[crate::act::ActAction]) -> (i32, i32, i32, i32) {
    let mut min_x = i32::MAX;
    let mut min_y = i32::MAX;
    let mut max_x = i32::MIN;
    let mut max_y = i32::MIN;

    for action in actions {
        for frame in &action.frames {
            for layer in &frame.sprites {
                if layer.spr_id < 0 {
                    continue;
                }
                let Some(img) = spr.get_image(layer.spr_id, layer.spr_type) else {
                    continue;
                };

                let w = img.width as f32;
                let h = img.height as f32;
                let (sx, sy) = effective_scale(layer);
                let angle = (layer.rotation as f32).to_radians();
                let cos_a = angle.cos();
                let sin_a = angle.sin();
                let tx = layer.x as f32;
                let ty = layer.y as f32;

                for (u, v) in [
                    (-w / 2.0, -h / 2.0),
                    (w / 2.0, -h / 2.0),
                    (w / 2.0, h / 2.0),
                    (-w / 2.0, h / 2.0),
                ] {
                    let cx = (cos_a * sx * u - sin_a * sy * v + tx).round() as i32;
                    let cy = (sin_a * sx * u + cos_a * sy * v + ty).round() as i32;
                    min_x = min_x.min(cx);
                    min_y = min_y.min(cy);
                    max_x = max_x.max(cx);
                    max_y = max_y.max(cy);
                }
            }
        }
    }

    if min_x == i32::MAX {
        (0, 0, 64, 64)
    } else {
        (min_x, min_y, max_x, max_y)
    }
}

fn blit_transformed(
    canvas: &mut RgbaImage,
    src: &RgbaImage,
    layer: &ActSprite,
    origin_x: i32,
    origin_y: i32,
) {
    let w = src.width() as f32;
    let h = src.height() as f32;

    let (sx, sy) = effective_scale(layer);
    if sx.abs() < 1e-6 || sy.abs() < 1e-6 {
        return;
    }

    let angle = (layer.rotation as f32).to_radians();
    let cos_a = angle.cos();
    let sin_a = angle.sin();

    // Sprite-space position → canvas coords
    let tx = origin_x as f32 + layer.x as f32;
    let ty = origin_y as f32 + layer.y as f32;

    // Bounding box of the transformed sprite in canvas space
    let (min_x, min_y, max_x, max_y) = transformed_bounds((w, h), (sx, sy), (cos_a, sin_a), (tx, ty));

    let cw = canvas.width() as i32;
    let ch = canvas.height() as i32;

    let [tr, tg, tb, ta] = layer.tint.map(|v| v as f32 / 255.0);

    for py in min_y..=max_y {
        if py < 0 || py >= ch {
            continue;
        }
        for px in min_x..=max_x {
            if px < 0 || px >= cw {
                continue;
            }

            // Inverse transform: canvas pixel → sprite-local texel
            let dx = px as f32 - tx;
            let dy = py as f32 - ty;

            // Inverse of: x = cos*sx*u' - sin*sy*v', y = sin*sx*u' + cos*sy*v'
            let u = (cos_a * dx + sin_a * dy) / sx + w / 2.0;
            let v = (-sin_a * dx + cos_a * dy) / sy + h / 2.0;

            let ui = u.round() as i32;
            let vi = v.round() as i32;

            if ui < 0 || vi < 0 || ui >= w as i32 || vi >= h as i32 {
                continue;
            }

            let src_px = src.get_pixel(ui as u32, vi as u32).0;
            if src_px[3] == 0 {
                continue;
            }

            // Multiplicative tint
            let r = (src_px[0] as f32 * tr) as u8;
            let g = (src_px[1] as f32 * tg) as u8;
            let b = (src_px[2] as f32 * tb) as u8;
            let a = (src_px[3] as f32 * ta) as u8;

            if a == 0 {
                continue;
            }

            let dst = canvas.get_pixel(px as u32, py as u32).0;
            canvas.put_pixel(px as u32, py as u32, Rgba(alpha_over([r, g, b, a], dst)));
        }
    }
}

fn effective_scale(layer: &ActSprite) -> (f32, f32) {
    let flip = if layer.flags & 1 != 0 { -1.0f32 } else { 1.0f32 };
    (layer.x_scale * flip, layer.y_scale)
}

fn transformed_bounds(
    (w, h): (f32, f32),
    (sx, sy): (f32, f32),
    (cos_a, sin_a): (f32, f32),
    (tx, ty): (f32, f32),
) -> (i32, i32, i32, i32) {
    let corners = [
        (-w / 2.0, -h / 2.0),
        (w / 2.0, -h / 2.0),
        (w / 2.0, h / 2.0),
        (-w / 2.0, h / 2.0),
    ];
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    for (u, v) in corners {
        let cx = cos_a * sx * u - sin_a * sy * v + tx;
        let cy = sin_a * sx * u + cos_a * sy * v + ty;
        min_x = min_x.min(cx);
        min_y = min_y.min(cy);
        max_x = max_x.max(cx);
        max_y = max_y.max(cy);
    }
    (
        min_x.floor() as i32,
        min_y.floor() as i32,
        max_x.ceil() as i32,
        max_y.ceil() as i32,
    )
}

fn alpha_over(src: [u8; 4], dst: [u8; 4]) -> [u8; 4] {
    let sa = src[3] as f32 / 255.0;
    let da = dst[3] as f32 / 255.0;
    let out_a = sa + da * (1.0 - sa);
    if out_a < 1e-6 {
        return [0, 0, 0, 0];
    }
    let blend = |s: u8, d: u8| -> u8 {
        ((s as f32 * sa + d as f32 * da * (1.0 - sa)) / out_a).round() as u8
    };
    [blend(src[0], dst[0]), blend(src[1], dst[1]), blend(src[2], dst[2]), (out_a * 255.0).round() as u8]
}
