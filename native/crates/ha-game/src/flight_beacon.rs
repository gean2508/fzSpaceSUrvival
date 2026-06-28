//! Visible markers at hold-interact points (launch pad, orbital hub, Luna).

use bevy::prelude::*;

#[derive(Component)]
pub struct InteractBeacon;

pub fn spawn_interact_beacon(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    std_mats: &mut Assets<StandardMaterial>,
    position: Vec3,
    color: LinearRgba,
    height: f32,
    tag: impl Bundle,
) {
    let mat = std_mats.add(StandardMaterial {
        base_color: Color::linear_rgba(color.red, color.green, color.blue, color.alpha),
        unlit: true,
        emissive: LinearRgba::new(
            (color.red * 0.65).min(1.0),
            (color.green * 0.65).min(1.0),
            (color.blue * 0.65).min(1.0),
            1.0,
        ),
        ..default()
    });
    commands.spawn((
        Mesh3d(meshes.add(Cylinder::new(0.14, height))),
        MeshMaterial3d(mat),
        Transform::from_xyz(position.x, position.y + height * 0.5, position.z),
        InteractBeacon,
        tag,
    ));
}
