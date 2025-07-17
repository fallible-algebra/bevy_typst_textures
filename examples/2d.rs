use bevy::prelude::*;
use bevy_typst_textures::{TypstJobOptions, TypstTextureServer, TypstTexturesPlugin};

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, TypstTexturesPlugin::default()))
        .add_systems(Startup, start)
        .run();
}

fn start(mut commands: Commands, mut typst_server: ResMut<TypstTextureServer>) {
    commands.spawn(Camera2d);
    commands.spawn(Sprite {
        image: typst_server.add_job("example.zip".into(), TypstJobOptions::default()),
        ..default()
    });
}
