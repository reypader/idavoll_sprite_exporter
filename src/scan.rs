use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

use crate::manifest::*;

// ---------------------------------------------------------------------------
// headgear_slots.toml types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct HeadgearSlotEntry {
    view: u32,
    slot: String,
    accname: Option<String>,
}

#[derive(Deserialize)]
struct HeadgearSlotsFile {
    headgear: Vec<HeadgearSlotEntry>,
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub fn scan(data_root: &Path, slots_file: &Path, output: &Path, types: Option<&[String]>) -> Result<()> {
    let want = |t: &str| types.is_some_and(|ts| ts.iter().any(|x| x == t));
    let slots_text = std::fs::read_to_string(slots_file)
        .with_context(|| format!("reading {}", slots_file.display()))?;
    let slots_data: HeadgearSlotsFile = toml::from_str(&slots_text)
        .with_context(|| format!("parsing {}", slots_file.display()))?;

    // accname -> (view_id, slot string)
    let accname_map: HashMap<String, (u32, String)> = slots_data
        .headgear
        .iter()
        .filter_map(|e| {
            e.accname
                .as_ref()
                .map(|name| (name.clone(), (e.view, e.slot.clone())))
        })
        .collect();

    let data_str = data_root.to_string_lossy().into_owned();

    let mut m = Manifest {
        data_root: data_str,
        output_root: "./output".to_string(),
        body: Vec::new(),
        head: Vec::new(),
        headgear: Vec::new(),
        garment: Vec::new(),
        weapon: Vec::new(),
        shield: Vec::new(),
        shadow: Vec::new(),
        projectile: Vec::new(),
    };

    if want("shadow")  { scan_shadow(data_root, &mut m); }
    if want("body")    { scan_bodies(data_root, &mut m)?; }
    if want("head")    { scan_heads(data_root, &mut m)?; }
    if want("headgear"){ scan_headgears(data_root, &accname_map, &mut m)?; }
    if want("garment") { scan_garments(data_root, &mut m)?; }
    if want("weapon")  { scan_weapons(data_root, &mut m)?; }
    if want("shield")      { scan_shields(data_root, &mut m)?; }
    if want("projectile")  { scan_projectiles(data_root, &mut m)?; }

    let toml_text = toml::to_string_pretty(&m)?;
    std::fs::write(output, &toml_text)
        .with_context(|| format!("writing {}", output.display()))?;

    println!("Wrote manifest: {}", output.display());
    println!(
        "  bodies={} heads={} headgears={} garments={} weapons={} shields={} shadow={} projectiles={}",
        m.body.len(),
        m.head.len(),
        m.headgear.len(),
        m.garment.len(),
        m.weapon.len(),
        m.shield.len(),
        m.shadow.len(),
        m.projectile.len(),
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Per-type scanners
// ---------------------------------------------------------------------------

fn scan_shadow(data_root: &Path, m: &mut Manifest) {
    let sprite_root = data_root.join("sprite");
    if sprite_root.join("shadow.spr").exists() && sprite_root.join("shadow.act").exists() {
        m.shadow.push(ShadowEntry {
            spr: "sprite/shadow.spr".to_string(),
            act: "sprite/shadow.act".to_string(),
        });
    }
}

fn scan_bodies(data_root: &Path, m: &mut Manifest) -> Result<()> {
    // IMF anchor files live in data/imf/ (flat directory, same level as data/sprite/).
    // The single co-located .imf found under data/sprite/ is a GRF duplicate artifact.
    let imf_root = data_root.join("imf");

    // NOTE: grf_extract may also produce a human/body/_female/ directory alongside female/
    // when the GRF stores a file under _여 instead of 여. Confirmed to be a byte-identical
    // duplicate of the same file in female/ — intentionally ignored here.
    for gender in ["male", "female"] {
        let dir = data_root.join("sprite/human/body").join(gender);
        if !dir.exists() {
            continue;
        }
        let mut entries = spr_stems_in(&dir)?;
        entries.sort();
        let opposite = if gender == "male" { "female" } else { "male" };
        for stem in entries {
            // Skip _h_ hair-hidden duplicates.
            if stem.contains("_h_") {
                continue;
            }
            // Skip GRF artifact duplicate: lord_knight_남'' is byte-identical to
            // lord_knight_male.spr — the GRF stores both; the apostrophes are CP949 mojibake.
            if stem.contains("''") {
                continue;
            }
            let suffix = format!("_{gender}");
            let job = if let Some(j) = stem.strip_suffix(&suffix) {
                // Normal case: stem ends with _{gender}.
                j.to_string()
            } else if stem == format!("dancer_{gender}_pants") {
                // Dancer pants variant: full-replacement body triggered by pants equipment.
                // Treated as costume slot 1 equivalent for naming consistency.
                "dancer_costume_1".to_string()
            } else if !stem.contains(&format!("_{gender}"))
                && !stem.contains(&format!("_{opposite}"))
            {
                // No gender suffix at all: gender-specific sprites stored without suffix
                // (e.g. mercenary jobs, wedding dress). Use directory gender.
                stem.clone()
            } else {
                // Contains a gender marker but doesn't cleanly strip (weapon overlay,
                // wrong-dir file, equipment variant). Skip.
                continue;
            };
            let rel = format!("sprite/human/body/{gender}/{stem}");
            let imf_path = imf_root.join(format!("{stem}.imf"));
            m.body.push(BodyEntry {
                job,
                gender: gender.to_string(),
                spr: format!("{rel}.spr"),
                act: format!("{rel}.act"),
                imf: imf_path
                    .exists()
                    .then(|| format!("imf/{stem}.imf")),
            });
        }

        // Costume subdirectories: sprite/human/body/{gender}/costume_{n}/
        // Full-replacement body sprites triggered by costume-slot equipment.
        // Stem convention: {job}_{gender}_{n} → job = "{job}_costume_{n}".
        let mut costume_dirs = dir_names(&dir)?;
        costume_dirs.sort();
        for subdir_name in costume_dirs {
            let Some(n_str) = subdir_name.strip_prefix("costume_") else {
                continue;
            };
            if n_str.parse::<u32>().is_err() {
                continue;
            }
            let costume_dir = dir.join(&subdir_name);
            let mut entries = spr_stems_in(&costume_dir)?;
            entries.sort();
            let suffix = format!("_{gender}_{n_str}");
            for stem in entries {
                let Some(job_base) = stem.strip_suffix(&suffix) else {
                    continue;
                };
                let job = format!("{job_base}_costume_{n_str}");
                let rel = format!("sprite/human/body/{gender}/{subdir_name}/{stem}");
                m.body.push(BodyEntry {
                    job,
                    gender: gender.to_string(),
                    spr: format!("{rel}.spr"),
                    act: format!("{rel}.act"),
                    imf: None,
                });
            }
        }
    }
    Ok(())
}

fn scan_heads(data_root: &Path, m: &mut Manifest) -> Result<()> {
    for gender in ["male", "female"] {
        let dir = data_root.join("sprite/human/head").join(gender);
        if !dir.exists() {
            continue;
        }
        let mut entries = spr_stems_in(&dir)?;
        entries.sort_by_key(|s| {
            let suffix = format!("_{gender}");
            s.strip_suffix(&suffix)
                .and_then(|id| id.parse::<u32>().ok())
                .unwrap_or(u32::MAX)
        });
        for stem in entries {
            let suffix = format!("_{gender}");
            let Some(id_str) = stem.strip_suffix(&suffix) else {
                continue;
            };
            let Ok(id) = id_str.parse::<u32>() else {
                continue;
            };
            let rel = format!("sprite/human/head/{gender}/{stem}");
            m.head.push(HeadEntry {
                id,
                gender: gender.to_string(),
                spr: format!("{rel}.spr"),
                act: format!("{rel}.act"),
                imf: None,
            });
        }
    }
    Ok(())
}

fn scan_headgears(
    data_root: &Path,
    accname_map: &HashMap<String, (u32, String)>,
    m: &mut Manifest,
) -> Result<()> {
    for (gender, prefix) in [("male", "male_"), ("female", "female_")] {
        let dir = data_root.join("sprite/accessory").join(gender);
        if !dir.exists() {
            continue;
        }
        let mut entries = spr_stems_in(&dir)?;
        entries.sort();
        for stem in entries {
            let Some(accname) = stem.strip_prefix(prefix) else {
                continue;
            };
            let (view, slot) = accname_map
                .get(accname)
                .map(|(v, s)| (*v, s.clone()))
                .unwrap_or((0, "Head_Top".to_string()));
            let rel = format!("sprite/accessory/{gender}/{stem}");
            m.headgear.push(HeadgearEntry {
                name: accname.to_string(),
                view,
                slot,
                gender: gender.to_string(),
                spr: format!("{rel}.spr"),
                act: format!("{rel}.act"),
            });
        }
    }
    Ok(())
}

fn scan_garments(data_root: &Path, m: &mut Manifest) -> Result<()> {
    let robe_dir = data_root.join("sprite/robe");
    if !robe_dir.exists() {
        return Ok(());
    }
    let mut garment_names = dir_names(&robe_dir)?;
    garment_names.sort();
    for garment_name in garment_names {
        let garment_path = robe_dir.join(&garment_name);
        for gender in ["male", "female"] {
            let gender_dir = garment_path.join(gender);
            if !gender_dir.exists() {
                continue;
            }
            let mut entries = spr_stems_in(&gender_dir)?;
            entries.sort();
            for stem in entries {
                let suffix = format!("_{gender}");
                let Some(job) = stem.strip_suffix(&suffix) else {
                    continue;
                };
                let rel = format!("sprite/robe/{garment_name}/{gender}/{stem}");
                m.garment.push(GarmentEntry {
                    name: garment_name.clone(),
                    job: job.to_string(),
                    gender: gender.to_string(),
                    spr: format!("{rel}.spr"),
                    act: format!("{rel}.act"),
                });
            }
        }
    }
    Ok(())
}

// NOTE: known files intentionally not scanned:
//
//   human/accessory/{male,female}/female_hair_protector, male_blush_of_groom
//     — player-appearance sprites stored one level deeper than a flat job dir;
//       spr_stems_in returns nothing for the accessory/ dir so they are silently
//       skipped. They are not weapon overlays.
//
//   human/pecopeco_paladin/pecopeco_crusader_{female_1123,male_1466}.spr
//     — byte-identical duplicates of the same files already in pecopeco_crusader/.
//       The GRF stores them in both locations. The scan picks them up from
//       pecopeco_crusader/ (where they belong) and ignores the paladin copies.
fn scan_weapons(data_root: &Path, m: &mut Manifest) -> Result<()> {
    let human_dir = data_root.join("sprite/human");
    if !human_dir.exists() {
        return Ok(());
    }
    let mut job_dirs = dir_names(&human_dir)?;
    job_dirs.sort();
    for job_dir_name in job_dirs {
        if job_dir_name == "body" || job_dir_name == "head" {
            continue;
        }
        // human/accessory/{male,female}/ contains two player-appearance sprites
        // (female_hair_protector, male_blush_of_groom) that are not weapon overlays.
        // They are stored one level deeper than a flat job dir, so spr_stems_in returns
        // nothing — they are silently skipped. Confirmed intentional; do not add handling.

        let job_path = human_dir.join(&job_dir_name);
        let mut entries = spr_stems_in(&job_path)?;

        // Mercenary weapons have no gender marker: {job_prefix}_mercenary_{weapon}[_slash_glow]
        // Emit one entry for each gender pointing at the same shared file.
        if job_dir_name == "mercenary" {
            entries.sort();
            for stem in &entries {
                let (weapon_stem, slot) = if let Some(base) = stem.strip_suffix("_slash_glow") {
                    (base.to_string(), "slash".to_string())
                } else {
                    (stem.clone(), "weapon".to_string())
                };
                // weapon_stem = "{job_prefix}_mercenary_{weapon_name}"
                let Some(merc_pos) = weapon_stem.find("_mercenary_") else {
                    continue;
                };
                let job_prefix = &weapon_stem[..merc_pos];
                let weapon_name = &weapon_stem[merc_pos + "_mercenary_".len()..];
                let job = format!("{job_prefix}_mercenary");
                let rel = format!("sprite/human/mercenary/{stem}");
                for gender in ["male", "female"] {
                    m.weapon.push(WeaponEntry {
                        name: weapon_name.to_string(),
                        job: job.clone(),
                        gender: gender.to_string(),
                        slot: slot.clone(),
                        spr: format!("{rel}.spr"),
                        act: format!("{rel}.act"),
                    });
                }
            }
            continue;
        }

        // Shield sprites stored under some weapon job dirs (GRF artifact — copies of the
        // canonical files in sprite/shield/). Skip by weapon name; shield dir is authoritative.
        const SHIELD_NAMES: &[&str] = &["guard", "buckler_", "mirror_shield_", "te_woe_shield"];

        entries.sort();
        let prefix = format!("{job_dir_name}_");
        for stem in entries {
            let Some(rest) = stem.strip_prefix(&prefix) else {
                continue;
            };
            // rest = {gender}_{weaponname}[_검광]
            let Some(us) = rest.find('_') else { continue };
            let gender = &rest[..us];
            if gender != "male" && gender != "female" {
                continue;
            }
            let weapon_part = &rest[us + 1..];
            // Skip shield sprites misplaced in weapon dirs.
            let base_weapon = weapon_part.strip_suffix("_slash_glow").unwrap_or(weapon_part);
            if SHIELD_NAMES.contains(&base_weapon) {
                continue;
            }
            let (weapon_name, slot) = if let Some(base) = weapon_part.strip_suffix("_slash_glow") {
                (base.to_string(), "slash".to_string())
            } else {
                (weapon_part.to_string(), "weapon".to_string())
            };
            let rel = format!("sprite/human/{job_dir_name}/{stem}");
            // swordsman_female_two_handed_sword.act was authored with monster-layout
            // 40 actions instead of the correct 104. Fall back to the sword ACT which
            // is layout-compatible (verified externally).
            let act = if job_dir_name == "swordsman"
                && gender == "female"
                && weapon_name == "two_handed_sword"
                && slot == "weapon"
            {
                "sprite/human/swordsman/swordsman_female_sword.act".to_string()
            } else {
                format!("{rel}.act")
            };
            m.weapon.push(WeaponEntry {
                name: weapon_name,
                job: job_dir_name.clone(),
                gender: gender.to_string(),
                slot,
                spr: format!("{rel}.spr"),
                act,
            });
        }
    }
    Ok(())
}

fn scan_shields(data_root: &Path, m: &mut Manifest) -> Result<()> {
    let shield_dir = data_root.join("sprite/shield");
    if !shield_dir.exists() {
        return Ok(());
    }
    let mut job_dirs = dir_names(&shield_dir)?;
    job_dirs.sort();
    for job in job_dirs {
        let job_path = shield_dir.join(&job);
        let mut entries = spr_stems_in(&job_path)?;
        entries.sort();
        let prefix = format!("{job}_");
        for stem in entries {
            let Some(rest) = stem.strip_prefix(&prefix) else {
                continue;
            };
            // rest = {gender}_{shield_name}
            let Some(us) = rest.find('_') else { continue };
            let gender = &rest[..us];
            if gender != "male" && gender != "female" {
                continue;
            }
            let shield_name = rest[us + 1..].to_string();
            let rel = format!("sprite/shield/{job}/{stem}");
            m.shield.push(ShieldEntry {
                name: shield_name,
                job: job.clone(),
                gender: gender.to_string(),
                spr: format!("{rel}.spr"),
                act: format!("{rel}.act"),
            });
        }
    }
    Ok(())
}

fn scan_projectiles(data_root: &Path, m: &mut Manifest) -> Result<()> {
    let monster_dir = data_root.join("sprite/monster");
    if !monster_dir.exists() {
        return Ok(());
    }

    // Canonical name overrides: some projectile sprites are named after the unit
    // that fires them but represent a generic reusable effect.
    // e.g. skel_archer_arrow is the standard arrow sprite reused by all archers.
    let name_overrides: std::collections::HashMap<&str, &str> = [
        ("skel_archer_arrow", "arrow"),
    ].into_iter().collect();

    let mut entries = spr_stems_in(&monster_dir)?;
    entries.sort();
    for stem in entries {
        let act_path = monster_dir.join(format!("{stem}.act"));
        let action_count = match act_action_count(&act_path) {
            Some(n) => n,
            None => continue,
        };
        if action_count != 1 && action_count != 8 {
            continue;
        }
        let name = name_overrides.get(stem.as_str()).copied().unwrap_or(&stem);
        let rel = format!("sprite/monster/{stem}");
        m.projectile.push(crate::manifest::ProjectileEntry {
            name: name.to_string(),
            spr: format!("{rel}.spr"),
            act: format!("{rel}.act"),
        });
    }
    Ok(())
}

/// Read the action count from an ACT file header without fully parsing it.
/// ACT layout: 2-byte magic "AC", 2-byte version, 2-byte action count.
fn act_action_count(path: &Path) -> Option<u16> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).ok()?;
    let mut buf = [0u8; 6];
    f.read_exact(&mut buf).ok()?;
    if &buf[..2] != b"AC" {
        return None;
    }
    Some(u16::from_le_bytes([buf[4], buf[5]]))
}

// ---------------------------------------------------------------------------
// Directory helpers
// ---------------------------------------------------------------------------

/// Return the file stems of all .spr files in a directory.
fn spr_stems_in(dir: &Path) -> Result<Vec<String>> {
    let mut result = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("spr") {
            continue;
        }
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            result.push(stem.to_string());
        }
    }
    Ok(result)
}

/// Return the names of all subdirectories in a directory.
fn dir_names(dir: &Path) -> Result<Vec<String>> {
    let mut result = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir()
            && let Some(name) = entry.file_name().to_str() {
                result.push(name.to_string());
            }
    }
    Ok(result)
}
