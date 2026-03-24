use serde::{Deserialize, Serialize};

/// Top-level manifest file structure.
#[derive(Deserialize, Serialize)]
pub struct Manifest {
    /// Path to the GRF data root directory (the "data" folder inside the GRF extraction).
    /// All sprite paths in entries are relative to this directory.
    pub data_root: String,
    /// Absolute or relative path where exported spritesheets are written.
    pub output_root: String,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub body: Vec<BodyEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub head: Vec<HeadEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headgear: Vec<HeadgearEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub garment: Vec<GarmentEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub weapon: Vec<WeaponEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shield: Vec<ShieldEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shadow: Vec<ShadowEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub projectile: Vec<ProjectileEntry>,
}

#[derive(Deserialize, Serialize)]
pub struct BodyEntry {
    pub job: String,
    pub gender: String,
    /// SPR path relative to grf_root.
    pub spr: String,
    /// ACT path relative to grf_root.
    pub act: String,
    /// Optional IMF path relative to data_root (e.g. "imf/novice_male.imf").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imf: Option<String>,
}

#[derive(Deserialize, Serialize)]
pub struct HeadEntry {
    /// Numeric head sprite ID (the number in the filename, e.g. 1 for "1_male").
    pub id: u32,
    pub gender: String,
    pub spr: String,
    pub act: String,
    /// Body IMF path relative to data_root for head-behind-body z-order computation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imf: Option<String>,
}

#[derive(Deserialize, Serialize)]
pub struct HeadgearEntry {
    /// Sprite name (accname without gender prefix, e.g. "ribbon" for m_ribbon).
    pub name: String,
    /// Accname view ID; informational only, does not affect export.
    pub view: u32,
    /// Equipment slot: "Head_Top" | "Head_Mid" | "Head_Low".
    pub slot: String,
    pub gender: String,
    pub spr: String,
    pub act: String,
}

#[derive(Deserialize, Serialize)]
pub struct GarmentEntry {
    pub name: String,
    pub job: String,
    pub gender: String,
    pub spr: String,
    pub act: String,
}

#[derive(Deserialize, Serialize)]
pub struct WeaponEntry {
    pub name: String,
    pub job: String,
    pub gender: String,
    /// "weapon" for the main weapon sprite, "slash" for the slash effect overlay.
    pub slot: String,
    pub spr: String,
    pub act: String,
}

#[derive(Deserialize, Serialize)]
pub struct ShieldEntry {
    /// Shield sprite name (everything after `{job}_{gender}_` in the filename).
    pub name: String,
    pub job: String,
    pub gender: String,
    pub spr: String,
    pub act: String,
}

#[derive(Deserialize, Serialize)]
pub struct ShadowEntry {
    pub spr: String,
    pub act: String,
}

#[derive(Deserialize, Serialize)]
pub struct ProjectileEntry {
    /// Sprite name (filename stem, e.g. "canon_bullet").
    pub name: String,
    pub spr: String,
    pub act: String,
}

/// Convert a headgear slot string to a numeric slot index for SpriteKind::Headgear.
pub fn parse_headgear_slot(slot: &str) -> Result<u8, String> {
    match slot {
        "Head_Top" => Ok(0),
        "Head_Mid" => Ok(1),
        "Head_Low" => Ok(2),
        other => Err(format!(
            "unknown headgear slot '{other}'; expected Head_Top, Head_Mid, or Head_Low"
        )),
    }
}

/// Convert a weapon slot string to a numeric slot index for SpriteKind::Weapon.
pub fn parse_weapon_slot(slot: &str) -> Result<u8, String> {
    match slot {
        "weapon" => Ok(0),
        "slash" => Ok(1),
        other => Err(format!(
            "unknown weapon slot '{other}'; expected 'weapon' or 'slash'"
        )),
    }
}
