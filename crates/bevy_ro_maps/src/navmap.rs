use ro_maps::{GatFile, TerrainType};

/// Returns the [`TerrainType`] of the GAT tile at world position `(world_x, world_z)`.
///
/// `scale` is the world-units-per-tile value from `GndFile::scale` (always 10.0 in practice).
///
/// Returns [`TerrainType::Blocked`] for positions outside the map bounds.
pub fn terrain_at(gat: &GatFile, scale: f32, world_x: f32, world_z: f32) -> TerrainType {
    let col = (world_x / scale).floor() as u32;
    let row = (world_z / scale).floor() as u32;
    gat.tile(col, row)
        .map(|t| t.terrain_type)
        .unwrap_or(TerrainType::Blocked)
}

/// Returns `true` if the GAT tile at world position `(world_x, world_z)` is walkable.
///
/// `scale` is the world-units-per-tile value from `GndFile::scale` (always 10.0 in practice).
///
/// Returns `false` for positions outside the map bounds.
pub fn is_walkable(gat: &GatFile, scale: f32, world_x: f32, world_z: f32) -> bool {
    terrain_at(gat, scale, world_x, world_z) == TerrainType::Walkable
}
