# bevy_ro_sprite

Native Bevy plugin for Ragnarok Online ACT/SPR/IMF sprites.

Loads `.spr` + `.act` + `.imf` files directly as Bevy assets — no pre-export step required.
Supports both 2D sprite animation and 3D composited billboard rendering (body + head + weapon
layered onto a single quad).

Compatible with **Bevy 0.18**.

---

## Workspace

```
bevy_ro_sprite/
  crates/
    ro-sprite/          # Pure Rust parser: ACT, SPR, IMF, composite renderer (no Bevy)
    bevy_ro_sprite/     # Bevy plugin
```

`ro-sprite` is usable standalone if you only need to parse or render frames offline.

---

## Cargo.toml

```toml
[dependencies]
# 2D sprite animation only
bevy_ro_sprite = { path = "crates/bevy_ro_sprite" }

# 3D composited billboards (body + head + weapon on one quad)
bevy_ro_sprite = { path = "crates/bevy_ro_sprite", features = ["3d"] }
```

---

## Asset conventions

The loader is registered for `.spr` files. It expects a `.act` file at the same path, and
optionally a `.imf` at the same path. Place all three in your Bevy `assets/` folder:

```
assets/
  female_knight.spr
  female_knight.act
  female_knight.imf      # optional — drives head-behind-body z-order
  female_head1.spr
  female_head1.act
  female_knight_spear_weapon.spr
  female_knight_spear_weapon.act
```

Load with `AssetServer::load`:

```rust
let atlas: Handle<RoAtlas> = server.load("female_knight.spr");
```

---

## 2D sprite animation

For a single-layer sprite (monster, NPC, item drop):

```rust
use bevy::prelude::*;
use bevy_ro_sprite::prelude::*;

fn setup(mut commands: Commands, server: Res<AssetServer>) {
    commands.spawn((
        RoAnimation {
            atlas: server.load("orc_warrior.spr"),
            animation: RoAnimationControl::tag("idle_s"),
        },
        Sprite::default(),
        Transform::default(),
    ));
}
```

`RoAnimation` requires `RoAnimationState` (inserted automatically). The plugin drives the
`Sprite` texture atlas each frame. To switch animations, mutate `animation.tag`:

```rust
fn on_attack(mut q: Query<&mut RoAnimation, With<MyMonster>>) {
    for mut anim in &mut q {
        anim.animation.tag = Some("attack1_s".to_string());
    }
}
```

### Tag format

Tags follow `"{action}_{direction}"`, e.g. `"idle_s"`, `"walk_nw"`, `"attack1_e"`.

Player/human sprites (104 actions) use:

| Action   | Directions  |
|----------|-------------|
| `idle`   | s sw w nw n ne e se |
| `walk`   | s sw w nw n ne e se |
| `sit`    | … |
| `pickup`, `alert`, `skill`, `flinch`, `frozen`, `dead`, `attack1`, `attack2`, `spell` | … |

Monster sprites (40 actions) use: `idle`, `walk`, `attack1`, `flinch`, `dead`.

### Custom render targets

Implement `RenderAnimation` to drive any component type:

```rust
impl RenderAnimation for MyMaterial {
    type Extra<'e> = ResMut<'e, Assets<MyMaterial>>;
    fn render_animation(&mut self, atlas: &RoAtlas, state: &RoAnimationState, _extra: &mut ()) {
        // update self from atlas + state
    }
}

// Register the render system:
app.add_systems(PostUpdate, render_animation::<MyMaterial>);
```

---

## 3D composited billboard (player characters)

Player characters composite body + head + headgear + weapon onto a single billboard quad that
always faces the camera. Enable the `3d` feature for this.

### Plugin setup

```rust
use bevy_ro_sprite::prelude::*;

app.add_plugins(RoSpritePlugin); // includes RoCompositePlugin when feature = "3d"
```

### Spawning an actor

The actor entity holds the world position (feet). Spawn a **child** entity with `RoComposite`
on it — the plugin drives its `Transform` to keep the canvas feet pixel at the parent's
world origin:

```rust
fn spawn_actor(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<RoCompositeMaterial>>,
    server: Res<AssetServer>,
) {
    commands
        .spawn(Transform::from_xyz(0.0, 0.0, 0.0)) // actor — owns world position
        .with_children(|parent| {
            parent.spawn((
                RoComposite {
                    layers: vec![
                        CompositeLayerDef { atlas: server.load("female_knight.spr"),              role: SpriteRole::Body },
                        CompositeLayerDef { atlas: server.load("female_head1.spr"),               role: SpriteRole::Head },
                        CompositeLayerDef { atlas: server.load("female_knight_spear_weapon.spr"), role: SpriteRole::Weapon { slot: 0 } },
                    ],
                    tag:     Some("idle_s".to_string()),
                    playing: false, // idle frames are driven by head-direction, not time
                    ..Default::default()
                },
                Mesh3d(meshes.add(Rectangle::new(1.0, 1.0))),
                MeshMaterial3d(mats.add(RoCompositeMaterial::default())),
                Transform::default(),
            ));
        });
}
```

`SpriteRole` controls draw order. The `Body` layer is the compositing anchor — its attach
point drives the position of all other layers. Draw order is computed per-frame from the
role and the current direction suffix in the tag (topLeft vs bottomRight camera group).

### Driving the tag from game state

Use `direction_index` + `composite_tag` to build the correct tag from your game state:

