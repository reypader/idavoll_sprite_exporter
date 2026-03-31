use std::{num::NonZero, time::Duration};

use bevy::{
    asset::uuid_handle,
    ecs::system::{lifetimeless::SRes, SystemParamItem},
    pbr::Material,
    prelude::*,
    render::{
        render_asset::RenderAssets,
        render_resource::{
            binding_types::{sampler, storage_buffer_read_only_sized, texture_2d},
            AsBindGroup, AsBindGroupError, BindGroupEntry, BindGroupLayout,
            BindGroupLayoutDescriptor, BindGroupLayoutEntries, BindGroupLayoutEntry,
            BindingResource, BindingResources, BufferInitDescriptor, BufferUsages, PipelineCache,
            PreparedBindGroup, SamplerBindingType, ShaderStages, TextureSampleType,
            UnpreparedBindGroup,
        },
        renderer::RenderDevice,
        texture::{FallbackImage, GpuImage},
    },
    shader::ShaderRef,
};

use crate::{animation::SpriteFrameEvent, loader::RoAtlas};

// ─────────────────────────────────────────────────────────────
// Constants
// ─────────────────────────────────────────────────────────────

/// Maximum number of composited sprite layers (shadow, garment, body, head, headgear×3, weapon).
pub const MAX_LAYERS: usize = 8;

pub const COMPOSITE_SHADER_HANDLE: Handle<Shader> =
    uuid_handle!("726f5f63-6f6d-706f-7369-746500000000");

// ─────────────────────────────────────────────────────────────
// GPU data layout  (must match ro_composite.wgsl)
// ─────────────────────────────────────────────────────────────

/// Per-layer data in the uniform buffer.
#[repr(C)]
#[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LayerUniform {
    pub atlas_uv_min: [f32; 2],
    pub atlas_uv_max: [f32; 2],
    pub canvas_offset: [f32; 2],
    pub layer_size: [f32; 2],
}

/// Full composite uniform buffer. Must match the WGSL `CompositeData` struct.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CompositeUniform {
    canvas_size: [f32; 2],
    layer_count: u32,
    _pad: u32,
    layers: [LayerUniform; MAX_LAYERS],
}

// ─────────────────────────────────────────────────────────────
// Material asset
// ─────────────────────────────────────────────────────────────

/// GPU material holding one atlas texture per layer plus uniform data.
/// Updated by `update_ro_composite` each frame.
#[derive(Asset, TypePath, Clone)]
pub struct RoCompositeMaterial {
    /// Atlas image handle per layer slot.  Unused slots hold the default handle.
    pub textures: [Handle<Image>; MAX_LAYERS],
    pub canvas_size: Vec2,
    pub layer_count: u32,
    pub layers: [LayerUniform; MAX_LAYERS],
}

impl Default for RoCompositeMaterial {
    fn default() -> Self {
        Self {
            textures: std::array::from_fn(|_| Handle::default()),
            canvas_size: Vec2::ONE,
            layer_count: 0,
            layers: [LayerUniform::default(); MAX_LAYERS],
        }
    }
}

impl AsBindGroup for RoCompositeMaterial {
    type Data = ();
    type Param = (SRes<RenderAssets<GpuImage>>, SRes<FallbackImage>);

