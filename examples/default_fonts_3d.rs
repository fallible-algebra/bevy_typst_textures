use bevy::prelude::*;
use bevy_typst_textures::{TypstJobOptions, TypstTextureServer, TypstTexturesPlugin};

#[cfg(feature = "typst-asset-fonts")]
fn main() {
    App::new()
        .add_plugins((DefaultPlugins, TypstTexturesPlugin::default()))
        .add_systems(Startup, start)
        .run();
}

#[cfg(not(feature = "typst-asset-fonts"))]
fn main() {
    eprintln!("You need to enable the 'typst-asset-fonts' feature!");
}

fn start(
    mut commands: Commands,
    mut typst_server: ResMut<TypstTextureServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    commands.spawn((
        Camera3d { ..default() },
        Transform::from_xyz(2., 4., 2.5).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(1., 1., 1.))),
        MeshMaterial3d(materials.add(StandardMaterial {
            // "add_job"/"add_job_with_input" gives a `Handle<Image>`
            base_color_texture: Some(
                typst_server.add_job("standalone.typ", TypstJobOptions::default()),
            ),
            ..default()
        })),
    ));
    commands.insert_resource(AmbientLight {
        brightness: 2000.,
        ..default()
    });
}