```rust
fn update_actor_animation(
    actors: Query<
        (&MyActorState, &MyFacingDir, &Children),
        Or<(Changed<MyActorState>, Changed<MyFacingDir>)>,
    >,
    mut billboards: Query<&mut RoComposite>,
    camera_q: Query<&Transform, With<Camera3d>>,
) {
    let cam_fwd = camera_q
        .single()
        .ok()
        .map(|t| { let f = t.forward().as_vec3(); Vec2::new(f.x, f.z) })
        .unwrap_or(Vec2::NEG_Y);

    for (state, facing, children) in &actors {
        for child in children.iter() {
            let Ok(mut composite) = billboards.get_mut(child) else { continue };
            let dir = direction_index(facing.0, cam_fwd);
            composite.tag     = Some(composite_tag(state.action_name(), dir));
            composite.playing = state.should_animate();
        }
    }
}
```

### Direction index

`direction_index(facing: Vec2, cam_fwd: Vec2) -> u8`

Maps the actor's XZ facing direction + the camera's XZ forward to a 0–7 index:

```
0 = s  (toward camera)
1 = sw
2 = w
3 = nw
4 = n  (away from camera)
5 = ne
6 = e
7 = se
```

Pass `Vec2::ZERO` as `cam_fwd` during startup (before the camera is queried) — it defaults
to `Vec2::NEG_Y` (south), which is a safe fallback.

### Billboard positioning — how it works

The plugin computes `Transform::translation` on the billboard child so that the **canvas feet
pixel always lands at the parent entity's world position**, regardless of canvas size or
which weapon/layer is active:

```
local_x = canvas_feet.x − canvas_size.x / 2
local_y = canvas_size.y / 2 − canvas_feet.y
transform.translation = −local_x * cam_right − local_y * cam_up
```

`cam_right` and `cam_up` are the camera's world-space right/up vectors. This is
camera-angle-independent and eliminates the "character slides when weapon appears" artifact
that occurs if you naively use world X/Y for the canvas offset.

---

## SpriteRole — z-order reference

| `SpriteRole` | topLeft (W/NW/N/NE) | bottomRight (S/SW/E/SE) |
|---|---|---|
| `Shadow` | −1 | −1 |
| `Shield` | 10 | 30 |
| `Body` | 15 | 10 |
| `Head` (normal) | 20 | 15 |
| `Head` (IMF behind body) | 14 | 9 |
| `Headgear { slot: 0..3 }` | 22–25 | 17–20 |
| `Weapon { slot: 0..1 }` | 28–29 | 23–24 |
| `Garment` | 35 | 35 |

Direction group is derived from the tag suffix (`"attack2_nw"` → topLeft). The head z-order
drops when the body's `.imf` file says `priority(layer=1, action, frame) == 1` for that frame.
`Garment` uses 35 (always-on-top) as a default; the full RO client drives it per-frame via Lua.

---

## RoAtlas — asset internals

```rust
pub struct RoAtlas {
    pub atlas_layout:        Handle<TextureAtlasLayout>,
    pub atlas_image:         Handle<Image>,
    pub frame_durations:     Vec<Duration>,
    pub frame_origins:       Vec<IVec2>,         // feet position within each frame's rect
    pub frame_attach_points: Vec<Option<IVec2>>, // ACT attach point, feet-origin space
    pub tags:                HashMap<String, TagMeta>, // "idle_s" → frame range
    pub frame_indices:       Vec<usize>,          // logical frame → deduplicated atlas slot
    pub frame_head_behind:   Vec<bool>,           // IMF priority(1,action,frame)==1 per frame
}
```

Frames with identical pixel content share one atlas slot (deduplication happens at load time).
`frame_origins` gives the feet pixel within each frame's tight-cropped rect — use this if you
need to position the sprite in a custom renderer.

---

## Layer compositing — attach points and frame remapping

ACT attach points are in **feet-origin space, y-down** (`(0,0)` = feet, negative y = above
feet). The plugin composites layers as follows:

```
attach_offset  = anchor_attach_point − layer_attach_point
canvas_top_left = canvas_feet + attach_offset − layer.origin
```

Layers whose attach points match the anchor (weapons, garments, headgear) have
`attach_offset = (0,0)` and render at the shared feet origin with only their ACT `x,y`
offsets applied.

### Frame remapping for non-body layers

`RoComposite::current_frame` is an index into the **body** atlas's flat frame sequence.
Different sprite types have different frame counts per action (body idle = 3 frames/direction,
weapon idle = 1 invisible frame/direction), so their flat sequences diverge. The plugin
remaps non-body layers by computing the relative position within the body's current tag and
applying it to the same tag in the layer's own atlas:

```
rel_frame    = current_frame − body_tag.start
mapped_frame = layer_tag.start + rel_frame   (clamped to layer_tag.end)
```

This ensures the weapon shows `attack1_e` frame 2 when the body is on `attack1_e` frame 2,
regardless of how many invisible frames the weapon accumulates in earlier actions.

---

## `ro-sprite` — standalone parser

If you only need to parse files or render frames to pixel buffers (e.g. for a CLI exporter):

```rust
use ro_sprite::{ActFile, SprFile, composite::render_frame_tight};

let spr = SprFile::parse(&spr_bytes)?;
let act = ActFile::parse(&act_bytes)?;

for action in &act.actions {
    for frame in &action.frames {
        if let Some((buf, origin_x, origin_y)) = render_frame_tight(&spr, frame, /*pad=*/1) {
            // buf.pixels: Vec<u8> RGBA, buf.width/height
            // origin_x/y: feet position within buf
        }
    }
}
```
