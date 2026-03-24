# ACT/SPR Sprite Conversion — Research Notes

## Project Goal
Convert Ragnarok Online proprietary ACT/SPR sprite formats into standard animation formats
(currently: Aseprite-compatible PNG spritesheet + JSON metadata).

Source references:
- Format docs: `../zrenderer/source/ragnarokresearchlab.github.io/docs/file-formats/`
- D lang reference implementation: `../zrenderer/source/zrenderer/source/`

---

## File Formats

### SPR (sprite images)
- Signature: `"SP"`, version as LE u16 (e.g. `0x0201`)
- Two image banks: palette-indexed (type 0) and RGBA truecolor (type 1)
- Palette: always the last 1024 bytes of the file (256 × RGBA entries); index 0 is always transparent
- v2.1+: palette images are RLE-compressed (`0x00` byte triggers a run; next byte = count of transparent pixels)
- RGBA images: stored ABGR byte order, Y-axis inverted — must flip on load
- `spr_id` indexes separately into each bank depending on `spr_type` (0 or 1)

### ACT (animation data)
- Signature: `"AC"`, version as LE u16 (e.g. `0x0204`)
- Structure: `actions[]` → `frames[]` → `sprite_layers[]`
- Each frame skips 32 "mystery bytes" (attackRange + fitRange, 8 × u32) before the layer count
- Frame timing: `interval × 24ms` (stored per-action after the events block, v2.2+; default = 4 → 96ms)
- Events block (v2.1+): array of 40-byte null-terminated strings e.g. `"atk"`, `"attack.wav"`
- Each frame has an `event_id` (i32, -1 = none) referencing the events array
- Attach points (v2.3+): 1 per frame for player/head sprites (see below)
- Version milestones: `0x200` event_id, `0x201` events array, `0x202` per-action intervals,
  `0x203` attach points, `0x204` separate xScale/yScale, `0x205` explicit width/height per layer

---

## Player Sprite Structure

### Action layout
Player sprites have **104 actions** = 13 animation types × 8 directions.
Actions are grouped in blocks of 8 (one per direction):

| Base index | Name      |
|------------|-----------|
| 0          | stand     |
| 8          | walk      |
| 16         | sit       |
| 24         | pickup    |
| 32         | atk_wait  |
| 40         | attack    |
| 48         | damage    |
| 56         | damage2   |
| 64         | dead      |
| 72         | unk       |
| 80         | attack2   |
| 88         | attack3   |
| 96         | skill     |

Direction order within each block (index % 8):
`0=s, 1=sw, 2=w, 3=nw, 4=n, 5=ne, 6=e, 7=se`

---

## Body Sprite (`몸통/남/`, `몸통/여/`)

Verified with `초보자_남` (male novice body, SPR v0x0201, ACT v0x0204):
- 110 palette images, 0 RGBA images
- **1 sprite layer per frame**
- **Stand/sit actions have 3 frames** — NOT a time-based loop.
  Each frame corresponds to a **head direction** (frame 0=straight, 1=left, 2=right).
  The body sprite image is identical across all 3 frames; only the anchor point differs.
- **Walk and other actions** have unique sprites per frame (genuine animation)

### Direction symmetry (body)
Only 5 unique sprites cover all 8 stand directions.
`s/sw/w/nw/n` are the canonical sprites; `ne/e/se` are horizontal mirrors (`flags & 1 = 1`):

| Direction | Sprite | Mirror of |
|-----------|--------|-----------|
| s         | spr0   | —         |
| sw        | spr1   | —         |
| w         | spr2   | —         |
| nw        | spr3   | —         |
| n         | spr4   | —         |
| ne        | spr3   | nw        |
| e         | spr2   | w         |
| se        | spr1   | sw        |

---

## Head Sprite (`머리통/남/`, `머리통/여/`)

