use bevy::prelude::*;
use bevy_jfa::{Outline, OutlinePlugin};

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands
        .spawn_bundle(PbrBundle {
            mesh: meshes.add(Mesh::from(shape::Cube { size: 1.0 })),
            material: materials.add(StandardMaterial {
                base_color: Color::INDIGO,
                perceptual_roughness: 0.25,
                metallic: 0.5,
                ..Default::default()
            }),
            ..Default::default()
        })
        .insert(Outline {
            enabled: true,
            width: 16,
            color: Color::AZURE,
        });

    commands.spawn_bundle(PerspectiveCameraBundle {
        transform: Transform::from_xyz(3.0, 2.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
        ..PerspectiveCameraBundle::new_3d()
    });

    commands.spawn_bundle(PointLightBundle {
        point_light: PointLight {
            color: Color::WHITE,
            intensity: 800.0,
            range: 20.0,
            radius: 0.0,
            ..Default::default()
        },
        transform: Transform::from_xyz(6.0, 3.0, 1.0),
        ..Default::default()
    });
}

fn rotate_cube(time: Res<Time>, mut query: Query<&mut Transform, With<Outline>>) {
    let delta = time.delta_seconds();

    for mut xform in query.iter_mut() {
        xform.rotate(Quat::from_rotation_y(delta));
    }
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugin(OutlinePlugin)
        .add_startup_system(setup)
        .add_system(rotate_cube)
        .run();
}
