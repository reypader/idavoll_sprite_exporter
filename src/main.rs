mod act;
mod batch;
mod composite;
mod dump;
mod imf;
mod manifest;
mod output;
mod scan;
mod spr;
mod zorder;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "idavoll-sprite-exporter")]
#[command(about = "Convert Ragnarok Online ACT/SPR sprites to Aseprite spritesheets")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Export ACT/SPR to Aseprite spritesheet PNG + JSON
    Export {
        /// Input SPR file
        spr: PathBuf,

        /// Input ACT file
        act: PathBuf,

        /// Output directory
        #[arg(short, long, default_value = ".")]
        output: PathBuf,

        /// Only export these action indices (comma-separated, e.g. "0,8,16")
        #[arg(long)]
        actions: Option<String>,

        /// Sprite kind for z-order metadata in the output JSON.
        /// One of: shadow, body, head, weapon, weapon-slash, headgear, shield, garment.
        /// The zOrder field in the output is a recommended default; runtimes with access
        /// to per-item Lua tables (e.g. for garments) should override it as needed.
        #[arg(long, value_name = "KIND")]
        kind: Option<String>,

        /// Headgear compositing slot (0–3). Required when --kind headgear.
        /// Slot 0 = upper, 1 = middle, 2 = lower, 3 = extra.
        #[arg(long, value_name = "SLOT")]
        headgear_slot: Option<u8>,

        /// Body IMF file for per-frame head-behind-body priority (only used with --kind head).
        #[arg(long, value_name = "PATH")]
        imf: Option<PathBuf>,
    },

    /// Batch export sprites from a manifest TOML file
    Batch {
        /// Path to the manifest TOML file
        manifest: PathBuf,

        /// Override the output directory from the manifest
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Sprite types to process (comma-separated).
        /// Valid values: body, head, headgear, garment, weapon, shield, shadow, projectile.
        /// Example: --types body,head,headgear,weapon,shield,shadow
        #[arg(long, value_name = "TYPES")]
        types: Option<String>,
    },

    /// Scan a GRF extraction and generate a manifest TOML file
    Scan {
        /// GRF data root directory (the "data" folder inside the GRF extraction)
        data_root: PathBuf,

        /// Path to headgear_slots.toml
        #[arg(long, default_value = "headgear_slots.toml")]
        slots: PathBuf,

        /// Output manifest file path
        #[arg(short, long, default_value = "manifest.toml")]
        output: PathBuf,

        /// Sprite types to include (comma-separated).
        /// Valid values: body, head, headgear, garment, weapon, shield, shadow, projectile.
        /// Example: --types body,head,headgear,weapon,shield,shadow
        #[arg(long, value_name = "TYPES")]
        types: Option<String>,
    },

    /// Dump ACT frame/layer data for inspection
    Dump {
        /// Input ACT file
        act: PathBuf,

        /// Action indices to dump (comma-separated). Omit to show all visible actions.
        #[arg(long)]
        actions: Option<String>,

        /// Show only which actions have visible sprites (summary mode)
        #[arg(long)]
        scan: bool,
    },
}

fn main() -> Result<()> {
    let args = Args::parse();

    match args.command {
        Command::Export { spr: spr_path, act: act_path, output, actions, kind, headgear_slot, imf: imf_path } => {
            let spr_data = std::fs::read(&spr_path)?;
            let act_data = std::fs::read(&act_path)?;

            let spr = spr::SprFile::parse(&spr_data)?;
            let act = act::ActFile::parse(&act_data)?;

            println!(
                "SPR v{:#06x}: {} palette + {} RGBA images",
                spr.version,
                spr.palette_images.len(),
                spr.rgba_images.len()
            );
            println!(
                "ACT v{:#06x}: {} actions, {} events",
                act.version,
                act.actions.len(),
                act.events.len()
            );

            let base_name = spr_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("sprite")
                .to_string();

            std::fs::create_dir_all(&output)?;

            let action_filter: Option<Vec<usize>> = actions.as_deref().map(|s| {
                s.split(',')
                    .filter_map(|n| n.trim().parse().ok())
                    .collect()
            });

            let sprite_kind: Option<zorder::SpriteKind> = match kind.as_deref() {
                None => None,
                Some(s) => {
                    let mut parsed = zorder::SpriteKind::from_str(s)
                        .map_err(|e| anyhow::anyhow!(e))?;
                    if let zorder::SpriteKind::Headgear { slot } = &mut parsed {
                        *slot = headgear_slot.ok_or_else(|| {
                            anyhow::anyhow!("--headgear-slot <SLOT> is required when --kind headgear")
                        })?;
                        if *slot > 3 {
                            anyhow::bail!("--headgear-slot must be 0–3");
                        }
                    }
                    Some(parsed)
                }
            };

            let imf_file: Option<imf::ImfFile> = match imf_path {
                Some(path) => {
                    let data = std::fs::read(&path)?;
                    let parsed = imf::ImfFile::parse(&data)?;
                    println!("IMF v{:.2}", parsed.version);
                    Some(parsed)
                }
                None => None,
            };

            output::export(
                &spr,
                &act,
                &base_name,
                &output,
                action_filter.as_deref(),
                sprite_kind.as_ref(),
                imf_file.as_ref(),
            )?;
        }

        Command::Batch { manifest, output, types } => {
            let types = parse_types(types.as_deref())?;
            batch::batch(&manifest, output.as_deref(), types.as_deref())?;
        }

        Command::Scan { data_root, slots, output, types } => {
            let types = parse_types(types.as_deref())?;
            scan::scan(&data_root, &slots, &output, types.as_deref())?;
        }

        Command::Dump { act, actions, scan } => {
            let act_data = std::fs::read(&act)?;
            let act = act::ActFile::parse(&act_data)?;

            let action_filter: Option<Vec<usize>> = actions.as_deref().map(|s| {
                s.split(',')
                    .filter_map(|n| n.trim().parse().ok())
                    .collect()
            });

            if scan {
                dump::scan(&act);
            } else {
                dump::dump(&act, action_filter.as_deref());
            }
        }
    }

    Ok(())
}

const VALID_TYPES: &[&str] = &["body", "head", "headgear", "garment", "weapon", "shield", "shadow", "projectile"];

fn parse_types(types: Option<&str>) -> anyhow::Result<Option<Vec<String>>> {
    let Some(s) = types else { return Ok(None) };
    let parsed: Vec<String> = s.split(',').map(|t| t.trim().to_string()).collect();
    for t in &parsed {
        if !VALID_TYPES.contains(&t.as_str()) {
            anyhow::bail!(
                "unknown type '{t}'; valid types: {}",
                VALID_TYPES.join(", ")
            );
        }
    }
    Ok(Some(parsed))
}
