mod assets;
mod heightmap;
mod loader;
mod navmap;
mod render;

pub use assets::RoMapAsset;
pub use heightmap::height_at;
pub use loader::RoMapLoader;
pub use navmap::{is_walkable, terrain_at};
pub use render::{RoMapMesh, RoMapRoot};

pub mod prelude {
    pub use crate::{
        RoMapAsset, RoMapLoader, RoMapMesh, RoMapRoot, RoMapsPlugin, height_at, is_walkable,
        terrain_at,
    };
    pub use ro_maps::TerrainType;
}

use bevy::prelude::*;
use bevy_ro_rsm::RsmPlugin;

pub struct RoMapsPlugin;

impl Plugin for RoMapsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(RsmPlugin);
        app.init_asset::<RoMapAsset>();
        app.register_asset_loader(RoMapLoader);
        app.add_systems(Update, (render::spawn_map_meshes, render::spawn_model_meshes));
    }
}
