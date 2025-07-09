use derive_more::*;
use std::{
    io::{Read, Seek},
    path::PathBuf,
};
use zip::ZipArchive;

use typst::syntax::{FileId, Source, VirtualPath};

use crate::asset_loading::TypstAssetError;

use super::template::BevyTypstDotToml;

pub struct ArchiveFileResolver<T> {
    pub archive: T,
    pub in_memory_fs: T,
    pub allow_packages: bool,
}

#[derive(Debug, Error, Display)]
pub enum FilePreloaderError {
    NoPackageDotToml,
    MalformedPackageToml,
    NoMainDotTyp,
}

#[derive(Debug)]
pub struct StaticResolverForBoth {
    pub loaded_toml: BevyTypstDotToml,
    pub loaded_fonts: Vec<typst::text::Font>,
    pub loaded_main: String,
    pub path_given: PathBuf,
    pub file_resolver: Vec<(FileId, Vec<u8>)>,
    pub source_resolver: Vec<Source>,
}

macro_rules! log_and_pass {
    ($x:expr) => {{
        if $x.is_err() {
            println!(
                "[{}:{}] Typst file resolution warning: {:?}",
                file!(),
                line!(),
                $x
            )
        }
        $x
    }};
}

impl StaticResolverForBoth {
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
        let loaded_toml = typst_dot_toml_path.ok_or(TypstAssetError::Preloader(
            FilePreloaderError::NoPackageDotToml,
        ))?;
        Ok(StaticResolverForBoth {
            loaded_toml,
            loaded_fonts,
            path_given: PathBuf::from("/"),
            file_resolver,
            source_resolver,
            loaded_main,
        })
    }

    // #[cfg(not(target_arch = "wasm32"))]
    // pub fn get_from_local_dir(root: PathBuf) -> Result<Self, FilePreloaderError> {
    //     use serde::Deserialize;
    //     use std::io::Read;
    //     use typst::syntax::Source;
    //     let walk = walkdir::WalkDir::new(&root);
    //     let mut typst_dot_toml_path = None;
    //     let mut main_dot_typ = None;
    //     let mut font_paths = vec![];
    //     let mut source_paths = vec![];
    //     let mut bin_paths = vec![];
    //     for dir_entry in walk {
    //         match dir_entry {
    //             Ok(dir_entry) => {
    //                 if dir_entry.file_type().is_file() {
    //                     // match on files.
    //                     match dir_entry
    //                         .path()
    //                         .extension()
    //                         .and_then(|os_str| os_str.to_str())
    //                     {
    //                         Some("typ") => {
    //                             if dir_entry.file_name() == "main.typ" {
    //                                 main_dot_typ = Some(dir_entry.path().to_owned());
    //                             } else {
    //                                 source_paths.push(dir_entry.path().to_owned())
    //                             }
    //                         }
    //                         Some("otf") => font_paths.push(dir_entry.path().to_owned()),
    //                         Some("toml") if dir_entry.file_name() == "package.toml" => {
    //                             typst_dot_toml_path = Some(dir_entry.path().to_owned())
    //                         }
    //                         _ => bin_paths.push(dir_entry.path().to_owned()),
    //                     }
    //                 }
    //             }
    //             Err(err) => {
    //                 bevy::log::error!("Error during during typst preload: {err}")
    //             }
    //         }
    //     }

    //     let join_and_open = |path: PathBuf| log_and_pass!(fs::File::open(dbg!(&path))).ok();
    //     let make_path_relative = |path: PathBuf| {
    //         let one_pass = path.strip_prefix(&root).unwrap();
    //         let final_pass = if one_pass.is_absolute() {
    //             one_pass.strip_prefix("/").unwrap()
    //         } else {
    //             one_pass
    //         };
    //         VirtualPath::new(final_pass)
    //     };
    //     let loaded_toml = typst_dot_toml_path
    //         .and_then(join_and_open)
    //         .and_then(|mut file| {
    //             let mut buf = String::new();
    //             log_and_pass!(file.read_to_string(&mut buf)).ok()?;
    //             Some(buf)
    //         })
    //         .and_then(|data| BevyTypstDotToml::deserialize(toml::Deserializer::new(&data)).ok())
    //         .ok_or(FilePreloaderError::NoPackageDotToml)?;
    //     let loaded_main = main_dot_typ
    //         .and_then(join_and_open)
    //         .and_then(|mut file| {
    //             let mut buf = String::new();
    //             log_and_pass!(file.read_to_string(&mut buf)).ok()?;
    //             Some(buf)
    //         })
    //         .ok_or(FilePreloaderError::NoMainDotTyp)?;
    //     let loaded_fonts: Vec<_> = font_paths
    //         .into_iter()
    //         .filter_map(join_and_open)
    //         .filter_map(|mut file| {
    //             let mut buf = vec![];
    //             log_and_pass!(file.read_to_end(&mut buf)).ok()?;
    //             typst::text::Font::new(typst::foundations::Bytes::from(buf), 0)
    //         })
    //         .collect();
    //     // let file_resolver = StaticFileResolver::new(std::iter::empty());
    //     let binaries = bin_paths
    //         .into_iter()
    //         .filter_map(|relative_path| {
    //             join_and_open(relative_path.clone()).map(|file| (file, relative_path))
    //         })
    //         .filter_map(|(mut file, relative_path)| {
    //             let mut buf = Vec::new();
    //             log_and_pass!(file.read_to_end(&mut buf)).ok()?;
    //             Some((FileId::new(None, make_path_relative(relative_path)), buf))
    //         });
    //     let file_resolver = binaries.collect();
    //     let sources = source_paths
    //         .into_iter()
    //         .filter_map(|relative_path| {
    //             join_and_open(relative_path.clone()).map(|file| (file, relative_path))
    //         })
    //         .filter_map(|(mut file, relative_path)| {
    //             let mut buf = String::new();
    //             log_and_pass!(file.read_to_string(&mut buf)).ok()?;
    //             Some(Source::new(
    //                 FileId::new(None, make_path_relative(relative_path)),
    //                 buf,
    //             ))
    //         });
    //     let source_resolver = sources.collect();
    //     Ok(StaticResolverForBoth {
    //         loaded_toml,
    //         loaded_fonts,
    //         path_given: root.clone(),
    //         file_resolver,
    //         source_resolver,
    //         loaded_main,
    //     })
    // }
}
