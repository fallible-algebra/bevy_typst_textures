use std::io::Cursor;

use bevy_app::{App, Plugin};
use bevy_asset::{Asset, AssetApp, AssetLoader, AsyncReadExt};
use bevy_reflect::TypePath;

use crate::file_resolver::{FilePreloaderError, StructuredInMemoryTemplate};

pub struct AssetPluginForTypstTextures;

impl Plugin for AssetPluginForTypstTextures {
    fn build(&self, app: &mut App) {
        app.init_asset::<TypstTemplate>();
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
    UnsupportedFormat,
}

impl std::fmt::Display for TypstAssetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TypstAssetError::Io(error) => write!(f, "TypstAssetError::Io: {error}"),
            TypstAssetError::Zip(zip_error) => write!(f, "TypstAssetError::Zip: {zip_error}"),
            TypstAssetError::Preloader(file_preloader_error) => {
                write!(f, "TypstAssetError::Preloader: {file_preloader_error}")
            }
            TypstAssetError::UnsupportedFormat => write!(
                f,
                "TypstAssetError::UnsupportedFormat: Neither a .zip archive or a standalone .typ file"
            ),
        }
    }
}

impl std::error::Error for TypstAssetError {}

#[derive(Debug, Asset, TypePath)]
pub struct TypstTemplate(pub StructuredInMemoryTemplate);

impl AssetLoader for TypstZipLoader {
    type Asset = TypstTemplate;

    type Settings = ();

    type Error = TypstAssetError;

    async fn load(
        &self,
        reader: &mut dyn bevy_asset::io::Reader,
        _settings: &Self::Settings,
        load_context: &mut bevy_asset::LoadContext<'_>,
    ) -> std::result::Result<Self::Asset, Self::Error> {
        if load_context
            .path()
            .extension()
            .is_some_and(|ext| ext == "zip")
        {
            let mut buffer: Vec<u8> = vec![];
            reader
                .read_to_end(&mut buffer)
                .await
                .map_err(TypstAssetError::Io)?;
            let cursor = Cursor::new(buffer);
            let zip = zip::ZipArchive::new(cursor).map_err(TypstAssetError::Zip)?;
            let resolver = StructuredInMemoryTemplate::from_zip(zip)?;
            Ok(TypstTemplate(resolver))
        } else if load_context
            .path()
            .extension()
            .is_some_and(|ext| ext == "typ")
        {
            // Standalone file.
            if cfg!(not(any(
                feature = "typst-asset-fonts",
                feature = "typst-search-system-fonts"
            ))) {
                bevy_log::warn!(
                    "[TYPST WARNING] Standalone typst file being loaded without either of the 'typst-asset-fonts' or 'typst-search-system-fonts' features enabled. Compilation may fail if text is output is displayed."
                );
            }
            let mut buffer = String::new();
            reader
                .read_to_string(&mut buffer)
                .await
                .map_err(TypstAssetError::Io)?;
            Ok(TypstTemplate(StructuredInMemoryTemplate {
                loaded_main: buffer,
                ..Default::default()
            }))
        } else {
            Err(TypstAssetError::UnsupportedFormat)
        }
    }
}
