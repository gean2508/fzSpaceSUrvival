//! SI scale — 1 world unit = 1 metre on planetary surfaces.

/// Average adult height (m).
pub const HUMAN_HEIGHT_M: f32 = 1.75;
pub const HUMAN_EYE_HEIGHT_M: f32 = 1.63;
pub const PLAYER_COLLISION_RADIUS_M: f32 = 0.35;

pub const EARTH_RADIUS_M: f32 = 6_371_000.0;
pub const MOON_RADIUS_M: f32 = 1_737_400.0;
pub const EARTH_MOON_DISTANCE_M: f32 = 384_400_000.0;
pub const LEO_ALTITUDE_M: f32 = 408_000.0;

pub const MOON_TO_EARTH_RADIUS_RATIO: f32 = MOON_RADIUS_M / EARTH_RADIUS_M;

/// Hub orbital view — compressed visual metres (ratios stay astronomical).
pub const HUB_EARTH_VISUAL_RADIUS_M: f32 = 22.0;
pub const HUB_EARTH_VISUAL_DISTANCE_M: f32 = 55.0;
pub const HUB_MOON_VISUAL_RADIUS_M: f32 = HUB_EARTH_VISUAL_RADIUS_M * MOON_TO_EARTH_RADIUS_RATIO;

/// From Luna surface — Earth in the sky (arcade readable size, not angular-accurate).
pub const LUNA_SKY_EARTH_DISTANCE_M: f32 = 180.0;
pub const LUNA_SKY_EARTH_RADIUS_M: f32 = 14.0;
/// Walkable Earth shell at origin — arcade metres (SFS-style home body).
// CONTINUIDAD (fase 1): un SOLO radio para la Tierra — caminable == orbital.
// Antes 480 (caminable) vs 8000 (orbital) → despegar teletransportaba entre dos
// marcos/escala. Ahora la Tierra es un cuerpo normal de radio 1500: mismo radio
// caminable y orbital, centro único, remap identidad (ver `planet_surface.rs`).
/// ESCALA GTA-V (29 jul 2026): 30 km de radio ⇒ 60 km de punta a punta. A la
/// altura de crucero cómoda (~1.4 km, donde todavía distinguís el terreno) son
/// unos **10 minutos de vuelo** para cruzarlo — el orden de magnitud que pidió
/// el usuario, y el que hace que ver el planeta entero desde el espacio impresione.
/// Antes eran 1500 m: cruzabas el mundo en 20 segundos y el "planeta" era un
/// pueblo. Todos los cuerpos escalaron ×20 con él (ver `celestial_bodies`).
pub const EARTH_GAMEPLAY_RADIUS_M: f32 = 30_000.0;
/// Walkable moon shell — arcade metres (mantiene la proporción real con la Tierra).
pub const MOON_GAMEPLAY_RADIUS_M: f32 = 8_400.0;

/// Visual-only radius for orbital / hub spheres (unchanged ratios).
pub const MOON_SURFACE_RADIUS_M: f32 = MOON_GAMEPLAY_RADIUS_M;

/// Star dome radius in orbital void (metres).
pub const SPACE_STARFIELD_RADIUS_M: f32 = 128.0;

/// Moon visible in LEO sky — compressed layout, ratios astronomical.
pub const SPACE_MOON_SKY_X_M: f32 = 42.0;
pub const SPACE_MOON_SKY_Y_M: f32 = 14.0;
pub const SPACE_MOON_SKY_Z_M: f32 = -36.0;
