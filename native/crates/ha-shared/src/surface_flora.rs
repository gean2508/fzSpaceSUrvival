//! Decorative flora prop — spawned per chunk by `surface_chunk`.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FloraProp {
    pub tangent_x: f32,
    pub tangent_z: f32,
    pub kind: &'static str,
    pub scale: f32,
    pub yaw: f32,
}
