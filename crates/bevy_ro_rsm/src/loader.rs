use anyhow::Result;
use bevy::asset::{io::Reader, AssetLoader, LoadContext};
use bevy::log::error;
use ro_rsm::RsmFile;

use crate::assets::RsmAsset;

#[derive(Default, bevy::prelude::TypePath)]
pub struct RsmLoader;

impl AssetLoader for RsmLoader {
    type Asset = RsmAsset;
    type Settings = ();
    type Error = anyhow::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        load_context: &mut LoadContext<'_>,
    ) -> Result<RsmAsset> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let rsm = RsmFile::parse(&bytes)
            .inspect_err(|e| error!("[RoModel] failed to parse {}: {e:#}", load_context.path()))
            .map_err(|e| e.context(format!("parsing {}", load_context.path())))?;
        Ok(RsmAsset { rsm })
    }

    fn extensions(&self) -> &[&str] {
        &["rsm", "rsm2"]
    }
}