    fn as_bind_group(
        &self,
        layout_descriptor: &BindGroupLayoutDescriptor,
        render_device: &RenderDevice,
        pipeline_cache: &PipelineCache,
        (image_assets, fallback_image): &mut SystemParamItem<Self::Param>,
    ) -> Result<PreparedBindGroup, AsBindGroupError> {
        let layout = pipeline_cache.get_bind_group_layout(layout_descriptor);
        // Build a &[&wgpu::TextureView] by double-dereffing Bevy's TextureView wrapper.
        let fallback = &fallback_image.d2.texture_view;
        let mut views: Vec<_> = vec![&**fallback; MAX_LAYERS];
        for (i, handle) in self
            .textures
            .iter()
            .enumerate()
            .take(self.layer_count as usize)
        {
            match image_assets.get(handle) {
                Some(img) => views[i] = &*img.texture_view,
                None => return Err(AsBindGroupError::RetryNextUpdate),
            }
        }

        let uniform = CompositeUniform {
            canvas_size: self.canvas_size.into(),
            layer_count: self.layer_count,
            _pad: 0,
            layers: self.layers,
        };
        let uniform_buf = render_device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("ro_composite_uniform"),
            contents: bytemuck::bytes_of(&uniform),
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
        });

        let bind_group = render_device.create_bind_group(
            Self::label(),
            &layout,
            &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureViewArray(views.as_slice()),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Sampler(&fallback_image.d2.sampler),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: uniform_buf.as_entire_binding(),
                },
            ],
        );

        Ok(PreparedBindGroup {
            bindings: BindingResources(vec![]),
            bind_group,
        })
    }

    fn unprepared_bind_group(
        &self,
        _layout: &BindGroupLayout,
        _render_device: &RenderDevice,
        _param: &mut SystemParamItem<Self::Param>,
        _force_no_bindless: bool,
    ) -> Result<UnpreparedBindGroup, AsBindGroupError> {
        Err(AsBindGroupError::CreateBindGroupDirectly)
    }

    fn bind_group_layout_entries(
        _render_device: &RenderDevice,
        _force_no_bindless: bool,
    ) -> Vec<BindGroupLayoutEntry>
    where
        Self: Sized,
    {
        BindGroupLayoutEntries::with_indices(
            ShaderStages::FRAGMENT,
            (
                (
                    0,
                    texture_2d(TextureSampleType::Float { filterable: true })
                        .count(NonZero::new(MAX_LAYERS as u32).unwrap()),
                ),
                (1, sampler(SamplerBindingType::Filtering)),
                (
                    2,
                    storage_buffer_read_only_sized(
                        false,
                        NonZero::new(std::mem::size_of::<CompositeUniform>() as u64),
                    ),
                ),
            ),
        )
        .to_vec()
    }

    fn bind_group_data(&self) -> Self::Data {}

    fn label() -> &'static str {
        "ro_composite_material"
    }
}

impl Material for RoCompositeMaterial {
    fn fragment_shader() -> ShaderRef {
        COMPOSITE_SHADER_HANDLE.into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::AlphaToCoverage
    }
}

// ─────────────────────────────────────────────────────────────
// RoComposite component
// ─────────────────────────────────────────────────────────────

/// The role of a layer in the composite. Determines z-order based on direction and IMF data,
/// matching the draw-order table from zrenderer (`source/sprite.d`).
///
/// Direction groups:
/// - **topLeft**: W, NW, N, NE (direction indices 2–5)
/// - **bottomRight**: S, SW, E, SE (direction indices 0–1, 6–7)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpriteRole {
    Shadow,
    Body,
    Head,
    /// `slot` 0–3 maps to upper/middle/lower/extra headgear slots.
    Headgear { slot: u8 },
    /// `slot` 0 = main weapon, `slot` 1 = slash/glow overlay.
    Weapon { slot: u8 },
    Shield,
    /// Garment z-order is per-item/action/frame in Lua tables; 35 is the always-on-top fallback.
    Garment,
}

impl SpriteRole {
    /// Returns the z-order for this role given the current direction and IMF head-behind flag.
    /// Lower z = drawn first (behind). Values match the zrenderer reference table.
    pub fn z_order(self, top_left: bool, head_behind: bool) -> i32 {
        match self {
            SpriteRole::Shadow => -1,
            SpriteRole::Body => if top_left { 15 } else { 10 },
            SpriteRole::Head => match (top_left, head_behind) {
                (true,  true)  => 14,
                (true,  false) => 20,
                (false, true)  =>  9,
                (false, false) => 15,
            },
            SpriteRole::Headgear { slot } => {
                let base = if top_left { 22 } else { 17 };
                base + slot as i32
            }
            SpriteRole::Weapon { slot } => {
                let base = if top_left { 28 } else { 23 };
                base + slot as i32
            }
            SpriteRole::Shield => if top_left { 10 } else { 30 },
            SpriteRole::Garment => 35,
        }
    }
}

