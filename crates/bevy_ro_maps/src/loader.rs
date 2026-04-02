use anyhow::{Context, Result};
use bevy::asset::{io::Reader, AssetLoader, LoadContext};
use bevy::log::error;
use ro_maps::{GatFile, GndFile, RswFile};

use crate::assets::RoMapAsset;

#[derive(Default, bevy::prelude::TypePath)]
pub struct RoMapLoader;

impl AssetLoader for RoMapLoader {
    type Asset = RoMapAsset;
    type Settings = ();
    type Error = anyhow::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        load_context: &mut LoadContext<'_>,
    ) -> Result<RoMapAsset> {
        // Read the primary .gnd file
        let mut gnd_bytes = Vec::new();
        reader.read_to_end(&mut gnd_bytes).await?;
        let gnd = GndFile::parse(&gnd_bytes).inspect_err(|e| error!("{e:#}"))
            .with_context(|| format!("parsing {}", load_context.path()))?;

        // Co-load .gat and .rsw from the same base path
        let base = load_context.path().path().to_path_buf();

        let gat_bytes = load_context
            .read_asset_bytes(base.with_extension("gat"))
            .await?;
        let gat = GatFile::parse(&gat_bytes)?;

        let rsw_bytes = load_context
            .read_asset_bytes(base.with_extension("rsw"))
            .await?;
        let rsw = RswFile::parse(&rsw_bytes)?;

        Ok(RoMapAsset {
            gnd,
            gat,
            lighting: rsw.lighting,
            objects: rsw.objects,
        })
    }

    fn extensions(&self) -> &[&str] {
        &["gnd"]
    }
}
