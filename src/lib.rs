//! A simple `Resource` for generating rasterized textures (`Handle<Image>`) out of either standalone .typ files or structured, zipped typst projects, built on `typst-as-lib`.
//!
//! # Example
//!
//! To use this crate, add the `TypstTexturesPlugin` to your bevy app then request textures through `TypstTextureServer`:
//!
//! ```no_run
//! use bevy::prelude::*;
//! use bevy_typst_textures::{TypstJobOptions, TypstTextureServer, TypstTexturesPlugin};
//!
//! fn main() {
//!     App::new()
//!         .add_plugins((DefaultPlugins, TypstTexturesPlugin::default()))
//!         .add_systems(Startup, start)
//!         .run();
//! }
//!
//! fn start(mut commands: Commands, mut typst_server: ResMut<TypstTextureServer>) {
//!     commands.spawn(Camera2d);
//!     commands.spawn(Sprite {
//!         image: typst_server.add_job("my_zip_in_the_asset_folder.zip", TypstJobOptions::default()),
//!         ..default()
//!     });
//! }
//! ```
//!
//! ## Expected structure for Typst Assets
//!
//! Standalone `.typ` files can be loaded, but they will not have access to the bevy `asset/` folder or any other .typ files and if you want to display text then either the `typst-search-system-fonts` or `typst-asset-fonts` features must be enabled.
//!
//! For complex typst projects that need access to guaranteed, specific fonts as well as other assets, you'll need to create a **`.zip`** archive containing:
//! 1. a **`main.typ`** file.
//! 2. an optional `package.toml` file:
//!     - This doesn't need to be populated with anything right now.
//!     - That said, it expects:
//!         - a name field
//!         - a list of author strings
//!         - a list of bevy `asset/` folder asset requests (doesn't do anything right now)
//!         - a list of typst "universe" package requests (doesn't do anything right now)
//! 3. Any .otf fonts needed (they can exist anywhere, but a `fonts/` folder is a good idea)
//!     - unless either of the `typst-search-system-fonts` or `typst-asset-fonts` crate features are enabled, which will enable use of system fonts or the "default" typst fonts as embedded assets, respectively. This does still put the onus on you and your users to have these fonts either installed or bundled.
//! 4. Typst modules, assets, images, SVGs, data, etc.
//!
//! ## Limitations
//!
//! This project is built on top of the `typst-as-lib` crate, which provides a nice wrapper over the internals of `typst` for standalone projects. The limitations of `typst-as-lib` are inherited by this crate.
//!
//! This package expects typst assets as zip archives to simplify the asset-fetching process (as outlined above).
//!
//! Packages are supported, but not on web. This may change in the future, but for now this does not work.
//!
//! The archive unzipping is a bit fragile right now. Lots of `unwrap`s and assumptions about how different OSs handle zip archives, and some ad-hoc dealing with how they pollute filesystems with metadata (`_MACOS/` delenda est). Because zipping manually is a pain, I'd suggest setting up something to create zips of your typst assets folders in a `build.rs` script or as part of a watch command on your project.
//!
//! `add_job_with_data` uses serde to serialize the input data type to json before then de-seralizing it to typst's `Dict` type. This presents the regular `serde` overhead, mostly.
//!
//! ## Cargo Features
//!
//! All these features are pass-through features to `typst-as-lib` features.
//!
//! - `typst-packages`: Enable access to Universe packages. Package fetching is blocking, doesn't work on web, and relies on you also enabling one of the following:
//!     - `typst-resolve-ureq`: Use `ureq` to resolve packages.
//!     - `typst-resolve-reqwest`: Use `reqwest` to resolve packages.
//! - `typst-search-system-fonts`: Allow access to system fonts from Typst.
//! - `typst-asset-fonts`: Embed the "default" fonts of typst, embedding them directly in the program's executable.

