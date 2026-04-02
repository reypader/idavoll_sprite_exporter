use bevy::{
    asset::{LoadState, RenderAssetUsages},
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use bevy_ro_rsm::RsmAsset;
use ro_maps::{ModelInstance, RswObject};

use crate::assets::RoMapAsset;

/// Vertex data accumulated per texture group while building mesh geometry.
type MeshGroup = (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<[f32; 2]>, Vec<u32>);

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

/// Tracks RSM model instances that are still waiting for their asset to finish loading.
#[derive(Component)]
pub(crate) struct PendingModels {
    pub instances: Vec<(Handle<RsmAsset>, ModelInstance)>,
    /// GND tile scale (always 10.0 in practice), used when computing instance Z translation.
    pub gnd_scale: f32,
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

        // Center the map at the world origin.
        let cx = gnd.width as f32 * scale * 0.5;
        let cz = gnd.height as f32 * scale * 0.5;

        // Group top surfaces by texture_id so we emit one mesh per texture.
        // Each entry: (positions, normals, uvs, indices).
        let texture_count = gnd.texture_paths.len();
        let mut groups: Vec<MeshGroup> =
            (0..texture_count).map(|_| (vec![], vec![], vec![], vec![])).collect();

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
                // Z is negated vs RO: browedit renders row 0 at z=+cz, row N at z=-cz.
                let z0 = cz - row as f32 * scale;
                let z1 = cz - (row + 1) as f32 * scale;

                // Negate heights: RO is Y-down, Bevy is Y-up.
                // After Z-flip: vertex 0 is at (x0, z0=large Z, north edge),
                // vertex 2 is at (x0, z1=small Z, south edge). Heights[0..3] still
                // index the correct spatial corners per the GND binary layout.
                let sw = Vec3::new(x0, -cube.heights[0], z0);
                let se = Vec3::new(x1, -cube.heights[1], z0);
                let nw = Vec3::new(x0, -cube.heights[2], z1);
                let ne = Vec3::new(x1, -cube.heights[3], z1);

                // Face normal: CCW winding from above (+Y) uses sw→se edge × sw→nw edge.
                let edge1 = se - sw;
                let edge2 = nw - sw;
                let normal = edge1.cross(edge2).normalize();
                let normal_arr = normal.to_array();

                let (positions, normals, uvs, indices) = &mut groups[tex_idx];

                let base = positions.len() as u32;

                // Vertices: 0=sw, 1=se, 2=nw, 3=ne
                positions.push(sw.to_array());
                positions.push(se.to_array());
                positions.push(nw.to_array());
                positions.push(ne.to_array());

                for _ in 0..4 {
                    normals.push(normal_arr);
                }

                // UVs: match vertex order above.
                uvs.push([surface.u[0], surface.v[0]]);
                uvs.push([surface.u[1], surface.v[1]]);
                uvs.push([surface.u[2], surface.v[2]]);
                uvs.push([surface.u[3], surface.v[3]]);

                // Two CCW triangles viewed from above (+Y): normal points +Y.
                // sw(0)→se(1)→nw(2)  and  se(1)→ne(3)→nw(2)
                indices.push(base);
                indices.push(base + 1);
                indices.push(base + 2);

                indices.push(base + 1);
                indices.push(base + 3);
                indices.push(base + 2);
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

        // Kick off RSM model instance loading.
        let model_instances: Vec<ModelInstance> = map.objects.iter().filter_map(|obj| {
            if let RswObject::Model(inst) = obj {
                Some(inst.clone())
            } else {
                None
            }
        }).collect();

        if !model_instances.is_empty() {
            let unique_files: std::collections::HashSet<&str> =
                model_instances.iter().map(|inst| inst.model_file.as_str()).collect();
            info!(
                "[RoMap] {} model instance(s) ({} unique file(s)) queued for loading",
                model_instances.len(), unique_files.len()
            );

            let pending: Vec<(Handle<RsmAsset>, ModelInstance)> = model_instances
                .into_iter()
                .map(|inst| {
                    let handle = asset_server.load(inst.model_file.clone());
                    (handle, inst)
                })
                .collect();

            commands.entity(root_entity).insert(PendingModels { instances: pending, gnd_scale: scale });
        }
    }
}

