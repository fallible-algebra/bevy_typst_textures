use bevy::{math::VectorSpace, prelude::*};
use bevy_typst_textures::{TypstJobOptions, TypstTemplateServer, TypstTexturesPlugin};

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, TypstTexturesPlugin))
        .add_systems(Startup, start)
        .run();
}

fn start(
    mut commands: Commands,
    mut typst_server: ResMut<TypstTemplateServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    commands.spawn((
        Camera3d { ..default() },
        Transform::from_xyz(5., 4., 2.5).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(1., 1., 1.))),
        MeshMaterial3d(materials.add(StandardMaterial {
            // "add_job"/"add_job_with_data_in" gets called
            base_color_texture: Some(
                typst_server.add_job("example.zip".into(), TypstJobOptions::default()),
            ),
            ..default()
        })),
    ));
    commands.insert_resource(AmbientLight {
        brightness: 500.,
        ..default()
    });
}
