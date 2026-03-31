mod animation;
#[cfg(feature = "3d")]
pub mod composite;
mod loader;

pub use animation::{
    render_animation, AnimationRepeat, RenderAnimation, RoAnimation, RoAnimationControl,
    RoAnimationPlugin, RoAnimationState,
};
pub use loader::{RoAtlas, RoAtlasLoader, RoAtlasLoaderSettings, TagMeta};

pub mod prelude {
    pub use crate::animation::{
        render_animation, AnimationRepeat, RenderAnimation, RoAnimation, RoAnimationControl,
        RoAnimationPlugin, RoAnimationState,
    };
    #[cfg(feature = "3d")]
    pub use crate::composite::{
        composite_tag, direction_index, orient_billboard, CompositeLayerDef, RoComposite,
        RoCompositeMaterial, RoCompositePlugin, SpriteRole, MAX_LAYERS,
    };
    pub use crate::loader::{RoAtlas, RoAtlasLoader, TagMeta};
    pub use crate::RoSpritePlugin;
}

use bevy::prelude::*;

pub struct RoSpritePlugin;
impl Plugin for RoSpritePlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<RoAtlas>();
        app.register_asset_loader(RoAtlasLoader);
        app.add_plugins(RoAnimationPlugin);
        #[cfg(feature = "3d")]
        app.add_plugins(composite::RoCompositePlugin);
    }
}

/// Human-readable action label. Uses monster labels for ACTs with ≤40 actions (multiples of 8),
/// otherwise falls back to player labels.
pub(crate) fn action_label(idx: usize, total_actions: usize) -> String {
    const PLAYER_BASES: &[(usize, &str)] = &[
        (0, "idle"),
        (8, "walk"),
        (16, "sit"),
        (24, "pickup"),
        (32, "alert"),
        (40, "skill"),
        (48, "flinch"),
        (56, "frozen"),
        (64, "dead"),
        (72, "unknown"),
        (80, "attack1"),
        (88, "attack2"),
        (96, "spell"),
    ];
    const MONSTER_BASES: &[(usize, &str)] = &[
        (0, "idle"),
        (8, "walk"),
        (16, "attack1"),
        (24, "flinch"),
        (32, "dead"),
        (40, "unknown_1"),
        (48, "unknown_2"),
        (56, "unknown_3"),
        (64, "unknown_4"),
        (72, "unknown_5"),
    ];
    const DIRS: &[&str] = &["s", "sw", "w", "nw", "n", "ne", "e", "se"];

    let base = idx - (idx % 8);
    let dir = idx % 8;
    let bases: &[(usize, &str)] = if total_actions != 104 && total_actions.is_multiple_of(8) {
        MONSTER_BASES
    } else {
        PLAYER_BASES
    };

    if let Some(&(_, name)) = bases.iter().find(|&&(b, _)| b == base) {
        format!("{}_{}", name, DIRS[dir])
    } else {
        format!("action_{idx:03}_{}", DIRS[dir])
    }
}
