# idavoll-sprite-exporter

Converts Ragnarok Online `.act`/`.spr` sprite files into Aseprite-compatible
spritesheets (PNG + JSON). Supports single-file export, batch export from a
manifest, and GRF directory scanning to generate that manifest.

Designed to work on output produced by
[idavoll-grf-extractor](../idavoll_grf_extractor/README.md). Point it at
`extracted/data/sprite/` as the `grf_root`.

## Build

```sh
cargo build --release
# Binary: target/release/idavoll-sprite-exporter
```

## Subcommands

### `export` — Single sprite export

Convert one `.spr`/`.act` pair to a spritesheet.

```
idavoll-sprite-exporter export [OPTIONS] <spr> <act>

Arguments:
  <spr>  Input SPR file
  <act>  Input ACT file

Options:
  -o, --output <DIR>          Output directory [default: .]
      --actions <LIST>        Only export these action indices, comma-separated (e.g. "0,8,16")
      --kind <KIND>           Sprite kind for z-order metadata. One of:
                                shadow, body, head, headgear, weapon, weapon-slash,
                                shield, garment
      --headgear-slot <SLOT>  Headgear slot 0–3 (required when --kind headgear).
                                0 = upper, 1 = middle, 2 = lower, 3 = extra
      --imf <PATH>            Body IMF file for per-frame head-behind-body z-order
                              (only used with --kind head)
```

**Example:**

```sh
idavoll-sprite-exporter export sprite/human/body/male/novice_male.spr \
                        sprite/human/body/male/novice_male.act \
                        -o output/body/novice/male/ \
                        --kind body
```

---

### `scan` — Generate a manifest from a GRF data root

Walks the `data_root` directory tree and produces a `manifest.toml` describing
all discovered sprites. Requires a `headgear_slots.toml` lookup file to
determine headgear slot assignments (see below).

```
idavoll-sprite-exporter scan [OPTIONS] <data_root>

Arguments:
  <data_root>  The data/ directory from a idavoll-grf-extractor output

Options:
  --slots <PATH>      Path to headgear_slots.toml [default: headgear_slots.toml]
  -o, --output <PATH> Output manifest file [default: manifest.toml]
  --types <TYPES>     Sprite types to include, comma-separated.
                      Valid values: body, head, headgear, garment, weapon, shield, shadow, projectile
                      (opt-in; must be specified explicitly)
```

**Example — scan body, head, headgear, weapons, shields:**

```sh
idavoll-sprite-exporter scan extracted/data/ \
                    --slots headgear_slots.toml \
                    --output manifest.toml \
                    --types body,head,headgear,weapon,shield,shadow,projectile
```

The generated `manifest.toml` lists every sprite file found, grouped by type.
Edit it manually if you need to adjust paths or metadata before running `batch`.

---

### `batch` — Export all sprites from a manifest

Reads a manifest produced by `scan` and exports each sprite to an organized
output directory tree.

```
idavoll-sprite-exporter batch [OPTIONS] <manifest>

Arguments:
  <manifest>  Path to the manifest TOML file

Options:
  -o, --output <DIR>  Override the output directory from the manifest
  --types <TYPES>     Sprite types to process, comma-separated.
                      Valid values: body, head, headgear, garment, weapon, shield, shadow, projectile
                      (opt-in; must be specified explicitly)
```

**Example:**

```sh
idavoll-sprite-exporter batch manifest.toml --types body,head,headgear,weapon,shield,shadow,projectile
```

Output is organized as:

```
output/
├── body/<job>/<gender>/
├── head/<id>/<gender>/
├── headgear/<name>/<gender>/
├── garment/<name>/<job>/<gender>/
├── weapon/<name>/<job>/<gender>/<slot>/
├── shield/<name>/<job>/<gender>/
├── shadow/
└── projectile/
```

**Mercenaries** (`sword_mercenary`, `spear_mercenary`, `bow_mercenary`) appear under
`body/` and `weapon/` but differ from player jobs: their head is baked into the body
sprite (no separate head layer, no IMF anchor, no headgear/garment). The renderer
should treat them as a two-layer entity (body + weapon) rather than the full
player compositing stack.

Any sprite pair where the `.spr` or `.act` file is missing is skipped and
logged to `skipped.toml` in the output root.

---

### `dump` — Inspect ACT file contents

Print raw action/frame data from an ACT file for debugging.

