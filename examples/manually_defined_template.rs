use bevy::prelude::*;
use bevy_typst_textures::{
    TypstJobOptions, TypstTextureServer, TypstTexturesPlugin,
    file_resolver::StructuredInMemoryTemplate,
};

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, TypstTexturesPlugin::default()))
        .add_systems(Startup, start)
        .run();
}

fn start(mut commands: Commands, mut typst_server: ResMut<TypstTextureServer>) {
    commands.spawn(Camera2d);
    commands.spawn(Sprite {
        image: typst_server.add_job(
            StructuredInMemoryTemplate {
                loaded_toml: Default::default(),
                loaded_fonts: Default::default(),
                loaded_main: MAIN_DOT_TYP.to_string(),
                path_given: "manually_defined".into(),
                file_resolver: Default::default(),
                source_resolver: Default::default(),
            },
            TypstJobOptions::default(),
        ),
        ..default()
    });
}

const MAIN_DOT_TYP: &str = r#"
#set page(
  width: 256pt,
  height: 256pt,
  fill: none,
  margin: 0pt,
)

#import sys : inputs

#set text(fill: white, font: "Atkinson Hyperlegible Next", size: 25pt)

#rect(fill: gradient.conic(..color.map.rainbow), width: 100%, height: 100%)

#place(center + horizon)[
  Hello from a manually, in-code defined Typst project :)
  #inputs.at("text", default: "")
]
"#;
