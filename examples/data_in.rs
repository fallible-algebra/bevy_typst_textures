use bevy::prelude::*;
use bevy_typst_textures::{TypstJobOptions, TypstTemplateServer, TypstTexturesPlugin};
use serde::{Deserialize, Serialize};

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, TypstTexturesPlugin))
        .add_systems(Startup, start)
        .run();
}

fn start(mut commands: Commands, mut typst_server: ResMut<TypstTemplateServer>) {
    commands.spawn(Camera2d);
    commands.spawn(Sprite {
        image: typst_server.add_job_with_data_in(
            "example.zip".into(),
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
