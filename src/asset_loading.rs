use std::io::Cursor;

use bevy_app::{App, Plugin};
use bevy_asset::{Asset, AssetApp, AssetLoader};
use bevy_reflect::TypePath;

use crate::file_resolver::{FilePreloaderError, StructuredInMemoryTemplate};

pub struct TypstTextureAssetsPlugin;

impl Plugin for TypstTextureAssetsPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<TypstZip>();
        app.init_asset_loader::<TypstZipLoader>();
    }
}

#[derive(Debug, Default)]
pub struct TypstZipLoader;

#[non_exhaustive]
#[derive(Debug)]
pub enum TypstAssetError {
    Io(std::io::Error),
    Zip(zip::result::ZipError),
    Preloader(FilePreloaderError),
}

impl std::fmt::Display for TypstAssetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TypstAssetError::Io(error) => write!(f, "TypstAssetError::Io: {error}"),
            TypstAssetError::Zip(zip_error) => write!(f, "TypstAssetError::Zip: {zip_error}"),
            TypstAssetError::Preloader(file_preloader_error) => {
                write!(f, "TypstAssetError::Preloader: {file_preloader_error}")
            }
        }
    }
}

impl std::error::Error for TypstAssetError {}

#[derive(Debug, Asset, TypePath)]
pub struct TypstZip(pub StructuredInMemoryTemplate);

impl AssetLoader for TypstZipLoader {
    type Asset = TypstZip;

    type Settings = ();

    type Error = TypstAssetError;

    async fn load(
        &self,
        reader: &mut dyn bevy_asset::io::Reader,
        _settings: &Self::Settings,
        _load_context: &mut bevy_asset::LoadContext<'_>,
    ) -> std::result::Result<Self::Asset, Self::Error> {
        let mut buffer: Vec<u8> = vec![];
        reader
            .read_to_end(&mut buffer)
            .await
            .map_err(TypstAssetError::Io)?;
        let cursor = Cursor::new(buffer);
        let zip = zip::ZipArchive::new(cursor).map_err(TypstAssetError::Zip)?;
        let resolver = StructuredInMemoryTemplate::from_zip(zip)?;
        Ok(TypstZip(resolver))
    }
}