pub(crate) fn spawn_model_meshes(
    mut commands: Commands,
    mut pending_query: Query<(Entity, &mut PendingModels)>,
    rsm_assets: Res<Assets<RsmAsset>>,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (root_entity, mut pending) in &mut pending_query {
        let mut still_pending: Vec<(Handle<RsmAsset>, ModelInstance)> = Vec::new();
        let instances = std::mem::take(&mut pending.instances);

        for (handle, inst) in instances {
            match asset_server.get_load_state(&handle) {
                Some(LoadState::Loaded) => {
                    if let Some(rsm_asset) = rsm_assets.get(&handle) {
                        let children = build_model_children(
                            &inst, rsm_asset, pending.gnd_scale,
                            &mut commands, &mut meshes, &mut materials, &asset_server,
                        );
                        if !children.is_empty() {
                            commands.entity(root_entity).add_children(&children);
                        }
                    }
                }
                Some(LoadState::Failed(err)) => {
                    warn!("[RoModel] failed to load '{}': {err}", inst.model_file);
                }
                _ => {
                    still_pending.push((handle, inst));
                }
            }
        }

        pending.instances = still_pending;

        if pending.instances.is_empty() {
            commands.entity(root_entity).remove::<PendingModels>();
        }
    }
}

fn build_model_children(
    inst: &ModelInstance,
    rsm_asset: &RsmAsset,
    gnd_scale: f32,
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    asset_server: &AssetServer,
) -> Vec<Entity> {
    let rsm = &rsm_asset.rsm;

    // Browedit MapRenderer.cpp:786-787 builds the instance world transform as:
    //   Scale(1,1,-1) * Translate(cx+px, -py, -10-cz+pz) * Rotate * Scale(sx,-sy,sz) * Pivot
    // In centered world space the Z component becomes: 10 - pos.z.
    // Since RSW pos.z is centered (≈ row*scale - cz) and our terrain Z is cz - row*scale,
    // negating pos.z aligns models with corrected terrain. The +10 is within tolerance.
    // Browedit world Z (after its outer Scale(1,1,-1)) = gnd_scale + cz - pos.z.
    // Our Bevy terrain Z = Z_browedit - cz, so Our_Z = gnd_scale - pos.z.
    let translation = Vec3::new(
        inst.pos[0],
        -inst.pos[1],
        gnd_scale - inst.pos[2],
    );
    // With Z negated in mesh geometry (step 7 below), the outer scale(1,1,-1) is baked in.
    // Conjugation by Scale(1,1,-1): Ry(+y)→Ry(-y), Rx(-x)→Rx(+x), Rz(-z)→Rz(-z).
    let rotation = Quat::from_euler(
        EulerRot::YXZ,
        (-inst.rot[1]).to_radians(), // -ry (conjugated by Z-flip)
        inst.rot[0].to_radians(),    // +rx (double-negated: -rx then conjugated)
        (-inst.rot[2]).to_radians(), // -rz (unchanged by Z-flip conjugation)
    );
    let scale = Vec3::new(inst.scale[0], inst.scale[1], inst.scale[2]);

    // Browedit pivot uses realbbmin.y = -bbmax.y (the "bottom" of the model after Y-flip).
    // After our manual Y-negate: -vy_ro. Adding bbmax.y gives bbmax.y - vy_ro, which is exactly
    // what browedit's scale(-sy) * (vy_ro + realbbmin.y) produces. This places the model base
    // at local Y=0 so the instance translation positions the base correctly on terrain.
    let bbrange = rsm.bbrange;

    // Compute the actual minimum Bevy Y across all mesh vertices using the same transform chain
    // applied in the face loop. The simple pre-computed bbmax[1] only uses the offset matrix and
    // misses per-mesh scale and rotation, causing models with non-trivial transforms to float.
    let mut actual_min_y = f32::MAX;
    for mesh in &rsm.meshes {
        let is_root = mesh.parent_name.is_empty();
        for &raw in &mesh.vertices {
            let m = &mesh.offset;
            let mut p = [
                m[0][0] * raw[0] + m[1][0] * raw[1] + m[2][0] * raw[2],
                m[0][1] * raw[0] + m[1][1] * raw[1] + m[2][1] * raw[2],
                m[0][2] * raw[0] + m[1][2] * raw[1] + m[2][2] * raw[2],
            ];
            p[0] += mesh.pos_[0];
            p[1] += mesh.pos_[1];
            p[2] += mesh.pos_[2];
            p[0] *= mesh.scale[0];
            p[1] *= mesh.scale[1];
            p[2] *= mesh.scale[2];
            if !mesh.frames.is_empty() {
                let rot = Quat::from_array(mesh.frames[0].quaternion).normalize();
                p = (rot * Vec3::from(p)).to_array();
            } else if mesh.rot_angle.abs() > 0.001 {
                let axis = Vec3::from(mesh.rot_axis);
                if axis.length_squared() > 0.0001 {
                    let rot = Quat::from_axis_angle(axis.normalize(), mesh.rot_angle);
                    p = (rot * Vec3::from(p)).to_array();
                }
            }
            if !is_root {
                p[0] += mesh.pos[0];
                p[1] += mesh.pos[1];
                p[2] += mesh.pos[2];
            }
            actual_min_y = actual_min_y.min(-p[1]); // Y-negated Bevy Y
        }
    }
    if actual_min_y == f32::MAX {
        actual_min_y = 0.0;
    }

    // Build flat mesh geometry per texture, collecting all face data in model space.
    // Keyed by the resolved RsmFile::textures index.
    let tex_count = rsm.textures.len();
    let mut groups: Vec<MeshGroup> =
        (0..tex_count.max(1)).map(|_| (vec![], vec![], vec![], vec![])).collect();

    for mesh in &rsm.meshes {
        let is_root = mesh.parent_name.is_empty();

        for face in &mesh.faces {
            let tex_slot = face.texture_id as usize;
            let resolved_tex = mesh.texture_indices.get(tex_slot).copied().unwrap_or(0) as usize;
            if resolved_tex >= groups.len() {
                continue;
            }

            let (positions, normals, uvs, indices) = &mut groups[resolved_tex];

            let mut tri_verts = [[0.0f32; 3]; 3];
            let mut tri_uvs = [[0.0f32; 2]; 3];

            for corner in 0..3 {
                let vid = face.vertex_ids[corner] as usize;
                let tcid = face.texcoord_ids[corner] as usize;

                let raw = mesh.vertices.get(vid).copied().unwrap_or([0.0; 3]);

                // 1. Apply 3×3 offset matrix (column-major). Matches browedit matrix2.
                let m = &mesh.offset;
                let mut p = [
                    m[0][0] * raw[0] + m[1][0] * raw[1] + m[2][0] * raw[2],
                    m[0][1] * raw[0] + m[1][1] * raw[1] + m[2][1] * raw[2],
                    m[0][2] * raw[0] + m[1][2] * raw[1] + m[2][2] * raw[2],
                ];

                // 2. Secondary translation (pos_). Applied before scale/rotation in matrix2.
                p[0] += mesh.pos_[0];
                p[1] += mesh.pos_[1];
                p[2] += mesh.pos_[2];

                // 3. Per-mesh scale (browedit matrix1: Scale(scale) applied before rotation).
                p[0] *= mesh.scale[0];
                p[1] *= mesh.scale[1];
                p[2] *= mesh.scale[2];

                // 4. Static rotation or first keyframe quaternion (browedit matrix1).
                if !mesh.frames.is_empty() {
                    let q = mesh.frames[0].quaternion; // [x, y, z, w]
                    let rot = Quat::from_array(q).normalize();
                    p = (rot * Vec3::from(p)).to_array();
                } else if mesh.rot_angle.abs() > 0.001 {
                    let axis = Vec3::from(mesh.rot_axis);
                    if axis.length_squared() > 0.0001 {
                        let rot = Quat::from_axis_angle(axis.normalize(), mesh.rot_angle);
                        p = (rot * Vec3::from(p)).to_array();
                    }
                }

                // 5. Non-root: parent-relative translation (browedit matrix1: Translate(pos)).
                if !is_root {
                    p[0] += mesh.pos[0];
                    p[1] += mesh.pos[1];
                    p[2] += mesh.pos[2];
                }

                // 6. Negate Y (RO Y-down → Bevy Y-up).
                p[1] = -p[1];

                // 7. Negate Z (bakes browedit's outer Scale(1,1,-1) into mesh geometry).
                p[2] = -p[2];

                // 8. Apply bounding box pivot so the model base sits at local Y=0.
                // Use actual_min_y (computed from the full transform chain) instead of the
                // pre-computed bbmax[1] which omits per-mesh scale and rotation.
                p[0] -= bbrange[0];
                p[1] -= actual_min_y;
                p[2] += bbrange[2]; // ADD because Z was negated (browedit: -bbrange.z before Z-flip)

                tri_verts[corner] = p;
                tri_uvs[corner] = mesh.tex_coords.get(tcid).copied().unwrap_or([0.0; 2]);
            }

            let v0 = Vec3::from(tri_verts[0]);
            let v1 = Vec3::from(tri_verts[1]);
            let v2 = Vec3::from(tri_verts[2]);
            let normal = (v1 - v0).cross(v2 - v0).normalize();
            let normal_arr = normal.to_array();

            let base = positions.len() as u32;
            for corner in 0..3 {
                positions.push(tri_verts[corner]);
                normals.push(normal_arr);
                uvs.push(tri_uvs[corner]);
            }
            indices.push(base);
            indices.push(base + 1);
            indices.push(base + 2);

            if face.two_sided {
                let base2 = positions.len() as u32;
                for corner in (0..3).rev() {
                    positions.push(tri_verts[corner]);
                    normals.push((-normal).to_array());
                    uvs.push(tri_uvs[corner]);
                }
                indices.push(base2);
                indices.push(base2 + 1);
                indices.push(base2 + 2);
            }
        }
    }

    let non_empty = groups.iter().filter(|(p, _, _, _)| !p.is_empty()).count();
    let total_verts: usize = groups.iter().map(|(p, _, _, _)| p.len()).sum();
    info!(
        "[RoModel] spawning '{}' — {} mesh(es), {} tex group(s), {} total verts, bb {:?}..{:?}",
        inst.model_file, rsm.meshes.len(), non_empty, total_verts, rsm.bbmin, rsm.bbmax
    );

    let instance_root = commands
        .spawn(Transform::from_translation(translation)
            .with_rotation(rotation)
            .with_scale(scale))
        .id();

    let children: Vec<Entity> = vec![instance_root];

    for (tex_idx, (positions, normals, uvs_data, face_indices)) in groups.into_iter().enumerate() {
        if positions.is_empty() {
            continue;
        }

        let vert_count = positions.len();
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs_data);
        mesh.insert_indices(Indices::U32(face_indices));

        let tex_name = rsm.textures.get(tex_idx).map(|s| s.as_str()).unwrap_or("");
        info!(
            "[RoModel]   tex group {} — {} verts, texture: {}",
            tex_idx, vert_count, tex_name
        );

        let texture: Handle<Image> = asset_server.load(tex_name.to_string());
        let material = materials.add(StandardMaterial {
            base_color_texture: Some(texture),
            double_sided: false,
            cull_mode: None,
            ..default()
        });

        let mesh_entity = commands
            .spawn((
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(material),
                Transform::default(),
            ))
            .id();

        commands.entity(instance_root).add_child(mesh_entity);
    }

    children
}
