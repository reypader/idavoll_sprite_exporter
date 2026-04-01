use ro_maps::GatFile;

/// Returns the terrain height at world position `(world_x, world_z)` by bilinearly
/// interpolating the four corner altitudes of the enclosing GAT tile.
///
/// `scale` is the world-units-per-tile value from `GndFile::scale` (always 10.0 in practice).
///
/// RO altitudes use a Y-down convention (deeper = more negative). This function negates them
/// so the result is a positive Bevy Y-up height.
///
/// Returns `0.0` for positions outside the map bounds.
pub fn height_at(gat: &GatFile, scale: f32, world_x: f32, world_z: f32) -> f32 {
    let tile_x = world_x / scale;
    let tile_z = world_z / scale;
    let col = tile_x.floor() as u32;
    let row = tile_z.floor() as u32;

    let Some(tile) = gat.tile(col, row) else {
        return 0.0;
    };

    let fx = tile_x.fract();
    let fz = tile_z.fract();

    let sw = -tile.altitude_sw;
    let se = -tile.altitude_se;
    let nw = -tile.altitude_nw;
    let ne = -tile.altitude_ne;

    let bottom = sw + (se - sw) * fx;
    let top = nw + (ne - nw) * fx;
    bottom + (top - bottom) * fz
}
