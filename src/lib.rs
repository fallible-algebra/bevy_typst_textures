use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
};

use bevy_app::{First, Plugin, Startup};
use bevy_asset::{AssetServer, Assets, Handle, RenderAssetUsages};
use bevy_ecs::{
    resource::Resource,
    system::{Res, ResMut},
};
use bevy_image::Image;
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
            .add_systems(First, TypstTemplateServer::do_jobs);
    }
}

#[derive(Debug)]
pub struct TypstJob {
    pub use_template: Handle<TypstZip>,
    pub data_in: Dict,
    pub handle_target: Handle<bevy_image::Image>,
    pub target_dims: Option<(u32, u32)>,
    pub scale: f32,
}

#[derive(Debug, Clone)]
pub struct TypstJobOption {
    pub pixels_per_pt: f32,
    pub target_dimensions: Option<(u32, u32)>,
    pub asset_usage: RenderAssetUsages,
}

#[derive(Debug, Resource)]
pub struct TypstTemplateServer {
    asset_server: AssetServer,
    pub fallback: Image,
    pub templates: HashMap<PathBuf, Handle<TypstZip>>,
    jobs: VecDeque<TypstJob>,
    jobs_per_frame: Option<u32>,
}

impl TypstTemplateServer {
    pub(crate) fn insert_to_world() {}

    pub fn do_jobs(
        mut template_server: ResMut<TypstTemplateServer>,
        templates: Res<Assets<TypstZip>>,
        mut images: ResMut<Assets<Image>>,
    ) {
        let max_jobs = template_server
            .jobs_per_frame
            .unwrap_or(template_server.jobs.len() as u32);
        let mut jobs_done = 0;
        let mut compiled_map = HashMap::new();
        while let Some(job) = template_server.jobs.pop_front()
            && jobs_done < max_jobs
        {
            if template_server.asset_server.is_loaded(&job.use_template)
                && let Some(template) = templates.get(&job.use_template)
            {
                let (engine, toml) = compiled_map
                    .entry(job.use_template)
                    .or_insert_with(|| template.0.clone().to_engine());
                let compiled = engine.compile_with_input::<_, PagedDocument>(job.data_in);
                let Ok(page) = compiled.output else {
                    bevy_log::error!(
                        "[TYPST FATAL ERROR for {:?}] {}",
                        job.handle_target.path(),
                        compiled.output.unwrap_err()
                    );
                    continue;
                };
                for warning in compiled.warnings {
                    if warning.severity == Severity::Error {
                        bevy_log::error!(
                            "[TYPST ERROR for {:?}] {}",
                            job.handle_target.path(),
                            warning.message
                        );
                    } else {
                        bevy_log::warn!(
                            "[TYPST WARNING for {:?}] {}",
                            job.handle_target.path(),
                            warning.message
                        );
                    }
                }
                let rendered = typst_render::render(&page.pages[0], job.scale);
                images.insert(
                    &job.handle_target,
                    bevy_image::Image::new(
                        Extent3d {
                            width: rendered.width(),
                            height: rendered.height(),
                            depth_or_array_layers: 1,
                        },
                        TextureDimension::D2,
                        rendered.data().to_vec(),
                        TextureFormat::Rgba8UnormSrgb,
                        RenderAssetUsages::RENDER_WORLD,
                    ),
                );
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
                RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
            ),
        )
    }

    pub fn new_with_fallback(asset_server: AssetServer, fallback: Image) -> Self {
        Self {
            asset_server,
            fallback,
            templates: HashMap::new(),
            jobs: VecDeque::new(),
            jobs_per_frame: None,
        }
    }

    pub fn add_job(
        &mut self,
        zip: PathBuf,
        data_in: Dict,
        target_dims: Option<(u32, u32)>,
        scale: f32,
    ) -> Handle<Image> {
        let asset_server = self.asset_server.clone();
        let template = self
            .templates
            .entry(zip.clone())
            .or_insert_with(|| asset_server.load(zip));
        let image = self.fallback.clone();
        let handle = self.asset_server.add(image);
        self.jobs.push_back(TypstJob {
            use_template: template.clone(),
            data_in,
            handle_target: handle.clone(),
            target_dims,
            scale,
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
