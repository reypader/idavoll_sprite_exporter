mod assets;
mod loader;

pub use assets::RsmAsset;
pub use loader::RsmLoader;

use bevy::prelude::*;

pub struct RsmPlugin;

impl Plugin for RsmPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<RsmAsset>();
        app.register_asset_loader(RsmLoader);
    }
}
