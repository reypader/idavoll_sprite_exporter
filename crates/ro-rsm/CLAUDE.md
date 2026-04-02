# ro-rsm

Pure Rust parser for RSM v1.x and RSM2 v2.3. No Bevy dependency. Consumed by `bevy_ro_rsm`.

---

## Key Gotcha: Version Bytes

RSM version is stored as `[major, minor]` in big-endian byte order. Do NOT read with `ru16`
(little-endian); that gives `0x0401` for v1.4 instead of the correct `0x0104`. Read as two
separate `u8` bytes: `let version = (major as u16) << 8 | minor as u16`.

---

## Format References

Authoritative source: `browedit/brolib/BroLib/Rsm.cpp` and `Rsm2.cpp`.
The ragnarokresearchlab.github.io spec has no RSM content.

---

## RSM1 vs RSM2

Both start with magic `GRSM` + version u16. RSM1: version < 0x0200. RSM2: version >= 0x0200
(only v2.3 = 0x0203 is implemented; v2.2 returns an error).

RSM2 differences from RSM1:
- Strings are length-prefixed (i32 + bytes) instead of fixed 40-byte null-padded.
- Textures stored per-mesh and deduplicated into a shared list.
- No `pos_`, `rot_angle`, `rot_axis`, or `scale` per mesh (defaulted to identity).
- TexCoords are vec3; use y and z components as u and v.
- Faces have a variable `len` field; skip `(len - 24)` bytes of unknown data per face.
- Extra unknown sections after faces: pos keyframes, rot keyframes, two unknown arrays.

---

## Bounding Box

Computed in `compute_bounding_box` after parsing: for each mesh, apply `offset` matrix to each
vertex; if non-root (non-empty `parent_name`), also add `pos + pos_`. Track global min/max.
`bbrange = (bbmin + bbmax) / 2`. Used as the model pivot in `bevy_ro_maps` render.
