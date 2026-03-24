use anyhow::{Context, Result};
use std::path::Path;

use crate::manifest::{self, Manifest};
use crate::zorder::SpriteKind;
use crate::{act, imf, output, spr};

pub fn batch(
    manifest_path: &Path,
    output_override: Option<&Path>,
    types: Option<&[String]>,
) -> Result<()> {
    let want = |t: &str| types.is_some_and(|ts| ts.iter().any(|x| x == t));

    let text = std::fs::read_to_string(manifest_path)
        .with_context(|| format!("reading {}", manifest_path.display()))?;
    let m: Manifest =
        toml::from_str(&text).with_context(|| format!("parsing {}", manifest_path.display()))?;

    let data_root = Path::new(&m.data_root);
    let out_root: &Path = match output_override {
        Some(p) => p,
        None => Path::new(&m.output_root),
    };
    std::fs::create_dir_all(out_root)?;

    let mut skip_log = String::new();
    let mut exported = 0usize;
    let mut skipped = 0usize;

    // Bodies
    if want("body") {
        for entry in &m.body {
            let spr_path = data_root.join(&entry.spr);
            let act_path = data_root.join(&entry.act);
            let out_dir = out_root.join("body").join(&entry.job).join(&entry.gender);
            if let Some(reason) = missing(&spr_path, &act_path) {
                log_skip(&mut skip_log, "body", &toml::to_string(entry)?, &reason);
                skipped += 1;
                continue;
            }
            let imf_data = load_imf(entry.imf.as_deref().map(|p| data_root.join(p)))?;
            export(&spr_path, &act_path, &out_dir, SpriteKind::Body, imf_data.as_ref())?;
            exported += 1;
        }
    }

    // Heads
    if want("head") {
        for entry in &m.head {
            let spr_path = data_root.join(&entry.spr);
            let act_path = data_root.join(&entry.act);
            let out_dir = out_root
                .join("head")
                .join(entry.id.to_string())
                .join(&entry.gender);
            if let Some(reason) = missing(&spr_path, &act_path) {
                log_skip(&mut skip_log, "head", &toml::to_string(entry)?, &reason);
                skipped += 1;
                continue;
            }
            let imf_data = load_imf(entry.imf.as_deref().map(|p| data_root.join(p)))?;
            export(&spr_path, &act_path, &out_dir, SpriteKind::Head, imf_data.as_ref())?;
            exported += 1;
        }
    }

    // Headgears
    if want("headgear") {
        for entry in &m.headgear {
            let spr_path = data_root.join(&entry.spr);
            let act_path = data_root.join(&entry.act);
            let out_dir = out_root
                .join("headgear")
                .join(&entry.name)
                .join(&entry.gender);
            let slot =
                manifest::parse_headgear_slot(&entry.slot).map_err(|e| anyhow::anyhow!(e))?;
            if let Some(reason) = missing(&spr_path, &act_path) {
                log_skip(&mut skip_log, "headgear", &toml::to_string(entry)?, &reason);
                skipped += 1;
                continue;
            }
            export(
                &spr_path,
                &act_path,
                &out_dir,
                SpriteKind::Headgear { slot },
                None,
            )?;
            exported += 1;
        }
    }

    // Garments
    if want("garment") {
        for entry in &m.garment {
            let spr_path = data_root.join(&entry.spr);
            let act_path = data_root.join(&entry.act);
            let out_dir = out_root
                .join("garment")
                .join(&entry.name)
                .join(&entry.job)
                .join(&entry.gender);
            if let Some(reason) = missing(&spr_path, &act_path) {
                log_skip(&mut skip_log, "garment", &toml::to_string(entry)?, &reason);
                skipped += 1;
                continue;
            }
            export(&spr_path, &act_path, &out_dir, SpriteKind::Garment, None)?;
            exported += 1;
        }
    }

    // Weapons
    if want("weapon") {
        for entry in &m.weapon {
            let spr_path = data_root.join(&entry.spr);
            let act_path = data_root.join(&entry.act);
            let out_dir = out_root
                .join("weapon")
                .join(&entry.name)
                .join(&entry.job)
                .join(&entry.gender)
                .join(&entry.slot);
            let slot =
                manifest::parse_weapon_slot(&entry.slot).map_err(|e| anyhow::anyhow!(e))?;
            if let Some(reason) = missing(&spr_path, &act_path) {
                log_skip(&mut skip_log, "weapon", &toml::to_string(entry)?, &reason);
                skipped += 1;
                continue;
            }
            export(
                &spr_path,
                &act_path,
                &out_dir,
                SpriteKind::Weapon { slot },
                None,
            )?;
            exported += 1;
        }
    }

    // Shields
    if want("shield") {
        for entry in &m.shield {
            let spr_path = data_root.join(&entry.spr);
            let act_path = data_root.join(&entry.act);
            let out_dir = out_root
                .join("shield")
                .join(&entry.name)
                .join(&entry.job)
                .join(&entry.gender);
            if let Some(reason) = missing(&spr_path, &act_path) {
                log_skip(&mut skip_log, "shield", &toml::to_string(entry)?, &reason);
                skipped += 1;
                continue;
            }
            export(&spr_path, &act_path, &out_dir, SpriteKind::Shield, None)?;
            exported += 1;
        }
    }

    // Shadow
    if want("shadow") {
        for entry in &m.shadow {
            let spr_path = data_root.join(&entry.spr);
            let act_path = data_root.join(&entry.act);
            let out_dir = out_root.join("shadow");
            if let Some(reason) = missing(&spr_path, &act_path) {
                log_skip(&mut skip_log, "shadow", &toml::to_string(entry)?, &reason);
                skipped += 1;
                continue;
            }
            export(&spr_path, &act_path, &out_dir, SpriteKind::Shadow, None)?;
            exported += 1;
        }
    }

    // Projectiles
    if want("projectile") {
        for entry in &m.projectile {
            let spr_path = data_root.join(&entry.spr);
            let act_path = data_root.join(&entry.act);
            let out_dir = out_root.join("projectile");
            if let Some(reason) = missing(&spr_path, &act_path) {
                log_skip(&mut skip_log, "projectile", &toml::to_string(entry)?, &reason);
                skipped += 1;
                continue;
            }
            export_named(&spr_path, &act_path, &entry.name, &out_dir, SpriteKind::Body, None)?;
            exported += 1;
        }
    }

    println!("Exported: {exported}  Skipped: {skipped}");

    if !skip_log.is_empty() {
        let skip_path = out_root.join("skipped.toml");
        std::fs::write(&skip_path, &skip_log)?;
        println!("Skip log: {}", skip_path.display());
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn missing(spr: &Path, act: &Path) -> Option<String> {
    if !spr.exists() {
        return Some(format!("spr not found: {}", spr.display()));
    }
    if !act.exists() {
        return Some(format!("act not found: {}", act.display()));
    }
    None
}

fn log_skip(log: &mut String, table: &str, entry_toml: &str, reason: &str) {
    log.push_str(&format!("# SKIPPED: {reason}\n"));
    log.push_str(&format!("[[{table}]]\n"));
    log.push_str(entry_toml);
    log.push('\n');
}

fn load_imf(path: Option<std::path::PathBuf>) -> Result<Option<imf::ImfFile>> {
    match path {
        Some(p) if p.exists() => Ok(Some(imf::ImfFile::parse(&std::fs::read(&p)?)?)),
        _ => Ok(None),
    }
}

fn export(
    spr_path: &Path,
    act_path: &Path,
    out_dir: &Path,
    kind: SpriteKind,
    imf: Option<&imf::ImfFile>,
) -> Result<()> {
    let base_name = spr_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("sprite");
    export_named(spr_path, act_path, base_name, out_dir, kind, imf)
}

fn export_named(
    spr_path: &Path,
    act_path: &Path,
    name: &str,
    out_dir: &Path,
    kind: SpriteKind,
    imf: Option<&imf::ImfFile>,
) -> Result<()> {
    let spr_data = std::fs::read(spr_path)?;
    let act_data = std::fs::read(act_path)?;
    let spr = spr::SprFile::parse(&spr_data)?;
    let act = act::ActFile::parse(&act_data)?;
    std::fs::create_dir_all(out_dir)?;
    output::export(&spr, &act, name, out_dir, None, Some(&kind), imf)
}
