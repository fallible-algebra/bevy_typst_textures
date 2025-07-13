use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
};

use bevy_app::{Plugin, Startup, Update};
use bevy_asset::{AssetServer, Assets, Handle, RenderAssetUsages};
use bevy_ecs::{
    resource::Resource,
    system::{Commands, Res, ResMut},
};
use bevy_image::Image;
use bevy_tasks::AsyncComputeTaskPool;
use serde::Serialize;
use serde_json::value::Serializer;
use typst::{diag::Severity, foundations::Dict, layout::PagedDocument};
use wgpu_types::{Extent3d, TextureDimension, TextureFormat};

use crate::asset_loading::{TypstTextureAssetsPlugin, TypstZip};

pub mod asset_loading;
pub mod file_resolver;

pub struct TypstTexturesPlugin;

impl Plugin for TypstTexturesPlugin {
    fn build(&self, app: &mut bevy_app::App) {
        app.add_plugins(TypstTextureAssetsPlugin);
        app.add_systems(Startup, TypstTemplateServer::insert_to_world)
            .add_systems(Update, TypstTemplateServer::do_jobs);
    }
}

#[derive(Debug)]
pub struct TypstJob {
    pub use_template: Handle<TypstZip>,
    pub data_in: Dict,
    pub send_target: async_channel::Sender<bevy_image::Image>,
    pub job_options: TypstJobOptions,
    _handle: Handle<Image>,
}

#[derive(Debug, Clone)]
pub struct TypstJobOptions {
    pub pixels_per_pt: f32,
    pub target_dimensions: Option<(u32, u32)>,
    pub asset_usage: RenderAssetUsages,
}

impl Default for TypstJobOptions {
    fn default() -> Self {
        Self {
            pixels_per_pt: 1.0,
            target_dimensions: None,
            asset_usage: RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
        }
    }
}

#[derive(Debug, Resource)]
pub struct TypstTemplateServer {
    asset_server: AssetServer,
    pub fallback: Image,
    pub templates: HashMap<PathBuf, Handle<TypstZip>>,
    pub jobs: VecDeque<TypstJob>,
    pub jobs_per_frame: Option<u32>,
}

impl TypstTemplateServer {
    pub(crate) fn insert_to_world(mut commands: Commands, asset_server: Res<AssetServer>) {
        commands.insert_resource(Self::new(asset_server.clone()));
    }

    pub fn do_jobs(
        mut template_server: ResMut<TypstTemplateServer>,
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
                let compiled = engine.compile_with_input::<_, PagedDocument>(job.data_in);
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
                let rendered = typst_render::render(&page.pages[0], job.job_options.pixels_per_pt);
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
                                RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
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

    pub fn new_with_fallback(asset_server: AssetServer, fallback: Image) -> Self {
        Self {
            asset_server,
            fallback,
            templates: HashMap::new(),
            jobs: VecDeque::new(),
            jobs_per_frame: Some(2),
        }
    }

    pub fn add_job(
        &mut self,
        zip: PathBuf,
        data_in: impl Serialize,
        options: TypstJobOptions,
    ) -> Handle<Image> {
        let asset_server = self.asset_server.clone();
        let template = self
            .templates
            .entry(zip.clone())
            .or_insert_with(|| asset_server.load(zip));
        let (sender, receiver) = async_channel::unbounded::<bevy_image::Image>();
        let handle: Handle<Image> = self.asset_server.add_async(async move {
            let res = receiver.recv().await;
            if let Err(res) = &res {
                bevy_log::error!("[TYPST ASYNC JOB ERROR] {res}")
            }
            res
        });
        let Ok(data_in): Result<serde_json::Value, _> = data_in.serialize(Serializer) else {
            bevy_log::error!(
                "[TYPST INPUT ERROR] Could not transform value into a serde json as interim for Dict."
            );
            return handle;
        };
        let Ok(data_in) = serde_json::from_value(data_in) else {
            bevy_log::error!("[TYPST INPUT ERROR] Could not get Dict from interim serde json.");
            return handle;
        };
        self.jobs.push_back(TypstJob {
            use_template: template.clone(),
            data_in,
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
