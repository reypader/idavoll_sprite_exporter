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
            BindingResource, BindingResources, BufferInitDescriptor, BufferUsages,
            PipelineCache, PreparedBindGroup, SamplerBindingType, ShaderStages,
            TextureSampleType, UnpreparedBindGroup,
        },
        renderer::RenderDevice,
        texture::{FallbackImage, GpuImage},
    },
    shader::ShaderRef,
};

use crate::loader::RoAtlas;

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
    pub atlas_uv_min:  [f32; 2],
    pub atlas_uv_max:  [f32; 2],
    pub canvas_offset: [f32; 2],
    pub layer_size:    [f32; 2],
}

/// Full composite uniform buffer. Must match the WGSL `CompositeData` struct.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CompositeUniform {
    canvas_size:  [f32; 2],
    layer_count:  u32,
    _pad:         u32,
    layers:       [LayerUniform; MAX_LAYERS],
}

// ─────────────────────────────────────────────────────────────
// Material asset
// ─────────────────────────────────────────────────────────────

/// GPU material holding one atlas texture per layer plus uniform data.
/// Updated by `update_ro_composite` each frame.
#[derive(Asset, TypePath, Clone)]
pub struct RoCompositeMaterial {
    /// Atlas image handle per layer slot.  Unused slots hold the default handle.
    pub textures:     [Handle<Image>; MAX_LAYERS],
    pub canvas_size:  Vec2,
    pub layer_count:  u32,
    pub layers:       [LayerUniform; MAX_LAYERS],
}

impl Default for RoCompositeMaterial {
    fn default() -> Self {
        Self {
            textures:    std::array::from_fn(|_| Handle::default()),
            canvas_size: Vec2::ONE,
            layer_count: 0,
            layers:      [LayerUniform::default(); MAX_LAYERS],
        }
    }
}

impl AsBindGroup for RoCompositeMaterial {
    type Data  = ();
    type Param = (SRes<RenderAssets<GpuImage>>, SRes<FallbackImage>);

    fn as_bind_group(
        &self,
        layout_descriptor: &BindGroupLayoutDescriptor,
        render_device:     &RenderDevice,
        pipeline_cache:    &PipelineCache,
        (image_assets, fallback_image): &mut SystemParamItem<Self::Param>,
    ) -> Result<PreparedBindGroup, AsBindGroupError> {
        let layout = pipeline_cache.get_bind_group_layout(layout_descriptor);
        // Build a &[&wgpu::TextureView] by double-dereffing Bevy's TextureView wrapper.
        let fallback = &fallback_image.d2.texture_view;
        let mut views: Vec<_> = vec![&**fallback; MAX_LAYERS];
        for (i, handle) in self.textures.iter().enumerate().take(self.layer_count as usize) {
            match image_assets.get(handle) {
                Some(img) => views[i] = &*img.texture_view,
                None      => return Err(AsBindGroupError::RetryNextUpdate),
            }
        }

        let uniform = CompositeUniform {
            canvas_size: self.canvas_size.into(),
            layer_count: self.layer_count,
            _pad:        0,
            layers:      self.layers,
        };
        let uniform_buf = render_device.create_buffer_with_data(&BufferInitDescriptor {
            label:    Some("ro_composite_uniform"),
            contents: bytemuck::bytes_of(&uniform),
            usage:    BufferUsages::STORAGE | BufferUsages::COPY_DST,
        });

        let bind_group = render_device.create_bind_group(
            Self::label(),
            &layout,
            &[
                BindGroupEntry {
                    binding:  0,
                    resource: BindingResource::TextureViewArray(views.as_slice()),
                },
                BindGroupEntry {
                    binding:  1,
                    resource: BindingResource::Sampler(&fallback_image.d2.sampler),
                },
                BindGroupEntry {
                    binding:  2,
                    resource: uniform_buf.as_entire_binding(),
                },
            ],
        );

        Ok(PreparedBindGroup {
            bindings:   BindingResources(vec![]),
            bind_group,
        })
    }

