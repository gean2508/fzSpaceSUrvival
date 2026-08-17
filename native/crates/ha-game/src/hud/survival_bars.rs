//! **Las barritas de estado, cada una con su iconito.**
//!
//! Hambre, sed, energía, oxígeno y Ki. El oxígeno y el Ki sólo aparecen cuando
//! importan (vacío, o vuelo/carga).
//!
//! ## Por qué cada barra lleva dibujo
//!
//! Hasta el 15 ago 2026 las barras se distinguían **sólo por el color**: ámbar
//! era hambre, azul era sed. Nadie que abre el juego por primera vez sabe eso, y
//! en el Redmi al sol los cuatro tonos se parecen bastante. Ahora cada una lleva
//! su glifo a la izquierda —pata, gota, rayo, burbuja, chispa— y el color pasa a
//! ser refuerzo en vez de ser la única pista.
//!
//! ## Y por qué la fila de muslos se fue
//!
//! La energía se mostraba arriba a la derecha como seis **muslos de pollo**
//! (`drumstick_full`), al lado de una barra de hambre que ya existía: el jugador
//! veía comida dos veces y ninguna de las dos era la que le importaba. La fila
//! entera se borró y la energía bajó acá, como una barra más con su rayo. No se
//! perdió nada: el dato es el mismo `body.stamina` que alimentaba los muslos.
//!
//! Los glifos se rasterizan por código y no salen del pack. Son de 16 px, se ven
//! a un tamaño fijo y no tienen variantes: un PNG por cada uno serían cinco
//! texturas residentes para 1.280 píxeles de dibujo.

use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use ha_shared::{HUNGER_MAX, OXYGEN_MAX, THIRST_MAX};

use crate::sim::{Player, SimState};
use crate::ui_theme;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SurvivalBarKind {
    Hunger,
    Thirst,
    /// Aguante para correr y trabajar (`body.stamina`).
    Energy,
    Oxygen,
    /// Ki reserve — only shown while flying/charging (Saiyan flight).
    Ki,
}

/// El orden en pantalla, de arriba abajo. Los dos condicionales van al final
/// para que aparecer y desaparecer no empuje a los que siempre están.
const ORDEN: [SurvivalBarKind; 5] = [
    SurvivalBarKind::Hunger,
    SurvivalBarKind::Thirst,
    SurvivalBarKind::Energy,
    SurvivalBarKind::Oxygen,
    SurvivalBarKind::Ki,
];

/// La fila entera (icono + carril). Es la que se muestra y se esconde.
#[derive(Component)]
pub struct SurvivalBarRow(pub SurvivalBarKind);

#[derive(Component)]
pub struct SurvivalBarFill(pub SurvivalBarKind);

#[derive(Resource, Default)]
pub struct SurvivalBarsCache {
    hunger_pct: f32,
    thirst_pct: f32,
    energy_pct: f32,
    oxygen_pct: f32,
    ki_pct: f32,
    oxygen_visible: bool,
    ki_visible: bool,
    initialized: bool,
}

fn bar_color(kind: SurvivalBarKind) -> Color {
    match kind {
        SurvivalBarKind::Hunger => Color::srgb(0.94, 0.63, 0.25), // amber — food
        SurvivalBarKind::Thirst => Color::srgb(0.30, 0.65, 0.95), // water blue
        SurvivalBarKind::Energy => Color::srgb(0.98, 0.85, 0.30), // stamina yellow
        SurvivalBarKind::Oxygen => Color::srgb(0.78, 0.93, 0.98), // suit white-cyan
        SurvivalBarKind::Ki => Color::srgb(1.0, 0.82, 0.22),      // Saiyan gold
    }
}

// ---------------------------------------------------------------------------
// Los glifos
// ---------------------------------------------------------------------------

/// Los cinco iconos ya rasterizados. Se arman una vez al nacer el HUD.
#[derive(Clone)]
pub struct BarIcons {
    hunger: Handle<Image>,
    thirst: Handle<Image>,
    energy: Handle<Image>,
    oxygen: Handle<Image>,
    ki: Handle<Image>,
}

impl BarIcons {
    pub fn nuevos(images: &mut Assets<Image>) -> Self {
        Self {
            hunger: images.add(glifo(SurvivalBarKind::Hunger)),
            thirst: images.add(glifo(SurvivalBarKind::Thirst)),
            energy: images.add(glifo(SurvivalBarKind::Energy)),
            oxygen: images.add(glifo(SurvivalBarKind::Oxygen)),
            ki: images.add(glifo(SurvivalBarKind::Ki)),
        }
    }

