use bevy::prelude::*;
use bevy_typst_textures::{TypstJobOptions, TypstTextureServer, TypstTexturesPlugin};
use serde::Serialize;

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, TypstTexturesPlugin::default()))
        .add_systems(Startup, start)
        .run();
}

fn start(mut commands: Commands, mut typst_server: ResMut<TypstTextureServer>) {
    commands.spawn(Camera2d);
    commands.spawn(Sprite {
        image: typst_server.add_job_with_serde_input(
            "example.zip",
            DataIn {
                text: "This was passed to Typst as extra data.".into(),
            },
            TypstJobOptions::default(),
        ),
        ..default()
    });
}

#[derive(Debug, Serialize)]
struct DataIn {
    text: String,
}