/// Describes one layer in a composite sprite (body, head, headgear, …).
pub struct CompositeLayerDef {
    pub atlas: Handle<RoAtlas>,
    /// The semantic role of this layer. Drives z-order based on camera direction and IMF data.
    pub role: SpriteRole,
}

/// Drive a single-quad composite billboard from multiple RoAtlas layers.
///
/// Attach alongside `Mesh3d`, `MeshMaterial3d<RoCompositeMaterial>`, and `Transform`.
#[derive(Component)]
pub struct RoComposite {
    pub layers: Vec<CompositeLayerDef>,
    pub tag: Option<String>,
    pub playing: bool,
    pub speed: f32,
    pub current_frame: u16,
    pub elapsed: Duration,
}

impl Default for RoComposite {
    fn default() -> Self {
        Self {
            layers: Vec::new(),
            tag: None,
            playing: true,
            speed: 1.0,
            current_frame: 0,
            elapsed: Duration::ZERO,
        }
    }
}

// ─────────────────────────────────────────────────────────────
// Plugin + systems
// ─────────────────────────────────────────────────────────────

pub struct RoCompositePlugin;

impl Plugin for RoCompositePlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::load_internal_asset!(
            app,
            COMPOSITE_SHADER_HANDLE,
            "shaders/ro_composite.wgsl",
            Shader::from_wgsl
        );
        app.add_plugins(MaterialPlugin::<RoCompositeMaterial>::default());
        app.add_systems(Update, (orient_billboard, update_ro_composite).chain());
    }
}