    fn handle(&self, kind: SurvivalBarKind) -> Handle<Image> {
        match kind {
            SurvivalBarKind::Hunger => self.hunger.clone(),
            SurvivalBarKind::Thirst => self.thirst.clone(),
            SurvivalBarKind::Energy => self.energy.clone(),
            SurvivalBarKind::Oxygen => self.oxygen.clone(),
            SurvivalBarKind::Ki => self.ki.clone(),
        }
    }
}

const GLIFO: u32 = 16;

fn glifo(kind: SurvivalBarKind) -> Image {
    let mut data = vec![0u8; (GLIFO * GLIFO * 4) as usize];
    let c = color_bytes(bar_color(kind));
    let sombra = [0, 0, 0, 150];
    match kind {
        SurvivalBarKind::Hunger => pata(&mut data, c, sombra),
        SurvivalBarKind::Thirst => gota(&mut data, c),
        SurvivalBarKind::Energy => rayo(&mut data, c),
        SurvivalBarKind::Oxygen => burbuja(&mut data, c),
        SurvivalBarKind::Ki => chispa(&mut data, c),
    }
    Image::new(
        Extent3d { width: GLIFO, height: GLIFO, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

fn color_bytes(c: Color) -> [u8; 4] {
    let s = c.to_srgba();
    [
        (s.red * 255.0) as u8,
        (s.green * 255.0) as u8,
        (s.blue * 255.0) as u8,
        255,
    ]
}

fn px(data: &mut [u8], x: i32, y: i32, c: [u8; 4]) {
    if x < 0 || y < 0 || x >= GLIFO as i32 || y >= GLIFO as i32 {
        return;
    }
    let i = ((y as u32 * GLIFO + x as u32) * 4) as usize;
    data[i..i + 4].copy_from_slice(&c);
}

/// Una fila horizontal de `x0..x1` inclusive.
fn fila(data: &mut [u8], y: i32, x0: i32, x1: i32, c: [u8; 4]) {
    for x in x0..=x1 {
        px(data, x, y, c);
    }
}

/// **Pata de pollo** = hambre. Bola de carne arriba, hueso abajo a la izquierda.
fn pata(data: &mut [u8], carne: [u8; 4], hueso_sombra: [u8; 4]) {
    let hueso = [235, 226, 205, 255];
    // La bola: un círculo tosco de radio 4 centrado arriba a la derecha.
    for y in 1..10 {
        for x in 4..14 {
            let dx = x as f32 - 9.0;
            let dy = y as f32 - 5.0;
            if dx * dx + dy * dy <= 17.0 {
                px(data, x, y, carne);
            }
        }
    }
    // Brillo: la esquina superior izquierda de la bola, un tono más claro.
    for (x, y) in [(7, 3), (8, 2), (6, 4)] {
        px(data, x, y, aclarar(carne));
    }
    // El hueso sale en diagonal hacia abajo-izquierda.
    for i in 0..5 {
        px(data, 7 - i, 8 + i, hueso);
        px(data, 8 - i, 8 + i, hueso);
    }
    // Los dos nudillos de la punta.
    px(data, 1, 13, hueso);
    px(data, 3, 13, hueso);
    px(data, 2, 14, hueso);
    // Sombra de contacto bajo el hueso, para que no flote.
    px(data, 2, 15, hueso_sombra);
}

/// **Gota** = sed. Punta arriba, panza abajo, y un brillo fijo a la izquierda.
/// Es el dibujo que el plan pedía y que no existía en ningún lado.
fn gota(data: &mut [u8], agua: [u8; 4]) {
    // Punta: una cuña que se abre.
    for (i, y) in (1..7).enumerate() {
        let medio = 8;
        let ancho = i as i32 / 2;
        fila(data, y, medio - ancho, medio + ancho, agua);
    }
    // Panza: círculo de radio 4,5 centrado en (8, 10).
    for y in 6..15 {
        for x in 3..14 {
            let dx = x as f32 - 8.0;
            let dy = y as f32 - 10.0;
            if dx * dx + dy * dy <= 20.0 {
                px(data, x, y, agua);
            }
        }
    }
    // El brillo: dos píxeles claros donde pegaría la luz.
    px(data, 6, 9, aclarar(agua));
    px(data, 6, 10, aclarar(agua));
    px(data, 5, 10, aclarar(agua));
}

/// **Rayo** = energía. Zigzag clásico, con el quiebre en el medio.
fn rayo(data: &mut [u8], luz: [u8; 4]) {
    // Mitad de arriba: baja hacia la izquierda.
    for (i, y) in (1..8).enumerate() {
        let x0 = 9 - i as i32;
        fila(data, y, x0, x0 + 2, luz);
    }
    // La barra del quiebre.
    fila(data, 7, 3, 11, luz);
    // Mitad de abajo: sigue bajando, corrida a la derecha.
    for (i, y) in (8..15).enumerate() {
        let x0 = 9 - (6 - i as i32).max(0);
        fila(data, y, x0.min(9), (x0 + 2).min(11), luz);
    }
}

/// **Burbuja** = oxígeno. Anillo hueco: llena se confundía con la gota.
fn burbuja(data: &mut [u8], aire: [u8; 4]) {
    for y in 1..15 {
        for x in 1..15 {
            let dx = x as f32 - 7.5;
            let dy = y as f32 - 7.5;
            let d = dx * dx + dy * dy;
            if (20.0..=38.0).contains(&d) {
                px(data, x, y, aire);
            }
        }
    }
    // El reflejo de arriba a la izquierda, que es lo que la hace burbuja.
    px(data, 5, 4, aclarar(aire));
    px(data, 4, 5, aclarar(aire));
}

/// **Chispa** = Ki. Cuatro puntas, la vertical más larga.
fn chispa(data: &mut [u8], ki: [u8; 4]) {
    for y in 0i32..16 {
        let ancho = if (6..=9).contains(&y) { 2 } else { (8 - (y - 8).abs()) / 4 };
        fila(data, y, 8 - ancho, 7 + ancho, ki);
    }
    for x in 2i32..14 {
        let alto = if (6..=9).contains(&x) { 2 } else { (8 - (x - 8).abs()) / 4 };
        for y in (8 - alto)..(8 + alto) {
            px(data, x, y, ki);
        }
    }
    px(data, 7, 7, aclarar(ki));
    px(data, 8, 7, aclarar(ki));
}

/// Sube el brillo sin cambiar el tono: el acento del glifo sale del mismo color
/// de la barra, así no hay una segunda paleta que mantener.
fn aclarar(c: [u8; 4]) -> [u8; 4] {
    [
        c[0].saturating_add(60),
        c[1].saturating_add(60),
        c[2].saturating_add(60),
        c[3],
    ]
}

// ---------------------------------------------------------------------------
// La UI
// ---------------------------------------------------------------------------

/// Spawned inside the HudTopLeft column, right below the heart row.
/// **Las barras, con las medidas de la imagen de referencia.**
///
/// Antes las sacaba de proporciones propias con topes (`clamp(10, 16)` para el
/// icono, `clamp(4, 7)` para el alto) que venían de un HUD más chico: con las
/// medidas del mockup —icono de 29, carril de 428 × 18— el tope las dejaba en
/// una rayita. Ahora las recibe ya calculadas de quien arma el HUD.
pub fn spawn_survival_bars(
    parent: &mut ChildBuilder,
    total: f32,
    icono: f32,
    height: f32,
    gap: f32,
    iconos: &BarIcons,
) {
    let carril = total - icono - gap;
    for kind in ORDEN {
        parent
            .spawn((
                Node {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(gap),
                    ..default()
                },
                SurvivalBarRow(kind),
                Name::new(match kind {
                    SurvivalBarKind::Hunger => "HungerBar",
                    SurvivalBarKind::Thirst => "ThirstBar",
                    SurvivalBarKind::Energy => "EnergyBar",
                    SurvivalBarKind::Oxygen => "OxygenBar",
                    SurvivalBarKind::Ki => "KiBar",
                }),
            ))
            .with_children(|row| {
                row.spawn((
                    Node {
                        width: Val::Px(icono),
                        height: Val::Px(icono),
                        flex_shrink: 0.0,
                        ..default()
                    },
                    ImageNode::new(iconos.handle(kind)),
                    Name::new("BarIcon"),
                ));
                row.spawn((
                    Node {
                        width: Val::Px(carril.max(24.0)),
                        height: Val::Px(height),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BorderColor(ui_theme::accent_ink().with_alpha(0.65)),
                    BackgroundColor(ui_theme::panel_bg(0.72)),
                ))
                .with_children(|track| {
                    track.spawn((
                        Node {
                            width: Val::Percent(100.0),
                            height: Val::Percent(100.0),
                            ..default()
                        },
                        BackgroundColor(bar_color(kind)),
                        SurvivalBarFill(kind),
                    ));
                });
            });
    }
}

pub fn update_survival_bars(
    player: Query<&SimState, With<Player>>,
    world_ctx: Res<crate::scene::WorldContextRuntime>,
    flight: Res<crate::flight::Flight>,
    mut cache: ResMut<SurvivalBarsCache>,
    mut fills: Query<(&SurvivalBarFill, &mut Node), Without<SurvivalBarRow>>,
    mut rows: Query<(&SurvivalBarRow, &mut Node), Without<SurvivalBarFill>>,
) {
    let Ok(sim) = player.get_single() else {
        return;
    };
    let hunger_pct = (sim.survival.hunger / HUNGER_MAX * 100.0).clamp(0.0, 100.0);
    let thirst_pct = (sim.survival.thirst / THIRST_MAX * 100.0).clamp(0.0, 100.0);
    let oxygen_pct = (sim.survival.oxygen / OXYGEN_MAX * 100.0).clamp(0.0, 100.0);
    // El mismo dato que alimentaba la fila de muslos.
    let energy_pct = if sim.body.max_stamina > 0.0 {
        (sim.body.stamina / sim.body.max_stamina * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    };
    let ki_pct = flight.ki_pct();
    // O₂ row earns its pixels only when the suit is in play.
    let oxygen_visible = !matches!(
        world_ctx.active,
        ha_shared::WorldContext::Overworld | ha_shared::WorldContext::Mission
    ) || oxygen_pct < 99.5;
    // Ki row shows while flying/charging, or when not fully recovered.
    let ki_visible = flight.active || flight.charging || ki_pct < 99.5;

    let changed = !cache.initialized
        || (cache.hunger_pct - hunger_pct).abs() > 0.4
        || (cache.thirst_pct - thirst_pct).abs() > 0.4
        || (cache.energy_pct - energy_pct).abs() > 0.4
        || (cache.oxygen_pct - oxygen_pct).abs() > 0.4
        || (cache.ki_pct - ki_pct).abs() > 0.4
        || cache.oxygen_visible != oxygen_visible
        || cache.ki_visible != ki_visible;
    if !changed {
        return;
    }
    cache.initialized = true;
    cache.hunger_pct = hunger_pct;
    cache.thirst_pct = thirst_pct;
    cache.energy_pct = energy_pct;
    cache.oxygen_pct = oxygen_pct;
    cache.ki_pct = ki_pct;
    cache.oxygen_visible = oxygen_visible;
    cache.ki_visible = ki_visible;

    for (fill, mut node) in &mut fills {
        node.width = Val::Percent(match fill.0 {
            SurvivalBarKind::Hunger => hunger_pct,
            SurvivalBarKind::Thirst => thirst_pct,
            SurvivalBarKind::Energy => energy_pct,
            SurvivalBarKind::Oxygen => oxygen_pct,
            SurvivalBarKind::Ki => ki_pct,
        });
    }
    for (row, mut node) in &mut rows {
        match row.0 {
            SurvivalBarKind::Oxygen => {
                node.display = if oxygen_visible { Display::Flex } else { Display::None };
            }
            SurvivalBarKind::Ki => {
                node.display = if ki_visible { Display::Flex } else { Display::None };
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **Ningún glifo puede salir vacío.** Un icono transparente en el HUD se ve
    /// exactamente igual que un icono que no se dibujó, y es el modo de falla
    /// que nadie mira: la barra sigue ahí y el hueco parece margen.
    #[test]
    fn los_cinco_glifos_dibujan_algo() {
        for kind in ORDEN {
            let img = glifo(kind);
            let opacos = img.data.chunks_exact(4).filter(|p| p[3] > 0).count();
            assert!(
                opacos > 20,
                "el glifo de la barra dibujó sólo {opacos} píxeles opacos"
            );
        }
    }

    /// **Ningún glifo se va contra un borde.** `px` descarta en silencio lo que
    /// cae fuera del cuadro, así que un dibujo mal ubicado no falla: se recorta
    /// y queda pegado a un lado. Se mide el centro de masa y no los píxeles del
    /// medio, porque la burbuja es un anillo hueco y por el medio no pasa nada:
    /// ese fue el primer test que escribí y reprobaba al dibujo correcto.
    #[test]
    fn ningun_glifo_queda_pegado_a_un_borde() {
        for (kind, nombre) in ORDEN.iter().zip(["pata", "gota", "rayo", "burbuja", "chispa"]) {
            let img = glifo(*kind);
            let (mut sx, mut sy, mut n) = (0.0f32, 0.0f32, 0.0f32);
            for (i, p) in img.data.chunks_exact(4).enumerate() {
                if p[3] > 0 {
                    sx += (i as u32 % GLIFO) as f32;
                    sy += (i as u32 / GLIFO) as f32;
                    n += 1.0;
                }
            }
            let (cx, cy) = (sx / n, sy / n);
            let medio = (GLIFO - 1) as f32 / 2.0;
            assert!(
                (cx - medio).abs() <= 2.5 && (cy - medio).abs() <= 2.5,
                "el glifo «{nombre}» tiene el centro de masa en ({cx:.1}, {cy:.1}), \
                 lejos del medio ({medio:.1})"
            );
        }
    }
}
