use bevy::prelude::*;
use ro_rsm::RsmFile;

#[derive(Asset, TypePath)]
pub struct RsmAsset {
    pub rsm: RsmFile,
}
