use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::ops::RangeInclusive;
use std::time::Duration;

use bevy::{
    asset::{io::Reader, AssetLoader, RenderAssetUsages},
    image::ImageSampler,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use ro_sprite::{composite::render_frame_tight, ActFile, ImfFile, SprFile};

use crate::action_label;

#[derive(Asset, TypePath, Debug)]
pub struct RoAtlas {
    pub atlas_layout: Handle<TextureAtlasLayout>,
    pub atlas_image: Handle<Image>,
    pub frame_durations: Vec<Duration>,
    /// Feet position (sprite coordinate origin) within each frame's atlas rect, in pixels.
    pub frame_origins: Vec<IVec2>,
    /// ACT attach point per logical frame, in feet-origin pixel space. `None` if the frame
    /// has no attach point (e.g. monsters, NPCs). For body sprites this is the head anchor;
    /// for head sprites this is the head's own anchor used to align to the body.
    pub frame_attach_points: Vec<Option<IVec2>>,
    /// action tag name → frame range (inclusive) in the logical frame sequence
    pub tags: HashMap<String, TagMeta>,
    /// logical frame index → atlas slot index (after deduplication)
    pub frame_indices: Vec<usize>,
    /// Per logical frame: whether the IMF file says the head layer (layer 1) should render
    /// behind the body. Only meaningful for body sprites that have an associated `.imf` file;
    /// always `false` for other sprite types.
    pub frame_head_behind: Vec<bool>,
}

impl RoAtlas {
    pub fn get_atlas_index(&self, frame: usize) -> usize {
        self.frame_indices
            .get(frame)
            .copied()
            .unwrap_or_else(|| self.frame_indices.last().copied().unwrap_or(0))
    }
}

#[derive(Debug, Clone)]
pub struct TagMeta {
    pub range: RangeInclusive<u16>,
}

#[derive(Default, TypePath)]
pub struct RoAtlasLoader;

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct RoAtlasLoaderSettings {
    pub sampler: ImageSampler,
}

impl Default for RoAtlasLoaderSettings {
    fn default() -> Self {
        Self {
            sampler: ImageSampler::nearest(),
        }
    }
}

impl AssetLoader for RoAtlasLoader {
    type Asset = RoAtlas;
    type Settings = RoAtlasLoaderSettings;
    type Error = anyhow::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        settings: &Self::Settings,
        load_context: &mut bevy::asset::LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        // Read the .spr file
        let mut spr_bytes = Vec::new();
        reader.read_to_end(&mut spr_bytes).await?;
        let spr = SprFile::parse(&spr_bytes)?;

        // Derive the .act path from the asset path
        let spr_path = load_context.path().path().to_path_buf();
        let act_path = spr_path.with_extension("act");
        let imf_path = spr_path.with_extension("imf");

        let act_bytes = load_context.read_asset_bytes(act_path).await?;
        let act = ActFile::parse(&act_bytes)?;

        let imf: Option<ImfFile> =
            if let Ok(imf_bytes) = load_context.read_asset_bytes(imf_path).await {
                Some(ImfFile::parse(&imf_bytes)?)
            } else {
                None
            };

        // Determine which actions to include (skip "unknown" group 72-79 for 104-action sprites)
        let action_indices: Vec<usize> = (0..act.actions.len())
            .filter(|&i| !(act.actions.len() == 104 && (72..80).contains(&i)))
            .collect();

        // Pass 1: render each frame tight, deduplicate by pixel hash
        let mut unique_images: Vec<Image> = Vec::new();
        let mut hash_to_atlas: HashMap<u64, usize> = HashMap::new();
        let mut frame_indices: Vec<usize> = Vec::new();
        let mut frame_durations: Vec<Duration> = Vec::new();
        let mut frame_origins: Vec<IVec2> = Vec::new();
        let mut frame_attach_points: Vec<Option<IVec2>> = Vec::new();
        let mut frame_head_behind: Vec<bool> = Vec::new();

        let pad = 1i32;

        for &action_idx in &action_indices {
            let action = &act.actions[action_idx];
            let ms = u64::from(action.frame_ms().max(1));

            for (frame_idx, frame) in action.frames.iter().enumerate() {
                frame_durations.push(Duration::from_millis(ms));
                frame_attach_points
                    .push(frame.attach_points.first().map(|ap| IVec2::new(ap.x, ap.y)));

                // IMF priority(layer=1, action, frame) == 1 → head renders behind body.
                frame_head_behind.push(
                    imf.as_ref()
                        .and_then(|f| f.priority(1, action_idx, frame_idx)) == Some(1),
                );

                match render_frame_tight(&spr, frame, pad) {
                    Some((buf, origin_x, origin_y)) => {
                        let hash = hash_pixels(&buf.pixels);
                        let atlas_idx = if let Some(&idx) = hash_to_atlas.get(&hash) {
                            // Reuse origin from the deduplicated frame
                            frame_origins.push(IVec2::new(origin_x, origin_y));
                            idx
                        } else {
                            let idx = unique_images.len();
                            hash_to_atlas.insert(hash, idx);

                            let image = Image {
                                sampler: settings.sampler.clone(),
                                ..Image::new(
                                    Extent3d {
                                        width: buf.width,
                                        height: buf.height,
                                        depth_or_array_layers: 1,
                                    },
                                    TextureDimension::D2,
                                    buf.pixels,
                                    TextureFormat::Rgba8UnormSrgb,
                                    RenderAssetUsages::default(),
                                )
                            };
                            unique_images.push(image);
                            frame_origins.push(IVec2::new(origin_x, origin_y));
                            idx
                        };
                        frame_indices.push(atlas_idx);
                    }
                    None => {
                        // Invisible frame: use a 1×1 transparent placeholder
                        let hash = 0u64; // all-zero pixels → same slot
                        let atlas_idx = if let Some(&idx) = hash_to_atlas.get(&hash) {
                            frame_origins.push(IVec2::ZERO);
                            idx
                        } else {
                            let idx = unique_images.len();
                            hash_to_atlas.insert(hash, idx);
                            let image = Image {
                                sampler: settings.sampler.clone(),
                                ..Image::new(
                                    Extent3d {
                                        width: 1,
                                        height: 1,
                                        depth_or_array_layers: 1,
                                    },
                                    TextureDimension::D2,
                                    vec![0u8; 4],
                                    TextureFormat::Rgba8UnormSrgb,
                                    RenderAssetUsages::default(),
                                )
                            };
                            unique_images.push(image);
                            frame_origins.push(IVec2::ZERO);
                            idx
                        };
                        frame_indices.push(atlas_idx);
                    }
                }
            }
        }

        // Build TextureAtlasLayout + packed atlas image using TextureAtlasBuilder
        let mut atlas_builder = TextureAtlasBuilder::default();
        atlas_builder.max_size(UVec2::splat(8192));

        // We need stable IDs to map images → atlas indices
        let mut image_ids: Vec<AssetId<Image>> = Vec::new();
        for image in &unique_images {
            let id = AssetId::Uuid {
                uuid: uuid::Uuid::new_v4(),
            };
            image_ids.push(id);
            atlas_builder.add_texture(Some(id), image);
        }

        let (layout, source, atlas_image) = atlas_builder.build()?;

        // Remap frame_indices through the atlas builder's reordering
        let remapped: Vec<usize> = frame_indices
            .iter()
            .map(|&i| *source.texture_ids.get(&image_ids[i]).unwrap())
            .collect();

        // Build tags map
        let mut tags: HashMap<String, TagMeta> = HashMap::new();
        let mut cursor = 0u16;
        for &action_idx in &action_indices {
            let n = act.actions[action_idx].frames.len() as u16;
            let name = action_label(action_idx, act.actions.len());
            tags.insert(
                name,
                TagMeta {
                    range: cursor..=cursor + n - 1,
                },
            );
            cursor += n;
        }

        let atlas_layout = load_context.add_labeled_asset("atlas_layout".into(), layout);
        let atlas_image = load_context.add_labeled_asset("atlas_texture".into(), atlas_image);

        Ok(RoAtlas {
            atlas_layout,
            atlas_image,
            frame_durations,
            frame_origins,
            frame_attach_points,
            tags,
            frame_indices: remapped,
            frame_head_behind,
        })
    }

    fn extensions(&self) -> &[&str] {
        &["spr"]
    }
}

fn hash_pixels(bytes: &[u8]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}