    fn unprepared_bind_group(
        &self,
        _layout:           &BindGroupLayout,
        _render_device:    &RenderDevice,
        _param:            &mut SystemParamItem<Self::Param>,
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
                (0, texture_2d(TextureSampleType::Float { filterable: true })
                    .count(NonZero::new(MAX_LAYERS as u32).unwrap())),
                (1, sampler(SamplerBindingType::Filtering)),
                (2, storage_buffer_read_only_sized(
                    false,
                    NonZero::new(std::mem::size_of::<CompositeUniform>() as u64),
                )),
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

/// Describes one layer in a composite sprite (body, head, headgear, …).
pub struct CompositeLayerDef {
    pub atlas:   Handle<RoAtlas>,
    /// Draw order: lower values are drawn first (further back).
    pub z_order: i32,
}

/// Drive a single-quad composite billboard from multiple RoAtlas layers.
///
/// Attach alongside `Mesh3d`, `MeshMaterial3d<RoCompositeMaterial>`, and `Transform`.
#[derive(Component)]
pub struct RoComposite {
    pub layers:        Vec<CompositeLayerDef>,
    pub tag:           Option<String>,
    pub playing:       bool,
    pub speed:         f32,
    pub current_frame: u16,
    pub elapsed:       Duration,
}

impl Default for RoComposite {
    fn default() -> Self {
        Self {
            layers:        Vec::new(),
            tag:           None,
            playing:       true,
            speed:         1.0,
            current_frame: 0,
            elapsed:       Duration::ZERO,
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
        app.add_systems(Update, update_ro_composite);
    }
}

/// Advances animation and rebuilds the `RoCompositeMaterial` uniforms each frame.
pub fn update_ro_composite(
    mut composites: Query<(
        &mut RoComposite,
        &MeshMaterial3d<RoCompositeMaterial>,
        &mut Transform,
    )>,
    atlases:    Res<Assets<RoAtlas>>,
    layouts:    Res<Assets<TextureAtlasLayout>>,
    mut mats:   ResMut<Assets<RoCompositeMaterial>>,
    time:       Res<Time>,
    camera_q:   Query<&GlobalTransform, With<Camera3d>>,
) {
    // Camera right/up in world space: the billboard's local axes after look_at.
    // Used to convert canvas-pixel offsets to world-space translation.
    let (cam_right, cam_up) = if let Ok(cam_gt) = camera_q.single() {
        (cam_gt.right(), cam_gt.up())
    } else {
        (Dir3::X, Dir3::Y)
    };
    for (mut composite, mat_handle, mut transform) in &mut composites {
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
                    composite.current_frame = if next > *tag_range.end() {
                        *tag_range.start()
                    } else {
                        next
                    };
                }
            }
        }

        let frame = composite.current_frame as usize;

        // ── 2. Collect per-layer frame data ───────────────────────────────
        struct FrameInfo {
            image:         Handle<Image>,
            uv_min:        Vec2,
            uv_max:        Vec2,
            size_px:       Vec2,
            origin:        IVec2,
            /// Attach-point displacement in feet-origin pixel space.
            /// = anchor_attach − self_attach; zero for anchor layer and for layers
            /// whose attach points match the anchor's (weapons, garments, headgear).
            attach_offset: Vec2,
        }

        // Sort layers by z_order; the first entry is treated as the anchor (body).
        let mut sorted: Vec<&CompositeLayerDef> = composite.layers.iter().collect();
        sorted.sort_by_key(|l| l.z_order);

        // Resolve anchor attach point from the first (lowest z-order) layer.
        let anchor_attach: Option<Vec2> = sorted.first().and_then(|l| {
            atlases.get(&l.atlas)
                .and_then(|a| a.frame_attach_points.get(frame).copied().flatten())
                .map(|ap| ap.as_vec2())
        });

        let mut frames: Vec<FrameInfo> = Vec::with_capacity(sorted.len());

        let mut all_ready = true;
        for layer_def in &sorted {
            let Some(atlas)  = atlases.get(&layer_def.atlas) else { all_ready = false; break };
            let Some(layout) = layouts.get(&atlas.atlas_layout) else { all_ready = false; break };

            let atlas_idx = atlas.get_atlas_index(frame);
            let rect = layout.textures[atlas_idx];
            let atlas_size = layout.size.as_vec2();

            let uv_min = rect.min.as_vec2() / atlas_size;
            let uv_max = rect.max.as_vec2() / atlas_size;
            let size_px = (rect.max - rect.min).as_vec2();
            let origin = atlas.frame_origins.get(frame).copied().unwrap_or(IVec2::ZERO);

            let self_attach = atlas.frame_attach_points.get(frame).copied().flatten()
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
        let mut content_min = Vec2::ZERO;    // body top-left at canvas (0, 0)
        let mut content_max = body.size_px;  // body bottom-right
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

        // ── 4. Build layer uniforms ───────────────────────────────────────
        let mut textures: [Handle<Image>; MAX_LAYERS] = std::array::from_fn(|_| Handle::default());
        let mut layer_uniforms = [LayerUniform::default(); MAX_LAYERS];
        let count = frames.len().min(MAX_LAYERS) as u32;

        for (i, fi) in frames.iter().enumerate().take(MAX_LAYERS) {
            textures[i] = fi.image.clone();
            // top-left of this layer's frame in canvas pixels
            let offset = canvas_feet + fi.attach_offset - fi.origin.as_vec2();
            layer_uniforms[i] = LayerUniform {
                atlas_uv_min:  fi.uv_min.into(),
                atlas_uv_max:  fi.uv_max.into(),
                canvas_offset: offset.into(),
                layer_size:    fi.size_px.into(),
            };
        }

        // ── 5. Update material ────────────────────────────────────────────
        if let Some(mat) = mats.get_mut(&mat_handle.0) {
            mat.textures    = textures;
            mat.canvas_size = canvas_size;
            mat.layer_count = count;
            mat.layers      = layer_uniforms;
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

/// Builds the action tag string for use with [`RoComposite::tag`].
pub fn composite_tag(action: &str, dir: u8) -> String {
    const DIRS: &[&str] = &["s", "sw", "w", "nw", "n", "ne", "e", "se"];
    format!("{}_{}", action, DIRS[dir as usize % 8])
}
