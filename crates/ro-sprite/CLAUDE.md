# ro-sprite

Cross-reference `zrenderer/` (D-lang) for SPR/ACT/IMF binary layouts and rendering behavior
when the ragnarokresearchlab spec is ambiguous.

---

Pure Rust crate; no Bevy. Parses SPR, ACT, and IMF files and renders sprite frames to pixel
buffers. Consumed by `bevy_ro_sprite`.

Public surface: `SprFile`, `ActFile`, `ImfFile`, `render_frame_tight`.

---

## Tight-Rendered Frames

Each logical frame is rendered to a **tight canvas**: the smallest pixel buffer that contains
all of the frame's transformed sprite layers. The canvas is cropped to the union bounding box
of all layers (after applying each layer's scale, rotation, and translation from the ACT), with
`pad` pixels of transparent border added on each side.

The **feet-origin** within this tight canvas is the pixel that corresponds to the character's
feet (coordinate `(0,0)` in ACT/feet space). It is stored as `RoAtlas::frame_origins[i]`.

`origin` is **not** the center-bottom of the canvas. It is computed as:

```
origin = (pad - min_x, pad - min_y)
```

where `min_x/min_y` are the minimum x/y extents of all transformed layer corners in
feet-origin space. For a centered layer with no ACT translation this approximates the
canvas center-bottom, but diverges whenever `layer.x != 0`, there is rotation or non-unit
scale, or multiple layers with different offsets are composited.

---

## Attach Points

ACT attach points are stored per logical frame in `RoAtlas::frame_attach_points: Vec<Option<IVec2>>`.
The coordinate system is **feet-origin, y-down**: `(0, 0)` is the character's feet;
negative y is above the feet.

---

## Frame Selection Across Layers

Each atlas builds its own flat frame sequence by iterating all included actions in order.
Sprites with different frame counts per action (e.g. body idle = 3 frames/direction, weapon
idle = 1 invisible frame/direction) produce flat sequences that diverge after the very first
action. By alert (action 32) or attack (action 80), the raw body flat index maps to a
completely wrong slot in the weapon atlas.

**Fix: tag-relative remapping.** For each non-body layer the plugin computes:

```
rel_frame    = current_frame - body_tag.start
mapped_frame = layer_tag.start + rel_frame   (clamped to layer_tag.end)
```

This is an identity for the body layer. `frame_origins` and `frame_attach_points` are
indexed with `mapped_frame` for consistency.

---

## Multi-Layer Compositing Formula

All layers share the body's feet as the canvas origin. For each non-body layer:

```
attach_offset  = anchor_attach_point - self_attach_point
canvas_top_left = canvas_feet + attach_offset - layer.origin
```

Where:
- `canvas_feet` = body's feet position in canvas pixel space (= `body.origin + overflow`)
- `attach_offset` = displacement of this layer's feet relative to the body's (zero for
  weapons/garments/headgear whose attach points match the body's)
- `layer.origin` = feet position within that layer's own tight-rendered frame

**Canvas bounds (body-anchored):**

```
content_min = min over non-body layers of (body.origin + attach_offset - layer.origin)
content_max = max over non-body layers of (above + layer.size_px)
// include body: content_min.min(Vec2::ZERO), content_max.max(body.size_px)
overflow    = (-content_min).max(Vec2::ZERO)
canvas_size = content_max + overflow
canvas_feet = body.origin + overflow
```

**Example:** Sprite A (body) = 50x100, feet-origin (24, 99), attach-point (0, -75);
Sprite B = 100x150, feet-origin (49, 149), attach-point (1, -70):

```
attach_offset   = (0,-75) - (1,-70) = (-1, -5)
B top-left      = (24,99) + (-1,-5) - (49,149) = (-26, -55)
```

B extends 26 px left, 24 px right, 55 px above A's top, 5 px below A's bottom.
Canvas = 100x155.
