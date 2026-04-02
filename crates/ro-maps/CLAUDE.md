# ro-maps

Pure Rust crate; no Bevy. Parses GND (terrain mesh), GAT (height/navigation), and RSW (world
scene) binary files. Consumed by `bevy_ro_maps`.

Public surface: `GndFile`, `GatFile`, `RswFile` (and associated types).

Cross-reference `browedit/brolib/BroLib/Gnd.cpp` and `Rsw.cpp` for binary layouts. The
ragnarokresearchlab spec has known errors: wrong GND surface size (says 56 bytes, actually 40)
and omits RSW file references entirely.

---

## Coordinate Convention

RO uses a **Y-down** coordinate system for heights. All altitude values in `GatTile` and
`GndCube` are stored as-is (positive = lower ground in RO space). Callers that need Bevy
Y-up must negate them. `bevy_ro_maps` handles this; do not negate in this crate.

The tile grid origin is at `(col=0, row=0)` = world top-left. Each tile is `scale` world
units wide (always 10.0 in practice). GND and GAT share the same `width x height` grid
(1 GND cube = 1 GAT tile).

---

## GatFile

Magic: `GRAT`. Version bytes: major, minor.

Each tile stores four corner altitudes (SW, SE, NW, NE) as `f32`, followed by a `u32`
terrain field. In v1.3+, bit `0x80000000` of that field is the water flag; the lower 31 bits
encode `TerrainType` (0 = Walkable, 1 = Blocked, 5 = Snipeable).

`GatFile::tile(col, row)` returns `Option<&GatTile>` with bounds checking.

---

## GND Cube Height Corner Ordering

`GndCube::heights[0..4]` reads the binary in order h1,h2,h3,h4. Despite the `[SW,SE,NW,NE]`
comment in the source, the actual browedit mapping is h1=NW, h2=NE, h3=SW, h4=SE, so the
correct order is `[NW, NE, SW, SE]`. For a cube at (col, row): heights[0] (NW) is the north
edge at z = cz - (row-1)*scale and heights[2] (SW) is the south edge at z = cz - row*scale.

---

## GndFile

Magic: `GRGN`. Version bytes: major, minor.

Key version-gated behavior:
- Texture path length is version-dependent (40 bytes for v1.6 and below, 40 bytes for
  v1.7+; check the format spec if extending).
- Lightmap section: one global header (pixel_format i32, width i32, height i32 — always
  1/8/8) followed by `lightmap_count` slices of 256 bytes each (64-byte shadowmap + 192-byte
  lightmap). The ragnarokresearchlab spec incorrectly shows these header fields as per-slice.
- Surface struct is 40 bytes: u[4], v[4], texture_id (i16), lightmap_id (i16), color [u8;4].
  There is NO padding between texture_id and lightmap_id. The ragnarokresearchlab spec is wrong
  here; browedit (Gnd.cpp) is authoritative.
- Cube struct is 28 bytes: heights[4] (f32 x4), top_surface_id (i32), north_surface_id (i32),
  east_surface_id (i32). Surface IDs of -1 mean "no surface."
- Water plane present for v1.8+; v1.9+ supports multiple water planes.

---

## RswFile

Magic: `GRSW`. Version bytes: major, minor.

The format DOES contain file references (4×40-byte strings: ini, gnd, gat, src). The gat string
is present only in v1.5+. These are read and discarded; our loader derives paths from the asset
basename instead. The ragnarokresearchlab spec was wrong about this omission.

Layout after magic + version:
1. Build number (v2.2+: u8; v2.5+: u32 followed by an extra unknown u8)
2. Water plane (24 bytes, only present when version < 2.6)
3. Lighting parameters (36 bytes, all versions)
4. Map boundaries / bounding box (16 bytes, all versions; content is unused)
5. Object count + object list
6. QuadTree (v2.1+, at the very end; not parsed)

Version-gated fields within objects:
- Model instance: extra unknown byte after `collision_flags` in v2.6 build >= 162.
- Audio source: `cycle` field present in v2.0+; defaults to 4.0 otherwise.

Helper `at_least(version, major, minor) -> bool` is used internally for all version checks.

---

## Key Data Types

| Type | Description |
|---|---|
| `GatFile` | Parsed GAT: `width x height` grid of `GatTile` (corner altitudes + terrain type). |
| `GatTile` | `altitude_sw/se/nw/ne: f32`, `terrain_type: TerrainType`, `is_water: bool`. |
| `TerrainType` | `Walkable / Blocked / Snipeable / Unknown(u8)`. |
| `GndFile` | Parsed GND: cubes, surfaces, textures, lightmaps, optional water plane. |
| `GndCube` | `heights: [f32; 4]` (SW/SE/NW/NE), `top_surface_id`, `north_surface_id`, `east_surface_id` (all i32; -1 = none). |
| `GndSurface` | `u[4]`, `v[4]`, `texture_id: i16`, `lightmap_id: i16`, `color: [u8;4]` (single BGRA). |
| `GndLightmapSlice` | `shadowmap: [u8; 64]`, `lightmap: [u8; 192]` (8x8 grayscale AO + RGB). |
| `GndWaterPlane` | Level, type, and wave parameters. |
| `RswFile` | Parsed RSW: version, gnd/gat file refs, `lighting: RswLighting`, `objects: Vec<RswObject>`. |
| `RswLighting` | `longitude`, `latitude` (u32 degrees), `diffuse[3]`, `ambient[3]`, `shadowmap_alpha`. |
| `RswObject` | Enum: `Model(ModelInstance) / Light(LightSource) / Audio(AudioSource) / Effect(EffectEmitter)`. |