/// Advances animation and rebuilds the `RoCompositeMaterial` uniforms each frame.
pub fn update_ro_composite(
    mut composites: Query<(
        Entity,
        &mut RoComposite,
        &MeshMaterial3d<RoCompositeMaterial>,
        &mut Transform,
    )>,
    atlases: Res<Assets<RoAtlas>>,
    layouts: Res<Assets<TextureAtlasLayout>>,
    mut mats: ResMut<Assets<RoCompositeMaterial>>,
    time: Res<Time>,
    camera_q: Query<&GlobalTransform, With<Camera3d>>,
    mut commands: Commands,
) {
    // Camera right/up in world space: the billboard's local axes after look_at.
    // Used to convert canvas-pixel offsets to world-space translation.
    let (cam_right, cam_up) = if let Ok(cam_gt) = camera_q.single() {
        (cam_gt.right(), cam_gt.up())
    } else {
        (Dir3::X, Dir3::Y)
    };
    for (entity, mut composite, mat_handle, mut transform) in &mut composites {
        // ── 1. Advance animation ──────────────────────────────────────────
        // Resolve tag range and frame duration from the first layer's atlas.
        // We extract owned values immediately to avoid holding borrows across
        // the subsequent mutations of `composite`.
        let first_handle = composite.layers.first().map(|l| l.atlas.clone());
        let Some(first_atlas) = first_handle.as_ref().and_then(|h| atlases.get(h)) else {
            continue;
        };

        let tag_range = match composite.tag.as_ref() {
            Some(tag) => match first_atlas.tags.get(tag) {
                Some(meta) => meta.range.clone(),
                None => continue,
            },
            None => 0..=(first_atlas.frame_durations.len().saturating_sub(1) as u16),
        };
        let frame_dur = first_atlas
            .frame_durations
            .get(composite.current_frame as usize)
            .copied();
        let _ = first_atlas; // release borrow on atlases before mutating composite

        // The Body layer is the compositing anchor and the source of ACT frame events.
        // Compute this early so it's available both during frame advance and in step 2.
        let body_idx = composite
            .layers
            .iter()
            .position(|l| l.role == SpriteRole::Body)
            .unwrap_or(0);

        if !tag_range.contains(&composite.current_frame) {
            composite.current_frame = *tag_range.start();
            composite.elapsed = Duration::ZERO;
        }

        if composite.playing {
            let speed = composite.speed.max(0.0);
            composite.elapsed += Duration::from_secs_f32(time.delta_secs() * speed);
            if let Some(dur) = frame_dur {
                if composite.elapsed >= dur {
                    composite.elapsed = Duration::ZERO;
                    let next = composite.current_frame + 1;
                    let new_frame = if next > *tag_range.end() {
                        *tag_range.start()
                    } else {
                        next
                    };
                    composite.current_frame = new_frame;

                    // Emit ACT frame events from the body atlas (the animation driver).
                    let body_handle = composite.layers.get(body_idx).map(|l| l.atlas.clone());
                    if let Some(atlas) = body_handle.as_ref().and_then(|h| atlases.get(h)) {
                        if let Some(Some(event)) = atlas.frame_events.get(new_frame as usize) {
                            commands.trigger(SpriteFrameEvent {
                                entity,
                                event: event.clone(),
                                tag: composite.tag.clone(),
                            });
                        }
                    }
                }
            }
        }

        let frame = composite.current_frame as usize;

        // ── 2. Collect per-layer frame data ───────────────────────────────
        struct FrameInfo {
            image: Handle<Image>,
            uv_min: Vec2,
            uv_max: Vec2,
            size_px: Vec2,
            origin: IVec2,
            /// Attach-point displacement in feet-origin pixel space.
            /// = anchor_attach − self_attach; zero for anchor layer and for layers
            /// whose attach points match the anchor's (weapons, garments, headgear).
            attach_offset: Vec2,
            /// Computed z-order for this frame: role + direction + IMF head-behind flag.
            z_order: i32,
        }

        // Direction from the current tag suffix ("idle_nw" → top_left = true).
        // Drives the z-order table: topLeft (W/NW/N/NE) vs bottomRight (S/SW/E/SE).
        let is_top_left = composite.tag.as_deref().map(tag_is_top_left).unwrap_or(false);

        // Anchor attach point and IMF head-behind flag both come from the body atlas.
        let body_atlas = atlases.get(&composite.layers[body_idx].atlas);
        let anchor_attach: Option<Vec2> = body_atlas
            .and_then(|a| a.frame_attach_points.get(frame).copied().flatten())
            .map(|ap| ap.as_vec2());
        let head_behind = body_atlas
            .and_then(|a| a.frame_head_behind.get(frame).copied())
            .unwrap_or(false);

        // Iterate body first (so frames[0] is always the anchor), then all other layers.
        let layer_order: Vec<usize> = std::iter::once(body_idx)
            .chain((0..composite.layers.len()).filter(|&i| i != body_idx))
            .collect();

        // `current_frame` is a flat index into the body atlas's frame sequence.
        // Non-body layers (weapon, head, etc.) may have different frame counts per action,
        // so their flat sequences diverge from the body's. We remap by computing the
        // relative position within the body's tag, then applying it to each layer's own
        // tag range for the same tag name.
        let tag_name = composite.tag.clone();
        let rel_frame = (frame as u16).saturating_sub(*tag_range.start());

        let mut frames: Vec<FrameInfo> = Vec::with_capacity(composite.layers.len());
        let mut all_ready = true;
        for &layer_idx in &layer_order {
            let layer_def = &composite.layers[layer_idx];
            let Some(atlas) = atlases.get(&layer_def.atlas) else {
                all_ready = false;
                break;
            };
            let Some(layout) = layouts.get(&atlas.atlas_layout) else {
                all_ready = false;
                break;
            };

            // Map relative position within body's tag to this layer's own tag range.
            let mapped_frame = tag_name
                .as_deref()
                .and_then(|t| atlas.tags.get(t))
                .map(|m| {
                    (*m.range.start() + rel_frame).min(*m.range.end()) as usize
                })
                .unwrap_or(frame);

            let atlas_idx = atlas.get_atlas_index(mapped_frame);
            let rect = layout.textures[atlas_idx];
            let atlas_size = layout.size.as_vec2();

            let uv_min = rect.min.as_vec2() / atlas_size;
            let uv_max = rect.max.as_vec2() / atlas_size;
            let size_px = (rect.max - rect.min).as_vec2();
            let origin = atlas
                .frame_origins
                .get(mapped_frame)
                .copied()
                .unwrap_or(IVec2::ZERO);

            let self_attach = atlas
                .frame_attach_points
                .get(mapped_frame)
                .copied()
                .flatten()
                .map(|ap| ap.as_vec2());
            let attach_offset = match (anchor_attach, self_attach) {
                (Some(a), Some(s)) => a - s,
                _ => Vec2::ZERO,
            };

            frames.push(FrameInfo {
                image: atlas.atlas_image.clone(),
                uv_min,
                uv_max,
                size_px,
                origin,
                attach_offset,
                z_order: layer_def.role.z_order(is_top_left, head_behind),
            });
        }
        if !all_ready || frames.is_empty() {
            continue;
        }

        // ── 3. Compute canvas bounds anchored to the body (first/anchor) layer ──
        // The body's tight frame is placed at canvas (0, 0). Its feet are at
        // body.origin within that frame, so canvas_feet = body.origin (stable).
        // Other layers can extend the canvas in any direction; if they extend
        // left or above the body's top-left, we shift the canvas right/down by
        // the overflow so all content remains in positive canvas coordinates.
        let body = &frames[0];
        let mut content_min = Vec2::ZERO; // body top-left at canvas (0, 0)
        let mut content_max = body.size_px; // body bottom-right
        for fi in frames.iter().skip(1) {
            let lo = body.origin.as_vec2() + fi.attach_offset - fi.origin.as_vec2();
            let hi = lo + fi.size_px;
            content_min = content_min.min(lo);
            content_max = content_max.max(hi);
        }
        // Shift canvas right/down if any layer extends above/left of body origin.
        let overflow = (-content_min).max(Vec2::ZERO);
        let canvas_size = (content_max + overflow).max(Vec2::ONE);
        // canvas_feet = body's feet in canvas pixel space.
        // Stable as long as nothing extends above/left of the body's top-left (rare for weapons).
        let canvas_feet = body.origin.as_vec2() + overflow;

        // ── 4. Build layer uniforms (sorted by z-order) ───────────────────
        // Canvas bounds (step 3) always use frames[0] (body) as the anchor.
        // GPU draw order is determined by each frame's computed z_order:
        // lower z = submitted first = drawn behind.
        let mut render_order: Vec<usize> = (0..frames.len()).collect();
        render_order.sort_by_key(|&i| frames[i].z_order);

        let mut textures: [Handle<Image>; MAX_LAYERS] = std::array::from_fn(|_| Handle::default());
        let mut layer_uniforms = [LayerUniform::default(); MAX_LAYERS];
        let count = frames.len().min(MAX_LAYERS) as u32;

        for (slot, &fi_idx) in render_order.iter().enumerate().take(MAX_LAYERS) {
            let fi = &frames[fi_idx];
            textures[slot] = fi.image.clone();
            // top-left of this layer's frame in canvas pixels
            let offset = canvas_feet + fi.attach_offset - fi.origin.as_vec2();
            layer_uniforms[slot] = LayerUniform {
                atlas_uv_min: fi.uv_min.into(),
                atlas_uv_max: fi.uv_max.into(),
                canvas_offset: offset.into(),
                layer_size: fi.size_px.into(),
            };
        }

        // ── 5. Update material ────────────────────────────────────────────
        if let Some(mat) = mats.get_mut(&mat_handle.0) {
            mat.textures = textures;
            mat.canvas_size = canvas_size;
            mat.layer_count = count;
            mat.layers = layer_uniforms;
        }

        // ── 6. Size and position the billboard quad ───────────────────────
        // Scale so 1 unit = 1 pixel.
        transform.scale = Vec3::new(canvas_size.x, canvas_size.y, 1.0);

        // The canvas feet pixel is at local offset (local_x, local_y) from the
        // billboard center (in y-up scaled space):
        //   local_x = canvas_feet.x - canvas_size.x / 2  (rightward in canvas)
        //   local_y = canvas_size.y / 2 - canvas_feet.y  (upward; feet below center → negative)
        //
        // The billboard's local axes are the camera's right/up in world space.
        // To place the feet pixel at the actor's world position (parent origin),
        // the billboard center must be offset by -R*(local_x, local_y, 0):
        //   translation = -local_x * cam_right - local_y * cam_up
        let local_x = canvas_feet.x - canvas_size.x / 2.0;
        let local_y = canvas_size.y / 2.0 - canvas_feet.y;
        transform.translation = -*cam_right * local_x - *cam_up * local_y;
    }
}