```
idavoll-sprite-exporter dump [OPTIONS] <act>

Arguments:
  <act>  Input ACT file

Options:
  --actions <LIST>  Action indices to dump, comma-separated. Omit to show all.
  --scan            Summary mode: show only which actions have visible sprites
```

---

## Output format

Each sprite exports two files:

- **`<name>.png`** — Spritesheet with all frames packed horizontally.
- **`<name>.json`** — Aseprite-format metadata describing frame rectangles,
  durations, action tags, and per-frame `zOrder` values.

The `zOrder` field in the JSON is the recommended compositing layer order for
that frame. Higher values render on top.

### Entity categories and compositing layers

| Category | Actions | Action groups | Compositing layers |
|---|---|---|---|
| Human | 104 | stand, walk, sit, pickup, atk_wait, attack, damage, damage2, dead, unk, attack2, attack3, skill | body + head + headgear + garment + weapon + shield |
| Mercenary | 104 | same 13 groups (weapon visible in stand + walk + atk_wait + attack2 only) | body + weapon |
| Monster | 40 | stand, move, attack, damage, dead | body only |
| NPC | 8 | stand | body only |
| Projectile | 1 or 8 | directional still (1) or stand × 8 dirs (8) | body only |

**Monster action count variants** (stored in `sprite/monster/`, not separate files):
- **Extended (48 / 56 / 64 / 72 actions)** — multiples of 8; extra animation types beyond the standard 5. Includes cosmetic headgear variants (Korean-suffixed, e.g. `poring_backpack`) which are separate sprite files that ship with more actions than their base form. Extra actions beyond dead (index 32+) appear unused in the base game.
- **Anomalous (32 / 39 / 41 / 47 actions)** — 5 files with non-multiple-of-8 counts: `8w_soldier` (32), `increase_soil` (39), `bellare` (41), `zerom` (41), `dullahan` (47). Likely GRF authoring errors; consumers should load whatever actions are present.

---

### z-order reference

| Sprite type | Top-left dirs (2–5) | Bottom-right dirs (0,1,6,7) |
|---|---|---|
| Shadow | −1 | −1 |
| Shield | 10 | 30 |
| Body | 15 | 10 |
| Head (normal) | 20 | 15 |
| Head (behind body¹) | 14 | 9 |
| Headgear upper (slot 0) | 22 | 17 |
| Headgear middle (slot 1) | 23 | 18 |
| Headgear lower (slot 2) | 24 | 19 |
| Headgear extra (slot 3) | 25 | 20 |
| Weapon (slot 0) | 28 | 23 |
| Weapon slash (slot 1) | 29 | 24 |
| Garment | 35 (default²) | 35 (default²) |

¹ Head "behind body" is determined per-frame from the body's IMF file.
Provide the body's `.imf` path via `--imf` (export) or the manifest's `imf`
field (batch/scan).

² Garment z-order defaults to 35 (above all layers) so misconfigured garments
are visually obvious. The correct value per garment/action/frame is defined in
runtime Lua tables (`_New_DrawOnTop`, `IsTopLayer`) and should be overridden by
the consumer when those tables are available.

---

## Additional resources

### `headgear_slots.toml`

Required by the `scan` subcommand to assign each headgear sprite to its
equipment slot (upper/middle/lower). Generated automatically by `idavoll-grf-extractor`
when `--rathena-db` is provided — no separate step needed.

The file maps each headgear view ID to its slot and accname:

```toml
[[headgear]]
view = 17
slot = "Head_Top"
accname = "ribbon"
items = [2208, 2209]
```

Valid slot values: `"Head_Top"`, `"Head_Mid"`, `"Head_Low"`.

The file is human-editable. Add entries for any headgear not covered by the
rAthena DB (e.g. newer content not yet included).

---

## Typical workflow

```sh
# 1. Extract and translate the GRF (also generates headgear_slots.toml)
idavoll-grf-extractor data.grf -o extracted/ \
    --rathena-db /path/to/rathena/db \
    --headgear-slots headgear_slots.toml

# 2. Scan data root to generate a manifest
idavoll-sprite-exporter scan extracted/data/ \
    --slots headgear_slots.toml \
    --output manifest.toml \
    --types body,head,headgear,weapon,shield,shadow,projectile

# 3. Batch export
idavoll-sprite-exporter batch manifest.toml \
    --types body,head,headgear,weapon,shield,shadow,projectile \
    --output spritesheets/
```
