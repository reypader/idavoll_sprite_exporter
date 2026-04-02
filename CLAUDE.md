# Bevy RO Libs

This workspace houses all Bevy plugins needed to load and render Ragnarok Online resources.
`bevy_ro_sprite` composites SPR/ACT sprite layers onto a billboard quad.
`bevy_ro_maps` loads and renders GND/GAT/RSW map files.
Additional plugins for other RO resource types will be added here.

---

## Reference Materials

Local copies of all key references are at:
`/Users/rmpader/Downloads/ragnarok_online_resource_references/`

| Directory | Purpose |
|---|---|
| `ragnarokresearchlab.github.io/` | **Primary file format reference.** Docusaurus site; check `docs/` for SPR, ACT, IMF, GND, RSW, GAT format specs. Use this before guessing at binary layouts. |
| `browedit/brolib/BroLib/` | **Authoritative RSM/RSM2 reference.** The ragnarokresearchlab spec has no RSM content; browedit C++ source is the only reliable reference for RSM format. |
| `rathena/` | **Word/name translations.** C++ source for the rAthena emulator. Useful for mapping Korean item/monster names to their English equivalents via the DB files under `db/`. |
| `ase-file-specs.md` | Aseprite file format spec (for output format reference). |
| `data.grf` | Raw GRF archive (source assets). Use `../idavoll_grf_extractor` to extract. |
| `zextractor/` | GRF extraction tool reference. |

---

## Crate Structure

```
crates/ro-sprite/         — pure Rust, no Bevy: SPR/ACT/IMF file parsers + software renderer
crates/bevy_ro_sprite/    — Bevy integration: asset loader, animation systems, composite material
crates/ro-maps/           — pure Rust, no Bevy: GND/GAT/RSW file parsers
crates/bevy_ro_maps/      — Bevy integration: map asset loader, terrain mesh rendering, height/nav queries
crates/ro-rsm/            — pure Rust, no Bevy: RSM/RSM2 file parser
crates/bevy_ro_rsm/       — Bevy integration: RSM asset loader (embedded in RoMapsPlugin)
```

Each crate has its own `CLAUDE.md` with domain-specific notes.

---

## TO DO

- Play sounds on animation events (`SpriteFrameEvent` → Bevy audio)
- Map rendering: wall surfaces (`north_surface_id` / `east_surface_id` in `GndCube`)
- Map rendering: lightmap UV blending via custom material (data already parsed into `GndLightmapSlice`)
- Map rendering: water plane (`GndWaterPlane`)
- Map rendering: RSW 3D model placement; static geometry done, skeletal animation and RSM2 v2.2 deferred
- Map rendering: Per-mesh entity hierarchy (needed only for animation) deferred
- Try compositing mercenary sprites
- Try compositing NPC and Monster Sprites
- Try compositing items-on-ground
- Try compositing emoticons and effects on the player