/// Returns `true` if the direction suffix in `tag` is topLeft (W, NW, N, NE).
/// Used to select the correct z-order column from the zrenderer reference table.
fn tag_is_top_left(tag: &str) -> bool {
    matches!(tag.rsplit('_').next(), Some("w" | "nw" | "n" | "ne"))
}

/// Builds the action tag string for use with [`RoComposite::tag`].
///
/// `action` is the animation name (e.g. `"idle"`, `"walk"`) and `dir` is the
/// 0-7 direction index from [`direction_index`].
pub fn composite_tag(action: &str, dir: u8) -> String {
    const DIRS: &[&str] = &["s", "sw", "w", "nw", "n", "ne", "e", "se"];
    format!("{}_{}", action, DIRS[dir as usize % 8])
}

/// Converts a world-space facing direction + camera forward to a 0–7 direction index.
///
/// Index 0 = south (toward camera), clockwise: `0=s 1=sw 2=w 3=nw 4=n 5=ne 6=e 7=se`.
///
/// `facing` is the actor's facing direction in the XZ world plane (Y component ignored).
/// `cam_fwd` is the camera's forward direction projected onto XZ (from `Transform::forward`).
pub fn direction_index(facing: Vec2, cam_fwd: Vec2) -> u8 {
    let facing = facing.normalize_or(Vec2::Y);
    let cam_right = Vec2::new(-cam_fwd.y, cam_fwd.x);
    let screen_x = facing.dot(cam_right);
    let screen_y = facing.dot(-cam_fwd);
    let angle = screen_y.atan2(screen_x);
    let angle = if angle < 0.0 {
        angle + std::f32::consts::TAU
    } else {
        angle
    };
    ((angle + std::f32::consts::PI / 8.0) / (std::f32::consts::TAU / 8.0)) as u8 % 8
}

/// Keeps every [`RoComposite`] billboard facing the camera.
///
/// Rotates the billboard about its parent's world position (actor feet) so that the
/// canvas always faces the camera regardless of the billboard's canvas-layout translation.
pub fn orient_billboard(
    mut billboards: Query<(&mut Transform, &ChildOf), (With<RoComposite>, Without<Camera3d>)>,
    parents: Query<&GlobalTransform>,
    camera_q: Query<&Transform, (With<Camera3d>, Without<RoComposite>)>,
) {
    let Ok(cam) = camera_q.single() else { return };
    for (mut tf, child_of) in &mut billboards {
        let pivot = parents
            .get(child_of.parent())
            .map(|gt| gt.translation())
            .unwrap_or(Vec3::ZERO);
        let dir_to_cam = (cam.translation - pivot).normalize_or_zero();
        let target = tf.translation - dir_to_cam;
        tf.look_at(target, cam.up());
    }
}
