# bevy_ro_sprite

Bevy integration for RO sprites. Depends on `ro-sprite` for all parsing and frame rendering.
Bevy is only in this crate, not in `ro-sprite`.

---

## Feature Flags

| Flag | Default | Effect |
|---|---|---|
| `3d` | off | Enables `composite` module: `RoComposite`, `RoCompositeMaterial`, `RoCompositePlugin`, `SpriteRole`, `CompositeLayerDef` |

Without `3d`, only the 2D `RoAnimation` system and the `RoAtlas` asset loader are compiled.

---

## 2D Animation (non-composite)

For simple sprites that don't need compositing (items, NPCs, effects), use `RoAnimation` with
a 2D `Sprite`:

```rust
commands.spawn((
    RoAnimation {
        atlas: server.load("my_sprite.spr"),
        animation: RoAnimationControl::tag("idle_s"),
    },
    Sprite::default(),
));
```

`update_ro_animation` (registered by `RoAnimationPlugin`) advances
`RoAnimationState::current_frame` each tick and writes the atlas index to the `Sprite`.
`AnimationRepeat::Loop` (default) or `AnimationRepeat::Count(n)` controls looping.
`SpriteFrameEvent` is also emitted from this path.

---

## Plugin Responsibilities vs User Responsibilities

**Plugin (`RoCompositePlugin`) handles:**
- Asset loading: `RoAtlas` (from `.spr` + `.act` + optional `.imf`)
- Animation: advancing `RoComposite::current_frame` each tick
- Material update: rebuilding `RoCompositeMaterial` uniforms (canvas layout, layer UVs)
- Billboard orientation: `orient_billboard` keeps the quad facing the camera
- Billboard positioning: sets `Transform::translation` so the canvas feet pixel lands at the actor's world position

**User code handles:**
- Spawning a child entity with `RoComposite` + `Mesh3d(Rectangle)` + `MeshMaterial3d(RoCompositeMaterial)` as a child of the actor
- Writing a system that maps game state to `RoComposite::tag` + `RoComposite::playing`
- Using `direction_index(facing, cam_fwd)` and `composite_tag(action, dir)` from the plugin prelude to build the tag string

---

## Z-Order / Draw Order

Z-order is computed per-frame from each layer's `SpriteRole` and the current direction.
Reference: `zrenderer/source/sprite.d`. Users assign a `SpriteRole` to each
`CompositeLayerDef`; they do not set raw z-order values.

### Direction groups

```
topLeft     = direction in {w, nw, n, ne}
bottomRight = direction in {s, sw, e, se}
```

### Z-order table (lower = drawn first / behind)

| SpriteRole | topLeft | bottomRight |
|---|---|---|
| `Shadow` | -1 | -1 |
| `Shield` | 10 | 30 |
| `Body` | 15 | 10 |
| `Head` (normal) | 20 | 15 |
| `Head` (IMF behind) | 14 | 9 |
| `Headgear { slot: 0 }` | 22 | 17 |
| `Headgear { slot: 1 }` | 23 | 18 |
| `Headgear { slot: 2 }` | 24 | 19 |
| `Headgear { slot: 3 }` | 25 | 20 |
| `Weapon { slot: 0 }` | 28 | 23 |
| `Weapon { slot: 1 }` | 29 | 24 |
| `Garment` | 35 | 35 |

`Garment` z-order is driven by Lua tables in the full RO client (`_New_DrawOnTop`,
`IsTopLayer`). 35 (always-on-top) is used as a safe default.

### IMF integration

`priority(layer=1, action, frame) == 1` in the IMF file signals the head renders behind
the body for that frame. The plugin reads this from `RoAtlas::frame_head_behind` and passes
`head_behind = true` to `SpriteRole::Head::z_order()`, dropping head z-order from 20 to 14
(topLeft) or 15 to 9 (bottomRight).

---

## ACT Frame Events

When a frame with an ACT event string is reached, the plugin triggers `SpriteFrameEvent`.

```rust
// Global observer
app.add_observer(|trigger: On<SpriteFrameEvent>| {
    let e = trigger.event();
    // e.entity, e.event (e.g. "atk"), e.tag (e.g. "attack1_s")
});

// Entity-specific observer
commands.spawn(RoComposite { ... })
    .observe(|trigger: On<SpriteFrameEvent>| { ... });
```

Events are read from the **body atlas** only. They fire on every frame advance (including
loop wrap-around) but not on tag changes that reset without advancing.

ACT event strings are in `RoAtlas::frame_events: Vec<Option<String>>`.

---

## Billboard Positioning

The billboard quad (1x1 `Rectangle`, scaled to `canvas_size`) faces the camera via `look_at`.
To place the **canvas feet pixel** at the actor's world position:

```
local_x = canvas_feet.x - canvas_size.x / 2
local_y = canvas_size.y / 2 - canvas_feet.y

transform.translation = -local_x * cam_right - local_y * cam_up
```

`cam_right` and `cam_up` are from `GlobalTransform::right()` / `GlobalTransform::up()` in
world space. Setting `translation.x` (world X) directly is wrong for non-north cameras; this
formula handles all camera orientations.

---

## Public Utilities

| Symbol | Description |
|---|---|
| `composite_tag(action, dir)` | Builds `"idle_n"` style tag string |
| `direction_index(facing, cam_fwd)` | Maps XZ facing + camera forward to 0-7 direction index (0=s, clockwise) |
| `orient_billboard` | System: keeps `RoComposite` quads facing the camera (registered by `RoCompositePlugin`) |

---

## Key Data Types

| Type | Description |
|---|---|
| `RoAtlas` | Asset loaded from `.spr`. Contains atlas image/layout, per-frame durations, origins, attach points, head-behind flags, ACT event strings, tags. |
| `RoAnimation` | Component for 2D sprites. `atlas: Handle<RoAtlas>`, `animation: RoAnimationControl`. |
| `RoAnimationControl` | `tag`, `playing`, `speed`, `repeat: AnimationRepeat`. |
| `RoAnimationState` | Runtime state: `current_frame: u16`, `elapsed: Duration`. Added automatically via `#[require]`. |
| `SpriteFrameEvent` | `EntityEvent` triggered when a frame with an ACT event string is reached. Fields: `entity`, `event: String`, `tag: Option<String>`. |
| `RoComposite` | Component on the billboard entity. User sets `layers`, `tag`, `playing`, `speed`. |
| `CompositeLayerDef` | `{ atlas: Handle<RoAtlas>, role: SpriteRole }`. `SpriteRole::Body` is the compositing anchor. |
| `SpriteRole` | Enum: `Shadow / Body / Head / Headgear { slot } / Weapon { slot } / Shield / Garment`. Drives direction-dependent z-order. |
| `RoCompositeMaterial` | Custom `Material` with a texture binding array + storage buffer of layer uniforms. Up to `MAX_LAYERS = 8` layers. |
