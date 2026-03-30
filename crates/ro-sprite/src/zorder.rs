use crate::imf::ImfFile;

/// The sprite type, used to determine compositing z-order.
///
/// When multiple sprite types are composited together (body + head + headgear + garment +
/// weapon), the draw order depends on both the sprite type and the current direction.
#[derive(Debug, Clone)]
pub enum SpriteKind {
    Shadow,
    /// Garment (robe). The z-order value (35, above all other layers) is a recommended
    /// default that keeps garments visible. The actual value depends on the specific garment
    /// item, job, action, and frame via runtime Lua tables (`_New_DrawOnTop`, `IsTopLayer`).
    /// The runtime should override this value when those tables are available.
    Garment,
    Shield,
    Body,
    Head,
    /// `slot` 0–3: upper, middle, lower, extra headgear layer
    Headgear {
        slot: u8,
    },
    /// `slot` 0 = weapon, 1 = weapon slash effect
    Weapon {
        slot: u8,
    },
}

impl SpriteKind {
    /// Parse from CLI string. Returns an error message on unknown input.
    pub fn for_str(s: &str) -> Result<Self, String> {
        match s {
            "shadow" => Ok(Self::Shadow),
            "garment" => Ok(Self::Garment),
            "shield" => Ok(Self::Shield),
            "body" => Ok(Self::Body),
            "head" => Ok(Self::Head),
            "headgear" => Ok(Self::Headgear { slot: 0 }),
            "weapon" => Ok(Self::Weapon { slot: 0 }),
            "weapon-slash" => Ok(Self::Weapon { slot: 1 }),
            other => Err(format!(
                "unknown sprite kind '{other}'; expected one of: shadow, garment, \
                 shield, body, head, headgear, weapon, weapon-slash"
            )),
        }
    }
}

/// Compute the z-order value for a single frame of a sprite.
///
/// Higher values render on top. `action_idx` is the absolute action index (0-based) in
/// the ACT file; direction = action_idx % 8. `frame_idx` is the frame index within that
/// action. `imf` is the body's IMF file and is only consulted for `Head` sprites.
///
/// For `Garment`, the returned value (35) is a conservative "always on top" default. The
/// correct value for a specific garment item may differ per action/frame based on runtime
/// Lua tables and should be overridden by the consumer when that context is available.
pub fn z_order(
    kind: &SpriteKind,
    action_idx: usize,
    frame_idx: usize,
    imf: Option<&ImfFile>,
) -> i32 {
    let top_left = matches!(action_idx % 8, 2..=5);

    match kind {
        SpriteKind::Shadow => -1,
        SpriteKind::Garment => 35,
        SpriteKind::Shield => {
            if top_left {
                10
            } else {
                30
            }
        }
        SpriteKind::Body => {
            if top_left {
                15
            } else {
                10
            }
        }
        SpriteKind::Head => {
            let behind = imf
                .and_then(|f| f.priority(1, action_idx, frame_idx))
                .map(|p| p == 1)
                .unwrap_or(false);
            match (top_left, behind) {
                (true, false) => 20,
                (true, true) => 14,
                (false, false) => 15,
                (false, true) => 9,
            }
        }
        SpriteKind::Headgear { slot } => {
            if top_left {
                25 - (3 - *slot as i32)
            } else {
                20 - (3 - *slot as i32)
            }
        }
        SpriteKind::Weapon { slot } => {
            if top_left {
                30 - (2 - *slot as i32)
            } else {
                25 - (2 - *slot as i32)
            }
        }
    }
}
