use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};

use crate::assets::RoMapAsset;

/// Marker component placed on each terrain mesh entity spawned by the plugin.
#[derive(Component)]
pub struct RoMapMesh;

/// Place this component on a root entity to have the plugin spawn terrain mesh children once
/// the referenced [`RoMapAsset`] has loaded.
///
/// ```rust,no_run
/// # use bevy::prelude::*;
/// # use bevy_ro_maps::render::RoMapRoot;
/// # fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
/// commands.spawn((
///     RoMapRoot { asset: asset_server.load("maps/prontera.gnd"), spawned: false },
///     Transform::default(),
///     Visibility::default(),
/// ));
/// # }
/// ```
#[derive(Component)]
pub struct RoMapRoot {
    pub asset: Handle<RoMapAsset>,
    /// Set to `true` by the plugin once mesh children have been spawned. Prevents re-spawning
    /// on subsequent frames.
    pub spawned: bool,
}

pub(crate) fn spawn_map_meshes(
    mut commands: Commands,
    mut map_roots: Query<(Entity, &mut RoMapRoot)>,
    map_assets: Res<Assets<RoMapAsset>>,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (root_entity, mut root) in &mut map_roots {
        if root.spawned {
            continue;
        }
        let load_state = asset_server.get_load_state(&root.asset);
        let Some(map) = map_assets.get(&root.asset) else {
            info!("[RoMap] asset not ready yet, load state: {:?}", load_state);
            continue;
        };
        info!("[RoMap] asset loaded — grid {}x{}, scale {}, {} textures, {} surfaces, {} cubes",
            map.gnd.width, map.gnd.height, map.gnd.scale,
            map.gnd.texture_paths.len(), map.gnd.surfaces.len(), map.gnd.cubes.len());
        root.spawned = true;

        let gnd = &map.gnd;
        let scale = gnd.scale;

        // Group top surfaces by texture_id so we emit one mesh per texture.
        // Each entry: (positions, normals, uvs, indices).
        let texture_count = gnd.texture_paths.len();
        let mut groups: Vec<(Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<[f32; 2]>, Vec<u32>)> =
            (0..texture_count).map(|_| (vec![], vec![], vec![], vec![])).collect();

        // Center the map at the world origin.
        let cx = gnd.width as f32 * scale * 0.5;
        let cz = gnd.height as f32 * scale * 0.5;

        for row in 0..gnd.height {
            for col in 0..gnd.width {
                let cube = &gnd.cubes[(row * gnd.width + col) as usize];

                if cube.top_surface_id < 0 {
                    continue;
                }
                let surface = &gnd.surfaces[cube.top_surface_id as usize];
                if surface.texture_id < 0 {
                    continue;
                }
                let tex_idx = surface.texture_id as usize;
                if tex_idx >= texture_count {
                    continue;
                }

                let x0 = col as f32 * scale - cx;
                let x1 = (col + 1) as f32 * scale - cx;
                let z0 = row as f32 * scale - cz;
                let z1 = (row + 1) as f32 * scale - cz;

                // Negate heights: RO is Y-down, Bevy is Y-up.
                let sw = Vec3::new(x0, -cube.heights[0], z0);
                let se = Vec3::new(x1, -cube.heights[1], z0);
                let nw = Vec3::new(x0, -cube.heights[2], z1);
                let ne = Vec3::new(x1, -cube.heights[3], z1);

                // Face normal: CCW winding from +Y means edge order NW-SW x NE-SW gives +Y normal.
                let edge1 = nw - sw;
                let edge2 = ne - sw;
                let normal = edge1.cross(edge2).normalize();
                let normal_arr = normal.to_array();

                let (positions, normals, uvs, indices) = &mut groups[tex_idx];

                let base = positions.len() as u32;

                // Vertices: SW, SE, NW, NE
                positions.push(sw.to_array());
                positions.push(se.to_array());
                positions.push(nw.to_array());
                positions.push(ne.to_array());

                for _ in 0..4 {
                    normals.push(normal_arr);
                }

                // UVs: surface stores [SW, SE, NW, NE]
                uvs.push([surface.u[0], surface.v[0]]);
                uvs.push([surface.u[1], surface.v[1]]);
                uvs.push([surface.u[2], surface.v[2]]);
                uvs.push([surface.u[3], surface.v[3]]);

                // Two triangles, CCW winding viewed from above (+Y):
                // SW(0), NW(2), NE(3)  and  SW(0), NE(3), SE(1)
                indices.push(base);
                indices.push(base + 2);
                indices.push(base + 3);

                indices.push(base);
                indices.push(base + 3);
                indices.push(base + 1);
            }
        }

        let total_verts: usize = groups.iter().map(|(p, _, _, _)| p.len()).sum();
        let non_empty = groups.iter().filter(|(p, _, _, _)| !p.is_empty()).count();
        let all_positions: Vec<[f32; 3]> = groups.iter().flat_map(|(p, _, _, _)| p.iter().copied()).collect();
        if !all_positions.is_empty() {
            let min = all_positions.iter().fold([f32::MAX; 3], |acc, p| [acc[0].min(p[0]), acc[1].min(p[1]), acc[2].min(p[2])]);
            let max = all_positions.iter().fold([f32::MIN; 3], |acc, p| [acc[0].max(p[0]), acc[1].max(p[1]), acc[2].max(p[2])]);
            info!("[RoMap] mesh AABB  min {:?}  max {:?}", min, max);
        }
        info!("[RoMap] built {} non-empty mesh groups, {} total vertices", non_empty, total_verts);

        // Spawn child mesh entities
        let mut children: Vec<Entity> = Vec::new();
        for (tex_idx, (positions, normals, uvs, indices)) in groups.into_iter().enumerate() {
            if positions.is_empty() {
                continue;
            }

            let vert_count = positions.len();
            let mut mesh = Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::default(),
            );
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
            mesh.insert_indices(Indices::U32(indices));

            let texture_path = &gnd.texture_paths[tex_idx];
            info!("[RoMap] spawning mesh group {} — {} verts, texture: {}", tex_idx, vert_count, texture_path);

            let texture: Handle<Image> = asset_server.load(texture_path);

            let material = materials.add(StandardMaterial {
                base_color_texture: Some(texture),
                ..default()
            });

            let child = commands
                .spawn((
                    Mesh3d(meshes.add(mesh)),
                    MeshMaterial3d(material),
                    Transform::default(),
                    RoMapMesh,
                ))
                .id();
            children.push(child);
        }

        if !children.is_empty() {
            commands.entity(root_entity).add_children(&children);
        }
    }
}
