use bevy::prelude::*;
use ro_maps::{GatFile, GndFile, RswLighting, RswObject};

/// Primary Bevy asset for a Ragnarok Online map. Loaded from a `.gnd` file; the loader
/// automatically co-loads the same-named `.gat` and `.rsw` files.
#[derive(Asset, TypePath)]
pub struct RoMapAsset {
    /// Ground mesh data: cubes, surfaces, textures, lightmaps.
    pub gnd: GndFile,
    /// Terrain altitude and type data. Use [`crate::heightmap::height_at`] and
    /// [`crate::navmap::terrain_at`] to query this.
    pub gat: GatFile,
    /// Directional lighting parameters from the RSW scene file.
    pub lighting: RswLighting,
    /// All RSW scene objects (models, lights, audio sources, effects).
    pub objects: Vec<RswObject>,
}