use std::{
    collections::{HashMap, VecDeque},
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy_app::{Last, Plugin, PreStartup};
use bevy_asset::{AssetServer, Assets, Handle, RenderAssetUsages};
use bevy_ecs::{
    resource::Resource,
    system::{Commands, Res, ResMut},
};
use bevy_image::Image;
use bevy_tasks::AsyncComputeTaskPool;
use serde::Serialize;
use serde_json::{Map, value::Serializer};
use typst::{
    diag::Severity,
    foundations::{Dict, IntoValue},
    layout::PagedDocument,
};
use wgpu_types::{Extent3d, TextureDimension, TextureFormat};

use crate::{
    asset_loading::{AssetPluginForTypstTextures, TypstTemplate},
    file_resolver::StructuredInMemoryTemplate,
};

pub mod asset_loading;
pub mod file_resolver;

/// This crate's core plugin. Add this to your app to enable typst-related asset loading, the TypstTextureServer resource, and typst compilation/rasterisation system.
#[derive(Debug, Clone, Resource, Default)]
pub struct TypstTexturesPlugin {
    /// Optional limit on the number of rasterization jobs to process every frame.
    /// This can also be modified on the [`TypstTextureServer`] resource itself.
    pub jobs_per_frame: Option<u32>,
}

impl Plugin for TypstTexturesPlugin {
    fn build(&self, app: &mut bevy_app::App) {
        app.add_plugins(AssetPluginForTypstTextures);
        app.insert_resource(self.clone());
        app.add_systems(PreStartup, TypstTextureServer::system_insert_to_world)
            .add_systems(Last, TypstTextureServer::system_do_jobs);
    }
}

/// The data needed to complete a Typst job.
#[derive(Debug)]
pub struct TypstJob {
    pub use_template: Handle<TypstTemplate>,
    pub input: Dict,
    pub send_target: async_channel::Sender<bevy_image::Image>,
    pub job_options: TypstJobOptions,
    _handle: Handle<Image>,
}

#[derive(Debug, Default, Clone)]
pub enum InputUnifyMode {
    #[default]
    SerdeOverridesDict,
    DictOverridesSerde,
    /// Create a new Dict with the serde and dict inputs placed in sub-dictionaries under the given keys.
    SeparateKeys {
        serde_key: String,
        dict_key: String,
    },
}

/// Options for the typst job.
#[derive(Debug, Clone)]
pub struct TypstJobOptions {
    /// How many pixels correspond to a typst `pt`. Defaults to `1.`
    pub pixels_per_pt: f32,
    /// Which page to render, defaults to the first page when not specified, and is clamped by the total number of pages in the document.
    pub specific_page: Option<usize>,
    /// Options to pass to [`Image::asset_usage`], defaults to RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD.
    pub asset_usage: RenderAssetUsages,
    pub input_unify_mode: InputUnifyMode,
}

impl Default for TypstJobOptions {
    fn default() -> Self {
        Self {
            pixels_per_pt: 1.0,
            specific_page: None,
            asset_usage: RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
            input_unify_mode: InputUnifyMode::default(),
        }
    }
}

/// Resource to access in systems under `ResMut<TypstTextureServer>` to queue typst jobs. [`TypstTextureServer::add_job`] and [`TypstTextureServer::add_job_with_serde_input`]
#[derive(Debug, Resource)]
pub struct TypstTextureServer {
    asset_server: AssetServer,
    pub fallback: Image,
    pub templates: HashMap<PathBuf, Handle<TypstTemplate>>,
    pub jobs: VecDeque<TypstJob>,
    pub jobs_per_frame: Option<u32>,
}

impl TypstTextureServer {
    pub(crate) fn system_insert_to_world(
        mut commands: Commands,
        asset_server: Res<AssetServer>,
        plugin_settings: Res<TypstTexturesPlugin>,
    ) {
        let mut typst_template_server = Self::new(asset_server.clone());
        typst_template_server.jobs_per_frame = plugin_settings.jobs_per_frame;
        commands.remove_resource::<TypstTexturesPlugin>();
        commands.insert_resource(typst_template_server);
    }

    /// Runs in `Last`. Exposed here to allow for specific scheduling on the user's part.
    pub fn system_do_jobs(
        mut template_server: ResMut<TypstTextureServer>,
        templates: Res<Assets<TypstTemplate>>,
    ) {
        let max_jobs = template_server
            .jobs_per_frame
            .unwrap_or(template_server.jobs.len() as u32);
        let mut jobs_done = 0;
        let mut compiled_map = HashMap::new();
        while jobs_done < max_jobs
            && let Some(job) = template_server.jobs.pop_front()
        {
            if template_server.asset_server.is_loaded(&job.use_template)
                && let Some(template) = templates.get(&job.use_template)
            {
                let (engine, _) = compiled_map
                    .entry(job.use_template.clone())
                    .or_insert_with(|| template.0.clone().to_engine());
                let compiled = engine.compile_with_input::<_, PagedDocument>(job.input);
                let path = job.use_template.path();
                let Ok(page) = compiled.output else {
                    bevy_log::error!(
                        "[TYPST FATAL ERROR for {:?}] {}",
                        path,
                        compiled.output.unwrap_err()
                    );
                    continue;
                };
                for warning in compiled.warnings {
                    if warning.severity == Severity::Error {
                        bevy_log::error!("[TYPST ERROR for {:?}] {}", path, warning.message);
                    } else {
                        bevy_log::warn!("[TYPST WARNING for {:?}] {}", path, warning.message);
                    }
                }
                let rendered = typst_render::render(
                    &page.pages[job
                        .job_options
                        .specific_page
                        .map(|page_num| (page.pages.len().saturating_sub(1)).min(page_num))
                        .unwrap_or(0)],
                    job.job_options.pixels_per_pt,
                );
                let asset_usage = job.job_options.asset_usage;
                let sender = job.send_target.clone();
                AsyncComputeTaskPool::get()
                    .spawn(async move {
                        sender
                            .send(bevy_image::Image::new(
                                Extent3d {
                                    width: rendered.width(),
                                    height: rendered.height(),
                                    depth_or_array_layers: 1,
                                },
                                TextureDimension::D2,
                                rendered.data().to_vec(),
                                TextureFormat::Rgba8UnormSrgb,
                                asset_usage,
                            ))
                            .await
                    })
                    .detach();
            } else {
                template_server.jobs.push_back(job);
            }
            jobs_done += 1;
        }
    }

    /// Create a new typst texture server, using a cloned `AssetServer` for internal use.
    pub fn new(asset_server: AssetServer) -> Self {
        Self::new_with_fallback(
            asset_server,
            bevy_image::Image::new(
                Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                vec![255, 0, 255, 255],
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::MAIN_WORLD,
            ),
        )
    }

    /// Create a new typst texture server with a given fallback image. The fallback image is not currently used.
    pub fn new_with_fallback(asset_server: AssetServer, fallback: Image) -> Self {
        Self {
            asset_server,
            fallback,
            templates: HashMap::new(),
            jobs: VecDeque::new(),
            jobs_per_frame: None,
        }
    }

    /// Add a typst job to the queue, returning a [`Handle<Image>`] immediately while the
    /// compilation and rasterization happens later. A valid asset is either a .zip archive
    /// that follows the structure set out at the root of this crate or a standalone .typ
    /// file with no non-sys imports.
    pub fn add_job(
        &mut self,
        path: impl Into<PathBufOrTemplate>,
        options: TypstJobOptions,
    ) -> Handle<Image> {
        self.add_job_with_dict_input(path, Dict::default(), options)
    }

    /// Add a typst job to the queue, as per [`TypstTextureServer::add_job`], but with a dictionary as input,
    /// available in the typst program as
    pub fn add_job_with_dict_input(
        &mut self,
        path: impl Into<PathBufOrTemplate>,
        input: impl Into<Dict>,
        options: TypstJobOptions,
    ) -> Handle<Image> {
        let asset_server = self.asset_server.clone();
        let path_or_template: PathBufOrTemplate = path.into();
        let template = match path_or_template {
            PathBufOrTemplate::PathBuf(path_buf) => self
                .templates
                .entry(path_buf.clone())
                .or_insert_with(|| asset_server.load(path_buf))
                .clone(),
            PathBufOrTemplate::NewTemplate(structured_in_memory_template) => self
                .templates
                .entry(structured_in_memory_template.path_given.clone())
                .insert_entry(asset_server.add(TypstTemplate(structured_in_memory_template)))
                .get()
                .clone(),
            PathBufOrTemplate::ExistingTemplate(handle) => handle,
        };
        let (sender, receiver) = async_channel::unbounded::<bevy_image::Image>();
        let handle: Handle<Image> = self.asset_server.add_async(async move {
            let res = receiver.recv().await;
            if let Err(res) = &res {
                bevy_log::error!("[TYPST ASYNC JOB ERROR] {res}")
            }
            res
        });
        self.jobs.push_back(TypstJob {
            use_template: template.clone(),
            input: input.into(),
            send_target: sender,
            job_options: options,
            _handle: handle.clone(),
        });
        handle
    }

    /// Add a typst job to the queue, with both a Serde and Dict input type, unified together as a single dict.
    /// If your inputs share any keys, be sure to understand which [`InputUnifyMode`] is relevant to what you want, the default being [`InputUnifyMode::SerdeOverridesDict`].
    pub fn add_job_with_dict_and_serde_input(
        &mut self,
        path: impl Into<PathBufOrTemplate>,
        input_serde: impl Serialize,
        input_dict: impl Into<Dict>,
        options: TypstJobOptions,
    ) -> Handle<Image> {
        let Ok(serde_input): Result<serde_json::Value, _> = input_serde.serialize(Serializer)
        else {
            bevy_log::error!(
                "[TYPST INPUT ERROR] Could not transform value into a serde json as interim for Dict."
            );
            return self.add_job_with_dict_input(path, input_dict, options);
        };
        let Ok(mut input_serde_dict): Result<Dict, _> = serde_json::from_value(serde_input) else {
            bevy_log::error!("[TYPST INPUT ERROR] Could not get Dict from interim serde json.");
            return self.add_job_with_dict_input(path, input_dict, options);
        };
        let mut input_dict: Dict = input_dict.into();
        let input_unified = match options.input_unify_mode.clone() {
            InputUnifyMode::SerdeOverridesDict => {
                for (key, value) in input_serde_dict {
                    input_dict.insert(key, value);
                }
                input_dict
            }
            InputUnifyMode::DictOverridesSerde => {
                for (key, value) in input_dict {
                    input_serde_dict.insert(key, value);
                }
                input_serde_dict
            }
            InputUnifyMode::SeparateKeys {
                serde_key,
                dict_key,
            } => {
                let mut dict = Dict::new();
                dict.insert(serde_key.into(), input_serde_dict.into_value());
                dict.insert(dict_key.into(), input_dict.into_value());
                dict
            }
        };
        self.add_job_with_dict_input(path, input_unified, options)
    }

    /// Add a typst job to the queue with input, as per [`TypstTextureServer::add_job_with_dict_input`],
    /// but with the input generated from a serde serializable type.
    /// This method overrides the `input_unify_mode` of your job options, setting it to [`InputUnifyMode::SerdeOverridesDict`].
    pub fn add_job_with_serde_input(
        &mut self,
        path: impl Into<PathBufOrTemplate>,
        input: impl Serialize,
        options: TypstJobOptions,
    ) -> Handle<Image> {
        self.add_job_with_dict_and_serde_input(
            path,
            input,
            Dict::default(),
            TypstJobOptions {
                input_unify_mode: InputUnifyMode::SerdeOverridesDict,
                ..options
            },
        )
    }

    pub fn limit_jobs(mut self, limit: u32) -> Self {
        self.jobs_per_frame = Some(limit);
        self
    }

    pub fn unlimited_jobs(mut self) -> Self {
        self.jobs_per_frame = None;
        self
    }
}

/// A valid input to
#[derive(Debug)]
pub enum PathBufOrTemplate {
    PathBuf(PathBuf),
    /// When passing a new template as input, be sure that your `path_given` is unique.
    /// This is used as the key for the template server, so if you keep it blank then it
    /// could be overridden.
    NewTemplate(StructuredInMemoryTemplate),
    ExistingTemplate(Handle<TypstTemplate>),
}

impl From<PathBuf> for PathBufOrTemplate {
    fn from(value: PathBuf) -> Self {
        Self::PathBuf(value)
    }
}

impl From<&str> for PathBufOrTemplate {
    fn from(value: &str) -> Self {
        Self::PathBuf(value.into())
    }
}

impl From<String> for PathBufOrTemplate {
    fn from(value: String) -> Self {
        Self::PathBuf(value.into())
    }
}

impl From<&Path> for PathBufOrTemplate {
    fn from(value: &Path) -> Self {
        Self::PathBuf(value.into())
    }
}

impl From<&OsStr> for PathBufOrTemplate {
    fn from(value: &OsStr) -> Self {
        Self::PathBuf(value.into())
    }
}

impl From<StructuredInMemoryTemplate> for PathBufOrTemplate {
    fn from(value: StructuredInMemoryTemplate) -> Self {
        Self::NewTemplate(value)
    }
}

impl From<Handle<TypstTemplate>> for PathBufOrTemplate {
    fn from(value: Handle<TypstTemplate>) -> Self {
        PathBufOrTemplate::ExistingTemplate(value)
    }
}
