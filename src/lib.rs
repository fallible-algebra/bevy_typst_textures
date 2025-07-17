//! A simple `Resource` for generating rasterized textures (`Handle<Image>`) out of structured, zipped typst projects, built on `typst-as-lib`.
//!
//! # Example
//!
//! To use this crate, add the `TypstTexturesPlugin` to your bevy app then request textures through `TypstTextureServer`:
//!
//! ```rust
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
//! A **`.zip`** archive containing:
//! 1. a **`main.typ`** file.
//! 2. an optional `package.toml` file:
//!     - This doesn't need to be populated with anything right now.
//!     - That said, it expects:
//!         - a name field
//!         - an list of author strings
//!         - a list of bevy `asset/` folder asset requests (doesn't do anything right now)
//!         - a list of typst "universe" package requests (doesn't do anything right now)
//! 3. Inclusion of all fonts needed (they can exist anywhere, but a `fonts/` folder is a good idea)
//!     - unless either of the `typst-search-system-fonts` or `typst-asset-fonts` crate features are enabled, which will enable use of system fonts or the "default" typst fonts as embedded assets, respectively.
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
//! - `packages`: Enable access to Universe packages. Package fetching is blocking, doesn't work on web, and relies on you also enabling one of the following:
//!     - `typst-resolve-ureq`: Use `ureq` to resolve packages.
//!     - `typst-resolve-reqwest`: Use `reqwest` to resolve packages.
//! - `typst-search-system-fonts`: Allow access to system fonts from Typst.
//! - `typst-asset-fonts`: Embed the "default" fonts of typst, embedding them directly in the program's executable.

use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
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
use typst::{diag::Severity, foundations::Dict, layout::PagedDocument};
use wgpu_types::{Extent3d, TextureDimension, TextureFormat};

use crate::asset_loading::{AssetPluginForTypstTextures, TypstZip};

pub mod asset_loading;
pub mod file_resolver;

/// This crate's core plugin. Add this to your app to enable typst zip asset loading, the TypstTextureServer resource, and typst compilation/rasterisation system.
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
    pub use_template: Handle<TypstZip>,
    pub input: Dict,
    pub send_target: async_channel::Sender<bevy_image::Image>,
    pub job_options: TypstJobOptions,
    _handle: Handle<Image>,
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
}

impl Default for TypstJobOptions {
    fn default() -> Self {
        Self {
            pixels_per_pt: 1.0,
            specific_page: None,
            asset_usage: RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
        }
    }
}

/// Resource to access in systems under `ResMut<TypstTextureServer>` to queue typst jobs. [`TypstTextureServer::add_job`] and [`TypstTextureServer::add_job_with_input`]
#[derive(Debug, Resource)]
pub struct TypstTextureServer {
    asset_server: AssetServer,
    pub fallback: Image,
    pub templates: HashMap<PathBuf, Handle<TypstZip>>,
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
        templates: Res<Assets<TypstZip>>,
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
    /// compilation and rasterization happens later.
    pub fn add_job(
        &mut self,
        zip_path: impl Into<PathBuf>,
        options: TypstJobOptions,
    ) -> Handle<Image> {
        self.add_job_with_input(zip_path, serde_json::Value::Object(Map::new()), options)
    }

    /// Add a typst job to the queue, with `input` being some type to be converted to the
    /// `Dict` accessible via `#import sys : inputs` in the typst project.
    pub fn add_job_with_input(
        &mut self,
        zip_path: impl Into<PathBuf>,
        input: impl Serialize,
        options: TypstJobOptions,
    ) -> Handle<Image> {
        let asset_server = self.asset_server.clone();
        let path_buf = zip_path.into();
        let template = self
            .templates
            .entry(path_buf.clone())
            .or_insert_with(|| asset_server.load(path_buf));
        let (sender, receiver) = async_channel::unbounded::<bevy_image::Image>();
        let handle: Handle<Image> = self.asset_server.add_async(async move {
            let res = receiver.recv().await;
            if let Err(res) = &res {
                bevy_log::error!("[TYPST ASYNC JOB ERROR] {res}")
            }
            res
        });
        let Ok(input): Result<serde_json::Value, _> = input.serialize(Serializer) else {
            bevy_log::error!(
                "[TYPST INPUT ERROR] Could not transform value into a serde json as interim for Dict."
            );
            return handle;
        };
        let Ok(input) = serde_json::from_value(input) else {
            bevy_log::error!("[TYPST INPUT ERROR] Could not get Dict from interim serde json.");
            return handle;
        };
        self.jobs.push_back(TypstJob {
            use_template: template.clone(),
            input,
            send_target: sender,
            job_options: options,
            _handle: handle.clone(),
        });
        handle
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
