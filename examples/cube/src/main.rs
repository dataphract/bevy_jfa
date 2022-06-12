use bevy::{
    input::{keyboard::KeyboardInput, ButtonState},
    prelude::*,
};
use bevy_jfa::{CameraOutline, Outline, OutlinePlugin, OutlineSettings, OutlineStyle};

#[derive(Clone, Debug, Component)]
struct RotationAxis(Vec3);

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut outline_styles: ResMut<Assets<OutlineStyle>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mesh = meshes.add(Mesh::from(shape::Cube { size: 1.0 }));
    let material = materials.add(StandardMaterial {
        base_color: Color::INDIGO,
        perceptual_roughness: 0.25,
        metallic: 0.5,
        ..Default::default()
    });

    commands
        .spawn_bundle(PbrBundle {
            mesh: mesh.clone(),
            material: material.clone(),
            transform: Transform::from_xyz(0.0, 0.0, 0.0),
            ..Default::default()
        })
        .insert(RotationAxis(Vec3::Y))
        .insert(Outline { enabled: true });

    commands
        .spawn_bundle(PbrBundle {
            mesh: mesh.clone(),
            material: material.clone(),
            transform: Transform::from_xyz(-2.0, 0.0, 0.0),
            ..Default::default()
        })
        .insert(RotationAxis(Vec3::X))
        .insert(Outline { enabled: true });

    commands
        .spawn_bundle(PbrBundle {
            mesh,
            material,
            transform: Transform::from_xyz(0.0, 0.0, -2.0),
            ..Default::default()
        })
        .insert(RotationAxis(Vec3::Z))
        .insert(Outline { enabled: true });

    commands
        .spawn_bundle(Camera3dBundle {
            transform: Transform::from_xyz(3.0, 2.0, 3.0)
                .looking_at([-1.0, -0.5, -1.0].into(), Vec3::Y),
            ..Camera3dBundle::default()
        })
        .insert(CameraOutline {
            enabled: true,
            style: outline_styles.add(OutlineStyle {
                color: Color::hex("b4a2c8").unwrap(),
                width: 33.0,
            }),
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

fn rotate_cube(time: Res<Time>, mut query: Query<(&mut Transform, &RotationAxis), With<Outline>>) {
    let delta = time.delta_seconds();

    for (mut xform, rot) in query.iter_mut() {
        xform.rotate(Quat::from_axis_angle(rot.0, delta));
    }
}

fn handle_keys(mut settings: ResMut<OutlineSettings>, mut keys: EventReader<KeyboardInput>) {
    for ev in keys.iter() {
        if ev.key_code == Some(KeyCode::R) && ev.state == ButtonState::Pressed {
            let old = settings.half_resolution();
            settings.set_half_resolution(!old);
        }
    }
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugin(OutlinePlugin)
        .add_startup_system(setup)
        .add_system(rotate_cube)
        .add_system(handle_keys)
        .run();
}
