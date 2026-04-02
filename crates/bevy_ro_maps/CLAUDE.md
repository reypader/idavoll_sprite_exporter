# bevy_ro_maps

Bevy integration for RO map files. Depends on `ro-maps` for all parsing.
Bevy is only in this crate, not in `ro-maps`.

---

## Plugin Responsibilities vs User Responsibilities

**Plugin (`RoMapsPlugin`) handles:**
- Asset loading: `RoMapAsset` from `.gnd` (co-loads `.gat` and `.rsw` by same basename)
- Terrain mesh generation: top surfaces grouped by `texture_id`, one mesh per texture
- Child entity spawning: each texture group becomes a `Mesh3d + MeshMaterial3d<StandardMaterial>` child of the map root

**User code handles:**
- Spawning a `RoMapRoot` component on an entity (sets `asset` handle, `spawned: false`)
- Querying height and walkability via `height_at` and `terrain_at` free functions

---

## Usage

```rust
// Spawn map
commands.spawn((
    RoMapRoot { asset: asset_server.load("maps/prontera.gnd"), spawned: false },
    Transform::default(),
    Visibility::default(),
));

// Query height at world position (bilinear interpolation across tile corners)
let y = height_at(&map_asset.gat, map_asset.gnd.scale, world_x, world_z);

// Query walkability
let walkable = is_walkable(&map_asset.gat, map_asset.gnd.scale, world_x, world_z);
```

---

## Coordinate System

- Each GND cube / GAT tile = `gnd.scale` (10.0) world units in X and Z.
- Tile `(col, row)` maps to world `(col * scale - cx, y, row * scale - cz)` where `cx = width * scale * 0.5`, `cz = height * scale * 0.5` (terrain is centered at origin).
- RO altitudes are Y-down; `height_at` and the terrain mesh renderer negate them for Bevy Y-up.
- GND and GAT share the same `width x height` grid.

### RSW Object Positions

RSW `pos.x` is centered: correct Bevy X = `pos.x` (no offset). RSW `pos.z` requires a +scale correction.
Correct instance translation: `(pos.x, -pos.y, gnd_scale - pos.z)`.
Derivation: browedit world Z (after its outer `Scale(1,1,-1)`) = `scale + cz - pos.z`; our Bevy Z = Z_bro - cz = `scale - pos.z`.
The terrain is NOT rendered through Scale(1,1,-1) in browedit; only RSM models use it.

RSM texture names in `RsmFile::textures` are bare filenames (e.g. `lamp.bmp`). Do not prefix `data/texture/`; the caller configures their asset server root.

---

## Key Data Types

| Type | Description |
|---|---|
| `RoMapAsset` | Bevy asset loaded from `.gnd`. Contains `gnd: GndFile`, `gat: GatFile`, `lighting: RswLighting`, `objects: Vec<RswObject>`. |
| `RoMapRoot` | Component: `asset: Handle<RoMapAsset>`, `spawned: bool`. Plugin watches this to spawn terrain. |
| `RoMapMesh` | Marker component on each spawned terrain mesh child entity. |
| `PendingModels` | Internal component on map root while RSM instances are still loading. Removed automatically when all are spawned. |

For parsed file types (`GatFile`, `GndFile`, `RswLighting`, etc.) see `crates/ro-maps/CLAUDE.md`.

---

## Terrain Mesh Winding

GND tile vertices are: sw(0), se(1), nw(2), ne(3). CCW winding viewed from above (+Y) requires
`sw→se→nw` and `se→ne→nw` (indices 0,1,2 and 1,3,2). The normal is `(se-sw) × (nw-sw)` which
gives +Y. The reversed order (`sw→nw→se`) gives a downward normal — terrain appears on the
underside.

---

## RSM Y Pivot

Use the **actual** minimum Bevy Y (scan all mesh vertices through the full per-mesh transform
chain: offset → pos_ → scale → rotation → pos for non-root, then negate Y) as the Y pivot.
Do NOT use `rsm.bbmax[1]` from the pre-computed simple bounding box — that only applies the
offset matrix and misses per-mesh scale/rotation, causing models to float or sink.
This matches browedit's `realbbmin.y` from `setBoundingBox2`.

---

## Deferred Features

Data is fully parsed and stored in `RoMapAsset`; rendering can be added incrementally.

- **Wall surfaces:** `north_surface_id` / `east_surface_id` in `GndCube` (requires comparing adjacent cube heights)
- **Lightmap UV blending:** data in `GndLightmapSlice`; needs a custom material with a second UV channel
- **Water plane:** `GndWaterPlane` in `GndFile`
- **RSW model placement:** static geometry implemented via `PendingModels` + `spawn_model_meshes`. Skeletal animation deferred.
