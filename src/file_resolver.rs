use derive_more::*;
use std::{
    collections::BTreeMap,
    io::{Read, Seek},
    path::PathBuf,
};
use typst_as_lib::{TypstEngine, TypstTemplateMainFile};
use zip::ZipArchive;

use typst::syntax::{FileId, Source, VirtualPath};

use crate::asset_loading::TypstAssetError;

use serde::{Deserialize, Serialize};
#[cfg(any(feature = "typst-asset-fonts", feature = "typst-search-system-fonts",))]
use typst_as_lib::typst_kit_options::TypstKitFontOptions;

#[derive(Debug, Error, Display)]
pub enum FilePreloaderError {
    NoPackageDotToml,
    MalformedPackageToml,
    NoMainDotTyp,
}

#[derive(Debug, Clone, Default)]
pub struct StructuredInMemoryTemplate {
    pub loaded_toml: BevyTypstDotToml,
    pub loaded_fonts: Vec<typst::text::Font>,
    pub loaded_main: String,
    pub path_given: PathBuf,
    pub file_resolver: Vec<(FileId, Vec<u8>)>,
    pub source_resolver: Vec<Source>,
}

impl StructuredInMemoryTemplate {
    pub fn to_engine(self) -> (TypstEngine<TypstTemplateMainFile>, BevyTypstDotToml) {
        let engine = TypstEngine::builder()
            .main_file(self.loaded_main)
            .with_static_file_resolver(self.file_resolver)
            .with_static_source_file_resolver(self.source_resolver)
            .fonts(self.loaded_fonts);
        #[cfg(all(
            feature = "typst-packages",
            any(feature = "typst-resolve-ureq", feature = "typst-resolve-reqwest")
        ))]
        let engine = engine.with_package_file_resolver();
        #[cfg(feature = "typst-asset-fonts")]
        let engine = engine.search_fonts_with(
            TypstKitFontOptions::default()
                .include_system_fonts(cfg!(feature = "typst-search-system-fonts")),
        );
        #[cfg(all(
            feature = "typst-search-system-fonts",
            not(feature = "typst-asset-fonts")
        ))]
        let engine =
            engine.search_fonts_with(TypstKitFontOptions::default().include_system_fonts(true));
        let engine = engine.build();
        (engine, self.loaded_toml)
    }

    pub fn from_zip<R: Read + Seek>(mut zip: ZipArchive<R>) -> Result<Self, TypstAssetError> {
        use serde::Deserialize;
        let mut typst_dot_toml_path = None;
        let mut main_dot_typ = None;
        let mut loaded_fonts = vec![];
        let mut source_resolver = vec![];
        let mut file_resolver = vec![];
        let mut prefix = None;
        for ix in 0..zip.len() {
            let mut file = zip.by_index(ix).map_err(TypstAssetError::Zip)?;
            if prefix.is_none() {
                prefix = Some(file.name().to_owned())
            }
            if file.is_file() {
                let path_buf = PathBuf::from(file.name());
                if path_buf.starts_with("__MACOSX") {
                    continue;
                }
                let path = path_buf.strip_prefix(prefix.as_ref().unwrap()).unwrap();
                match path.extension().and_then(|os| os.to_str()) {
                    Some("typ") => {
                        if path.file_name().unwrap() == "main.typ" {
                            let mut string_buf = String::new();
                            file.read_to_string(&mut string_buf)
                                .map_err(TypstAssetError::Io)?;
                            main_dot_typ = Some(string_buf);
                        } else {
                            let mut string_buf = String::new();
                            file.read_to_string(&mut string_buf)
                                .map_err(TypstAssetError::Io)?;
                            let source =
                                Source::new(FileId::new(None, VirtualPath::new(path)), string_buf);
                            source_resolver.push(source);
                        }
                    }
                    Some("otf") => {
                        let mut buf = Vec::new();
                        file.read_to_end(&mut buf).map_err(TypstAssetError::Io)?;
                        if let Some(font) =
                            typst::text::Font::new(typst::foundations::Bytes::new(buf), 0)
                        {
                            loaded_fonts.push(font);
                        }
                    }
                    Some("toml") if path.file_name().unwrap() == "package.toml" => {
                        let mut string_buf = String::new();
                        file.read_to_string(&mut string_buf)
                            .map_err(TypstAssetError::Io)?;
                        typst_dot_toml_path = Some(
                            BevyTypstDotToml::deserialize(toml::Deserializer::new(&string_buf))
                                .map_err(|_| {
                                    TypstAssetError::Preloader(
                                        FilePreloaderError::MalformedPackageToml,
                                    )
                                })?,
                        );
                    }
                    _ => {
                        let mut buf = Vec::new();
                        file.read_to_end(&mut buf).map_err(TypstAssetError::Io)?;
                        file_resolver.push((FileId::new(None, VirtualPath::new(path)), buf));
                    }
                }
            }
        }
        let loaded_main =
            main_dot_typ.ok_or(TypstAssetError::Preloader(FilePreloaderError::NoMainDotTyp))?;
        let loaded_toml = typst_dot_toml_path.unwrap_or_default();
        Ok(StructuredInMemoryTemplate {
            loaded_toml,
            loaded_fonts,
            path_given: PathBuf::from("/"),
            file_resolver,
            source_resolver,
            loaded_main,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BevyTypstDotToml {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub asset_requests: BTreeMap<PathBuf, Option<FileTypeHint>>,
    #[serde(default)]
    pub package_requests: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FileTypeHint {
    Image,
    Font,
    Typst,
}