Verified with `1_남` (male head #1, ACT v0x0204):
- **2 layers per frame** — layer 0 is always `spr_id = -1` (empty placeholder); actual sprite is **layer 1**
- **Stand/sit actions have 3 frames** — same as body: frame 0=straight, 1=left, 2=right
- Only 5 unique sprites (`spr0`–`spr4`) cover all 24 combinations (8 dirs × 3 head dirs)

### Head direction × body direction (`1_남` stand, layer 1)

| body dir | straight | left    | right    |
|----------|----------|---------|----------|
| s        | spr0     | spr1    | spr1 🪞  |
| sw       | spr1     | spr2    | spr0     |
| w        | spr2     | spr3    | spr1     |
| nw       | spr3     | spr4    | spr2     |
| n        | spr4     | spr3 🪞 | spr3     |
| ne       | spr3 🪞  | spr2 🪞 | spr4 🪞  |
| e        | spr2 🪞  | spr1 🪞 | spr3 🪞  |
| se       | spr1 🪞  | spr0 🪞 | spr2 🪞  |

🪞 = `flags & 1` (horizontal flip applied during rendering, not stored in output PNG)

**Mirror consistency:** `ne/e/se` are full mirrors of `nw/w/sw` with left↔right swapped —
geometrically correct since flipping a direction reverses the head turn sides.

**Adjacent direction reuse:** head turns reuse the adjacent body-direction's straight sprite:
- `w + head left` = spr3 = `nw straight`
- `w + head right` = spr1 = `sw straight`
- `s + head left` = spr1 = `sw straight`
- `s + head right` = spr1🪞 = `se straight`

---

## Anchor Points & Child Sprite Attachment

- Player body and head each store **exactly 1 attach point per frame**
- Attachment formula (from `sprite.d`):
  ```
  child_canvas_offset = parent_anchor - child_anchor
  ```
- For stand/sit: the body's 3 anchor points correspond to the 3 head directions (not time).
  Slight differences between the 3 anchors position the head correctly per head direction.
- For non-stand/sit actions: anchor points are genuine per-frame animation values.

---

## Sprite Rendering (transform per layer)

From `renderer.d` / `sprite.d`:
1. Center the sprite image at origin (`-width/2, -height/2`)
2. Scale: `xScale × (flags & 1 ? -1 : 1)` for X, `yScale` for Y
3. Rotate: degrees, clockwise in screen/Y-down space
4. Translate to layer `(x, y)` + parent anchor offset
5. Tint: multiplicative RGBA (`[255,255,255,255]` = no change)
6. Alpha-composite onto canvas

Canvas origin `(0,0)` is the sprite "feet" reference point. Sprites extend upward (negative Y).
The mirror flag (`flags & 1`) is consumed during rendering — output PNG always shows the
correct orientation; no flag metadata needed in the output JSON.

---

---

## Weapon Sprites (`인간족/<job>/<job>_<gender><weaponname>`)

Verified with `swordsman_male_양손검`, `swordsman_male_검`, `swordsman_male_창`, `swordsman_male_단검`.

### Path convention
- Resolved via Lua `ReqWeaponName(weaponid)` → generic type name (e.g. `양손검`, `검`, `창`)
- Fallback: `_<itemid>` if no Lua name found (those files often contain real sprites too)
- Path: `data/sprite/인간족/<jobWeaponName>_<gender><weaponname>.{spr,act}`
  where `jobWeaponName` = `job_weapon_names.txt[jobid]` (e.g. `swordsman/swordsman` for job 1)
- English-extracted data uses English job names; Korean client uses Korean equivalents

### Action visibility
Weapons are **invisible** (`spr_id=-1`, scale=0) in most actions. They only appear in:

| Visible actions | Weapon types |
|-----------------|--------------|
| `atk_wait` (32–39) | All weapon types |
| `attack2` (80–87) | Swords (`검`), 2H swords (`양손검`), daggers (`단검`) |
| `attack3` (88–95) | Spears (`창`) |

Stand, walk, sit, pickup, attack(40), damage, dead → weapon is invisible.

### Coordinate system
- Weapon and body **share the same feet-origin** — no translation offset needed
- Both body and weapon store **identical attach-point values** per frame
  → `parent_anchor - child_anchor = (0,0)` → weapon renders at origin
- Weapon layer `x,y` is an absolute offset from the feet-origin (like any body layer)
- Weapon SPR images are the actual sword/weapon art; ACT provides per-action positioning

### Weapon slash (`_검광` suffix / `_slash_glow` after translation)
- Separate SPR/ACT pair with the `_검광` suffix (e.g. `swordsman_male_검_검광`)
- Same path convention; used for the attack slash effect overlay

### Universal sparse overlay pattern (confirmed across all jobs)
Weapon sprites for ALL jobs (Knight, Gunslinger, Rebellion, etc.) share the same structure:
- 104 action slots, but ONLY `atk_wait` (32–39) and `attack2` (80–87) populated
- All other slots are invisible (spr_id=-1, scale=0)
- The base body handles all other animation frames

### Mercenary weapons
`human/mercenary/` contains gender-neutral weapon overlay sprites for the three mercenary
types: `sword_mercenary_sword.spr`, `spear_mercenary_spear.spr`, `bow_mercenary_bow.spr`,
plus `_slash_glow` variants for sword and spear. No `_male`/`_female` suffix — the same
file is shared by both genders. `scan_weapons` handles this via a special `mercenary/`
branch that parses `{job_prefix}_mercenary_{weapon}` and emits two entries (male + female)
pointing at the same file.

### Mercenary sprite structure — self-contained, not composited
Mercenaries differ fundamentally from player characters in how they render:

- **Head baked in**: The body sprite (`human/body/{gender}/{type}_mercenary.spr`) includes
  the head — there is no separate head layer or IMF anchor point file.
- **No headgear / garment layers**: Mercenaries are hired NPCs, not player job classes.
  The renderer draws them as a flat entity: body sprite + weapon overlay only.
- **Weapon overlays confirmed distinct** (MD5 verified): The weapon sprites in
  `human/mercenary/` are genuinely different files from the body sprites — they are real
  weapon art (sword, spear, bow shown at various attack angles), not duplicates.
- **Body sprites are gender-specific in storage but gender-neutral in content**: sword and
  spear mercenaries are stored under `male/`, bow mercenary under `female/`, each without
  a gender suffix. `scan_bodies` picks them up via the "no gender marker" case, producing
  one entry per gender per type (pointing at the same file).

---

---

## Rebellion Body Overlay System — DOES NOT EXIST

`human/body/{gender}/` contains files like `rebellion_{gender}_pistol.spr` that appear to be
weapon-keyed body overlays. **They are byte-identical (MD5 confirmed) copies of the weapon
sprites in `human/rebellion/`.** They were mis-copied into the body directory and carry no
unique animation data.

The body scan skips them correctly (they end with `_pistol` / `_machine_gun` which contains a
gender marker from the opposite direction — the no-suffix branch doesn't match, the suffix
branch fails to strip cleanly → they fall through to `continue`).

`WeaponEntry` has no `body_spr`/`body_act` fields. No body overlay export logic exists.

---

---

## Body Variant Types

All body variants are **full 104-action replacements** (not sparse overlays) triggered by
equipment/state. Analogous to mount toggling. All emit as separate `BodyEntry` with a
descriptive `job` name — the consumer selects the right one based on equipment state.

| Variant | job naming | trigger |
|---------|-----------|---------|
| Peco mount | `pecopeco_knight` (separate job ID) | mount/dismount |
| Dancer pants | `dancer_costume_1` | equip pants item |
| Costume slot N | `{base_job}_costume_{n}` | equip costume item in slot N |

**Costume subdirectories:** `human/body/{gender}/costume_{n}/` — scanned by `scan_bodies`.
Stem `{job}_{gender}_{n}` → `job = "{job}_costume_{n}"`. No IMF for costume variants.
`costume_1/` has ~37 sprites; `costume_2`–`costume_4` have 1–2 each.

**Dancer pants** is stored in the flat body dir as `dancer_{gender}_pants.spr` rather than
in a `costume_1/` subdir. The scan detects this stem specifically and emits `dancer_costume_1`.
`"바지" = "pants"` is a general translation; the costume_1 mapping is in the scan, not translations.

---

---

## Headgear Sprites (`악세사리/<gender>/<gender><name>`)

Verified with `m_ax_eyes` (axe-shaped eyes accessory).

- 104 actions (same player layout), visible in **all** action groups
- 1 sprite layer per frame (unlike head's 2-layer structure)
- Attach points mirror the **head's** attach points exactly per frame
  → `head_anchor - headgear_anchor = (0,0)` → headgear renders at body feet-origin
- Layer `x,y` is the absolute position relative to feet-origin
- Path: `data/sprite/악세사리/<gender>/<gender><name>.{spr,act}`
  - `name` from Lua `ReqAccName(headgearid)`; `<gender>` prefix is `m_` or `f_`

---

## Shield Sprites (`방패/<job>/<job>_<gender>_<name>`)

- **Structure:** `sprite/shield/{job}/` — one subdirectory per job class.
- **Naming:** `{job}_{gender}_{shield_name}.spr` / `.act`
  - Named shields: `buckler`, `guard` (only some jobs have these)
  - ID-based shields: `{item_id}_shield` (e.g. `28901_shield`) — the `_shield` suffix is part
    of the name, not a slot indicator
- **Actions:** 104 actions, same player layout. No attach points.
- **Z-order:** Fixed — `Shield` kind; 10 (top-left dirs), 30 (bottom-right dirs). No IMF needed.
- **Compositing:** Shield renders relative to the body feet-origin, same coordinate system as
  weapon. No displacement offset — shield layer `x,y` is absolute from origin.
- **Scan:** `scan_shields` iterates `sprite/shield/` job subdirs, strips `{job}_` prefix,
  splits on first `_` for gender. Manifest: `ShieldEntry { name, job, gender, spr, act }`.
- **Output:** `shield/<name>/<job>/<gender>/` — mirrors weapon output structure.

---

## Garment Sprites (`로브/<name>/<gender>/<jobname>_<gender>`)

Verified with `천사날개` (angel wings) for swordsman_male.

- 104 actions (same player layout), visible in **all** action groups
- Job-specific: separate SPR/ACT per job class
- Attach points mirror the **body's** attach points per frame
  → renders at the same feet-origin as the body
- Layer `x,y` is absolute relative to feet-origin
- Path: `data/sprite/로브/<name>/<gender>/<jobname>_<gender>.{spr,act}`
  - `name` from Lua `ReqRobSprName_V2(garmentid)`
  - `jobname` from `job_names.txt[jobid]`
- Fallback path (no job-specific sprite): `로브/<name>/<name>.{spr,act}`
- 3 events in garment ACT (sound cues: `attack_sword.wav`, etc.) — same event system

---

## Unified Coordinate System

All child sprites (head, weapon, headgear, garment) render in the **same feet-origin space**:

| Sprite | Parent | `parent_anchor - child_anchor` | Result |
|--------|--------|-------------------------------|--------|
| Head   | Body   | non-zero (head has own origin) | canvas displacement |
| Weapon | Body   | = 0 (attach points match)      | renders at origin |
| Headgear | Head | = 0 (attach points match)     | renders at origin |
| Garment | Body  | = 0 (attach points match)      | renders at origin |

Only the head requires actual displacement. All other child sprites use layer `x,y` directly as their position in the shared body-origin canvas.

---

---

## Monster Sprites (`몬스터/<jobname>`)

Full scan of all ~1265 monster sprites reveals four distinct groups:

### Action count groups

**Standard (40 actions = 5 types × 8 dirs)** — the common case.

| Base index | Type    |
|------------|---------|
| 0          | stand   |
| 8          | move    |
| 16         | attack  |
| 24         | damage  |
| 32         | dead    |

**Extended (48 / 56 / 64 / 72 actions)** — all clean multiples of 8; extra animation types beyond the standard 5.
Two sub-patterns:
- Named variants (e.g. `poring`=72, `wolf`=48) — more animation variety
- Korean-suffixed headgear variants (e.g. `baphomet_뼉다구모자`, `deviruchi_baby_pacifier`) — cosmetic versions of
  base monsters wearing equipment items. These are separate sprite files, not runtime compositing. Notably they
  ship with MORE actions than their base form (e.g. base `baphomet`=48, headgear variant `baphomet_뼉다구모자`=64).

**Projectile / effect sprites (1 or 8 actions)** — not gameplay monsters; one-shot or directional visual effects
fired by the combat system. Examples: `canon_bullet`, `arrow` (from `skel_archer_arrow`), `drosera_bullet`, `soccer_ball`, `sonia`.
8-action ones follow the NPC layout (1 type × 8 dirs). 1-action ones are directional stills.
Handled as a separate `projectile` scan type — see scan_projectiles() in scan.rs. `skel_archer_arrow` is renamed
to `arrow` in the manifest (intended for reuse by all archer-type units).

**Anomalous (5 files)** — genuinely odd-sized ACTs confirmed via total vs visible check (no hidden empty slots).
Likely GRF authoring errors; not worth special-casing — consumers should load whatever actions are present.

| Monster | Actions | Notes |
|---|---|---|
| `8w_soldier` | 32 | 4 types × 8 — missing one standard type |
| `increase_soil` | 39 | 5×8 − 1 — one direction dropped |
| `bellare` | 41 | 5×8 + 1 — one extra slot |
| `zerom` | 41 | same |
| `dullahan` | 47 | 6×8 − 1 — one direction dropped |

**Outlier group (10 actions)** — `celine_kimi` and five `mm_mana_*` sprites. Not yet classified.

### Direction pairing
Monsters have only **2 canonical sprite views** across 8 direction slots:

| Direction slots | Sprites | Notes |
|-----------------|---------|-------|
| `s` (0), `sw` (1) | Front-facing | Identical spr_ids |
| `w` (2), `nw` (3) | Side-facing  | Identical spr_ids |
| `n` (4), `ne` (5) | Flipped side | Mirror of `w`/`nw` |
| `e` (6), `se` (7) | Flipped front | Mirror of `s`/`sw` |

No attach points (monsters don't composite child sprites).

---

## NPC Sprites (`npc/<jobname>`)

Verified with `1_f_01` and `1_f_gypsy`.

- Typically just **8 actions** (1 animation type × 8 directions)
- Single static sprite (or very few frames) reused across all 8 directions
- Only the `se` (index 7) direction typically has a flip — all others are identical
- No attach points, no child sprites

---

## Action Layout Summary

| Sprite type | Total actions | Action types |
|-------------|---------------|--------------|
| Player body/head | 104 | 13 types × 8 dirs |
| Weapon | 104 | Same slots; only atk_wait + attack2/3 visible |
| Headgear | 104 | Same slots; all visible |
| Garment | 104 | Same slots; all visible, job-specific |
| Monster | 40 (standard) | 5 types × 8 dirs; some have more |
| NPC | 8 (typical) | 1 type × 8 dirs |
| Projectile | 1 or 8 | directional still (1) or stand × 8 dirs (8) |

### Entity categories and compositing layers

| Category | Actions | Action groups | Compositing layers |
|---|---|---|---|
| Human | 104 | stand, walk, sit, pickup, atk_wait, attack, damage, damage2, dead, unk, attack2, attack3, skill | body + head + headgear + garment + weapon + shield |
| Mercenary | 104 | same 13 groups (weapon visible in stand + walk + atk_wait + attack2 only) | body + weapon |
| Monster | 40 | stand, move, attack, damage, dead | body only |
| NPC | 8 | stand | body only |
| Projectile | 1 or 8 | directional still (1) or stand × 8 dirs (8) | body only |

---

## Mount Sprites (Mounted Job Body)

Investigated with `페코페코_knight_male` (PecoPeco Knight) and `레인져늑대_male` (Ranger on Warg).

### Architecture: pre-baked combined body

Mounted jobs are **not** two separate composited sprites. The rider + mount are drawn as a
single combined body spritesheet (same structure as a regular player body sprite):
- 104 actions, same player action layout (13 types × 8 dirs)
- Treated as a standard player body sprite by the renderer
- Head, headgear, garment, and weapon are composited on top normally

### Attack authorship (who attacks in each slot)
The distinction is baked into the animation frames:

| Slot       | Knight (PecoPeco) | Ranger (Warg) |
|------------|-------------------|---------------|
| `attack` (40–47) | Knight attacks | Ranger attacks |
| `attack2` (80–87) | Knight attacks | Ranger attacks |
| `attack3` (88–95) | PecoPeco not moving | Warg attacks (mount does the attack) |

The ranger's Warg attacks in `attack3` while the ranger figure is passive; the PecoPeco never
attacks. This is baked into the sprite art — no metadata flag distinguishes who is attacking.

### Path convention
- Path: `data/sprite/인간족/몸통/<gender>/<jobname>_<gender>.{spr,act}`
- `jobname` from `job_names.txt[jobid]` (e.g. `페코페코_knight` for PecoPeco Knight, `레인져늑대` for Ranger-Warg)
- Mounted jobs have distinct jobids from their unmounted counterparts

### IMF files
- Mounted jobs (and all player jobs) have `.imf` files in `data/imf/` (flat directory)
- Stem matches the body sprite stem: `pecopeco_knight_male.imf` → `pecopeco_knight_male.spr`
- Used only for head-behind-body z-order; `cx`/`cy` fields are always zero (verified)

### `_h` suffix variants
Several 2nd-class job body sprites (including mounted variants) have a `_h` suffix version:
- e.g. `knight_h_male`, `페코페코_knight_h_male`, `헌터_h_male`, `assassin_h_male`, etc.
- **All `_h` files are byte-for-byte identical** to their non-`_h` counterparts
- `_h` stands for **"hair hidden"**: the original RO client loads `{jobname}_h_{gender}`
  when a full-coverage headgear item is equipped (to show the character without hair
  clipping through the helmet). In this GRF the files are simply duplicates.
- zrenderer does not use `_h` variants — safe to ignore for conversion purposes

---

## IMF File Format

IMF files store per-frame draw priorities for each layer of a **body** sprite. The primary use
is determining whether the head should render behind the body.

- Path: `data/imf/<stem>.imf` — flat directory at the same level as `data/sprite/`.
  Stem matches the body sprite stem exactly (e.g. `novice_male.imf` → `novice_male.spr`).
  The single `.imf` found co-located under `data/sprite/` is a GRF duplicate artifact (MD5 confirmed).
- Binary structure:
  ```
  f32   version
  i32   checksum  (skip)
  u32   maxLayer
  for each layer (0..=maxLayer):
    u32   numActions
    for each action:
      u32   numFrames
      for each frame:
        i32   priority
        i32   cx  (always 0 — verified by comparing against ACT attach points)
        i32   cy  (always 0 — same)
  ```
- `priority(layer=1, action, frame) == 1` → head renders **behind** the body for that frame
- All other priority values → head renders in front (normal case)
- `cx`/`cy` are unused/reserved fields — ACT attach points are the sole source of compositing offsets

---

## Z-Order / Compositing Layer Order

When multiple sprite types are composited (body + head + headgear + garment + weapon), the
draw order depends on both the sprite type and the current direction.

### Direction groups

```
direction = action_idx % 8
topLeft   = direction in {2, 3, 4, 5}  (W, NW, N, NE)
bottomRight = direction in {0, 1, 6, 7} (S, SW, E, SE)
```

### Z-order table (higher = drawn on top)

| Sprite | topLeft | bottomRight | Notes |
|--------|---------|-------------|-------|
| Shadow | -1 | -1 | |
| Garment | 35 | 35 | Default; runtime should override via Lua tables |
| Shield | 10 | 30 | |
| Body | 15 | 10 | |
| Head (normal) | 20 | 15 | IMF priority ≠ 1 |
| Head (behind body) | 14 | 9 | IMF priority == 1 |
| Headgear slot 0 (upper) | 22 | 17 | |
| Headgear slot 1 (middle) | 23 | 18 | |
| Headgear slot 2 (lower) | 24 | 19 | |
| Headgear slot 3 (extra) | 25 | 20 | |
| Weapon slot 0 | 28 | 23 | |
| Weapon slot 1 (slash) | 29 | 24 | |

### Garment z-order (runtime override required)

The garment z-order is **per-item, per-job, per-action, per-frame** in the RO client.
Two Lua tables govern this, found in `luafiles514/lua files/spreditinfo/`:

- `_new_2dlayerdir_f.lub`, `_new_biglayerdir_*.lub`, `_new_smalllayerdir_*.lub`
- `_New_DrawOnTop(robeID, gender, jobID, action, frame)` → if non-zero, draw on top
- `IsTopLayer(robeID)` → always-on-top flag

The converter outputs z=35 (always-on-top) as a safe default. Runtimes with access to these
Lua tables should override the `zOrder` field per frame when compositing.

**Headgear has no equivalent Lua-driven z-order system** — only slot + direction is used.

### Output in JSON

The `zOrder` field is written per-frame in the Aseprite JSON output when `--kind <KIND>` is
passed to the export command. It is omitted entirely when `--kind` is not specified.

---

## Verifying Duplicate / Mis-copied Files

When a sprite file appears in an unexpected location (wrong directory, unexpected naming),
verify before adding scan logic by checking if it is a byte-identical copy of a file that
already exists elsewhere:

```bash
# Find the suspected canonical file
find ~/Downloads/extracted/data/sprite -name "<suspected_canonical_name>.spr"

# Compare MD5s
md5 <file_in_unexpected_location>.spr <canonical_file>.spr
```

If MD5s match → byte-identical duplicate; note it in this file and let the scan skip it
naturally (or explicitly via a comment). Do NOT add special scan handling for duplicates.

If MD5s differ → the file has unique content and needs to be handled.

---

## Known Ignored / Duplicate Files

### `_female` / `_male` Body Directory Anomaly

`idavoll_grf_extractor` may produce a `human/body/_female/` directory alongside the standard `female/`
when a GRF entry uses `_여` instead of `여` as the gender directory segment. Confirmed case:
`어쌔신크로스_female.spr` — byte-identical (MD5 match) to the copy already in `female/`.
Safe to ignore; the scan only reads `female/` and `male/`.

### `human/accessory/` under the weapon job dirs

`human/accessory/{male,female}/` contains two player-appearance sprites:
`female_hair_protector.spr` and `male_blush_of_groom.spr`. These are NOT weapon overlays.
`spr_stems_in` is non-recursive so it returns nothing for `human/accessory/` (which only
contains subdirs). They never enter the manifest. Intentionally excluded.

### `pecopeco_paladin/` duplicate crusader weapon files

`human/pecopeco_paladin/` contains `pecopeco_crusader_female_1123.spr` and
`pecopeco_crusader_male_1466.spr` — byte-identical (MD5 confirmed) copies of the same files
already in `human/pecopeco_crusader/`. The GRF stores them in both locations. The scan picks
them up from `pecopeco_crusader/` and skips the paladin copies (prefix mismatch). No action needed.

### `lord_knight_남''.spr` — CP949 mojibake duplicate

`human/body/male/lord_knight_남''.spr` is byte-identical (MD5 confirmed) to
`human/body/male/lord_knight_male.spr`. The GRF stores the file twice: once with the
standard `남` gender suffix (→ `lord_knight_male`) and once with extra bytes that became
the apostrophes `''` after CP949 decoding — likely a GRF authoring artifact. The scan
skips any stem containing `''` via an explicit check in `scan_bodies`. No female counterpart
of the artifact exists.

### Shield sprites duplicated into human weapon job directories

Several jobs have shield sprites stored under both `sprite/shield/{job}/` (canonical) and
`sprite/human/{job}/` (duplicate). Some are byte-identical; the alchemist copies are different
content but confirmed broken (wrong sprite, not interchangeable with the shield-dir version).

Affected names across all jobs: `guard`, `buckler_`, `mirror_shield_`, `te_woe_shield`.
`scan_weapons` skips any stem whose parsed weapon name matches this list via `SHIELD_NAMES`.
`scan_shields` picks them up correctly from `sprite/shield/`.

### `swordsman_female_two_handed_sword.act` — wrong action layout

This ACT file was authored with a monster-style 40-action layout instead of the correct
104-action weapon layout. The male counterpart is correct. The scan overrides the `act` path
for this entry to point at `swordsman_female_sword.act`, which is layout-compatible
(verified externally). The `.spr` file is used as-is.

### Rebellion weapon files mis-copied into body directory

`human/body/{gender}/rebellion_{gender}_{weapon}.spr` (pistol, machine_gun) are
byte-identical (MD5 confirmed) copies of the weapon sprites in `human/rebellion/`.
There is no Rebellion body overlay system. These are ignored by the body scan
(stem does not cleanly strip a gender suffix) and correctly absent from the manifest.

## Costume Body Sprites

Body sprites also exist in **costume subdirectories** under `human/body/{gender}/`:

```
human/body/female/costume_1/arch_bishop_female_1.spr
human/body/female/costume_1/arch_bishop_female_1.act
human/body/female/costume_1/madogear_female_1.spr
...
human/body/female/costume_2/...
human/body/female/costume_3/...
human/body/female/costume_4/...
```

- `costume_1/` has ~37 sprites; `costume_2/`–`costume_4/` have 1–2 each
- Full 104-action body replacements, triggered by equipping a costume item in a costume slot
- Analogous to dancer pants and peco mount toggles — same Option B treatment applies:
  `job = "{job}_costume_{n}"`, `gender = "{gender}"`
- Stem convention: `{job}_{gender}_{n}.spr` where `{n}` matches the costume dir number
- Scanned by `scan_bodies` which iterates `body/{gender}/costume_{n}/` subdirs, strips
  `_{gender}_{n}` suffix, and produces `job = "{job}_costume_{n}"`
- Dancer pants (`dancer_female_pants`) is treated as `dancer_costume_1` by the scan
  (special-cased in the flat dir scan, no translation change needed)

## Future Considerations

### Per-action spritesheet splitting

Currently each sprite exports as a single wide horizontal strip with all actions concatenated.
A potential future direction is splitting output into one spritesheet per action, e.g.:

```
novice_male/
  stand.png + stand.json
  walk.png  + walk.json
  attack.png + attack.json
  ...
```

**Motivation:** Memory efficiency — the consumer loads only the actions it needs rather than
the full strip. Also improves readability when inspecting exported files.

**Trade-offs to resolve before implementing:**

**1. Granularity: per-group vs per-direction**
Two natural split points exist:
- **Per action group** (recommended): one file per animation type, containing all 8 direction
  variants as rows or columns. Player sprites → 13 files (`stand.png`, `walk.png`, …).
  Consumer loads `stand.png` and computes which row/column is the active direction.
- **Per direction**: one file per action index (action_idx = group × 8 + dir). Player sprites
  → 104 files (`stand_s.png`, `stand_sw.png`, …). Simpler per-file structure but very high
  file count — 104 files per player sprite × thousands of sprites = hundreds of thousands of files.

Per-group is likely the right call, but requires the JSON to describe direction layout within each file.

**2. Action naming — non-standard sprite types**
Player, headgear, garment, weapon, and shield all have a fixed 104-action layout with
13 known group names. Naming is deterministic.

Monsters and NPCs do not follow this:
- Standard monsters: 40 actions = 5 named groups (stand, move, attack, damage, dead) × 8 dirs
- Non-standard monsters (e.g. Poring: 72 actions = 9 groups): the extra groups have no defined
  names in any known spec. The current `dump` fallback is `action_{idx:03}_{dir}` — that would
  become `action_040.png`, `action_048.png`, etc. Unintuitive for a consumer.
- NPCs: typically 8 actions (1 group); the group has no standard name beyond "idle".

Options: a per-monster action name table (high maintenance), a generic `action_{n}.png`
fallback for unknown groups (simple but opaque), or restricting per-action splitting to
player-type sprites only (pragmatic for the near term).

**3. Sparse sprite types (weapon, shield)**
Weapons and shields have 104 action slots but only 2–3 groups are populated (e.g. `atk_wait`,
`attack2`). Exporting all 13 groups would produce 10+ mostly-empty files per sprite.
Options: export only visible groups (consumer must handle missing files), or export all groups
with empty placeholder images (predictable file layout but wasteful).

**4. Consumer API contract**
Current: consumer loads one file, reads frame rects from JSON, seeks to the right rect by action+frame index.
Per-group split: consumer must first select the right file by action name, then read frame rects within it.
This is cleaner for streaming/lazy-loading but requires the consumer to know action group names,
which loops back to the naming problem above for non-standard types.

---

## Next Steps: Sprite Types Not Yet Supported

### Monster (`sprite/monster/`)

- **Structure:** Flat directory (~1265 sprite pairs). No job or gender subdirectory.
- **Naming:** `{name}.spr` / `{name}.act` — English names after extraction, 26 still Korean.
- **Actions:** Standard monster layout (5 types × 8 dirs = 40 actions). Some have more
  (e.g. Poring: 72 actions = 9 types). No attach points, no child sprites.
- **Implementation:** Add `MonsterEntry { name, spr, act }`, `scan_monsters`, batch output
  to `monster/<name>/`. No z-order compositing needed — standalone sprite.
- **Translation gap:** 26 Korean-named files remain; add entries to `translations.toml`
  before scanning. Run `ls sprite/monster/ | grep -E '[가-힣]'` after re-extraction to find them.

---

### NPC (`sprite/npc/`)

- **Structure:** Flat directory (~1130 sprite pairs). No subdirectories.
- **Naming:** `{name}.spr` / `{name}.act` — mostly ASCII, 1 Korean file remaining.
- **Actions:** Typically 8 actions (1 type × 8 dirs); some NPCs have more.
  See NPC section above for direction structure.
- **Implementation:** Identical to monster: `NpcEntry { name, spr, act }`, `scan_npcs`,
  output to `npc/<name>/`. Standalone sprite, no compositing.

---

### Homunculus (`sprite/homun/`)

- **Structure:** Flat directory, 21 sprite pairs. No subdirectories.
- **Naming convention:**
  - `{name}.spr` — base form
  - `{name}2.spr` — evolved form (e.g. Amistr → Castilla)
  - `{name}_h.spr` / `{name}_h2.spr` — variant (exact meaning TBD; may be hungry/starving state)
  - Known names: `amistr`, `filir`, `lif`, `vanilmirth`; mercenary-class variants: `mer_bayeri`,
    `mer_dieter`, `mer_eira`, `mer_eleanor`, `mer_sera`
- **Actions:** Uses monster action layout (standalone, no compositing). No attach points.
- **Implementation:** `HomuncEntry { name, spr, act }`, `scan_homun`, output to `homun/<name>/`.
  The `_h` and `2` variants are just additional `name` entries — no special handling needed.

---

### Item Drop Sprites (`sprite/item/`)

- **Structure:** Flat directory (~6202 sprite pairs). No subdirectories.
- **Naming:** `{name}.spr` / `{name}.act`. Heavily Korean — 2495 files still Korean-named.
  Many item names are not in `translations.toml`; would need bulk addition or an alternative
  lookup (item name table from rAthena DB).
- **Actions:** Very simple — typically 1 action with 1–3 frames (drop/idle animation).
  No attach points, no compositing.
- **Translation strategy:** Consider using the rAthena item DB (already loaded by idavoll_grf_extractor
  for headgear) to map Korean item res names to AegisNames for the item sprite directory.
  This would avoid needing per-item entries in `translations.toml`.
- **Implementation:** `ItemEntry { name, spr, act }`, `scan_items`, output to `item/<name>/`.

---

### Doram (`sprite/doram/`)

- **Structure:** `doram/body/{gender}/` and `doram/head/{gender}/` — mirrors human layout.
  Also has a `doram/summoner/` directory (purpose TBD).
- **Compositing:** Body + head compositing same as human (attach points, IMF z-order).
  No headgear, garment, or weapon layers — Doram does not use those slots.
- **IMF:** Check `data/imf/` for Doram IMF files (likely `{name}_{gender}.imf`).
- **Implementation:** Reuse `scan_bodies`/`scan_heads` logic with `doram/body` and `doram/head`
  as the scan roots, or add a `--doram` flag that adds `doram` entries to the existing
  `body`/`head` manifest sections. Output path would need a `doram/` prefix to avoid
  colliding with human entries in `body/<job>/` and `head/<id>/`.
