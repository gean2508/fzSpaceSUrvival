//! **Las manos agarran lo que llevan.** El jugador y los aldeanos sostienen su objeto (arma, herramienta, antorcha,
//! farol, comida…) con la mano que dibuja la hoja, no flotando al costado del cuerpo.
//!
//! Pedido del 3 oct 2026: «se ve horrible que el jugador dispare y su mano no se vea agarrando el arma».
//!
//! ## Cómo
//!
//! `scripts/medir-manos.py` MIDE, cuadro por cuadro, dónde está la mano que agarra (`art/chargen/manos.json`) y con
//! qué punto de su dibujo se agarra cada objeto, y deja una lámina por piel con SÓLO los píxeles de las manos
//! (`art/chargen/manos/NNN.png`, la misma grilla que la hoja). El objeto es un hijo del personaje, anclado por su
//! punto de agarre a la mano; encima va la lámina de manos recortada en el cuadro actual, con el mismo tinte: los
//! dedos tapan el mango y el objeto queda AGARRADO. Mirando hacia arriba las manos quedan detrás del cuerpo: el objeto
//! va detrás y no hay parche.
//!
//! Nada se escribe a ojo: el cuadro actual sale de la pose que ya eligió el sistema de animación
//! (`CurrentPose` / `NpcPose` + el índice del atlas) y la celda de `chargen_index::char_cell`.

use std::collections::HashMap;
use std::sync::OnceLock;

use bevy::prelude::*;
use bevy::sprite::Anchor;
use ha_assets::chargen_index::{char_anim, char_cell, CELL, STRIP_COLS, WALK_GUN_COL0};
use serde::Deserialize;

/// Metros por píxel de ARTE del objeto en la mano. Una celda de 32 px del pack son 0,95 m; el objeto se dibuja a
/// ~⅔ de eso (como el ítem en la mano de siempre, 0,62 m) para que un arma de 56 px mida ~1,1 m contra un
/// cuerpo de 1,9 m.
pub const M_POR_PX: f32 = 0.02;

const MANOS_JSON: &str = include_str!("../assets/art/chargen/manos.json");

#[derive(Deserialize)]
struct Mano {
    ancla: [f32; 2],
    #[serde(default)]
    detras: bool,
}

#[derive(Deserialize, Clone, Copy)]
pub struct Objeto {
    pub agarre: [f32; 2],
    pub tam: [f32; 2],
}

#[derive(Deserialize)]
struct Datos {
    filas: HashMap<String, u32>,
    manos: HashMap<String, Mano>,
    objetos: HashMap<String, Objeto>,
}

fn datos() -> Option<&'static Datos> {
    static D: OnceLock<Option<Datos>> = OnceLock::new();
    D.get_or_init(|| match serde_json::from_str(MANOS_JSON) {
        Ok(d) => Some(d),
        Err(e) => {
            eprintln!("manos: manos.json invalido ({e}) — los objetos quedan sin agarre");
            None
        }
    })
    .as_ref()
}

/// El agarre medido de un objeto (`items/<id>`), en px de su recorte.
///
/// Los camuflajes de un arma (`items/gun_ak__115`) tienen la forma y la empuñadura del arma base.
pub fn objeto(clave: &str) -> Option<Objeto> {
    let base = clave.split_once("__").map_or(clave, |(b, _)| b);
    datos()?.objetos.get(base).copied()
}

impl Objeto {
    /// Tamaño del dibujo en el mundo (m). Las tiras grandes (el farol: 3×3 celdas con su halo pintado) se
    /// achican a una celda, como siempre se llevaron en la mano.
    pub fn tam_en_mundo(&self) -> Vec2 {
        let t = Vec2::new(self.tam[0], self.tam[1]);
        let k = if t.max_element() > 64.0 { 32.0 / t.max_element() } else { 1.0 };
        t * k * M_POR_PX
    }
}

/// Dónde cae el cuadro `(pose, frame)` en la hoja y en la lámina de manos.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cuadro {
    pub fila_manos: u32,
    pub col: u32,
    pub ancla: Vec2,
    pub detras: bool,
}

/// El cuadro de manos de una pose del estado de render (`"walk_right#false"`) y su cuadro de animación.
/// `None` = esa pose no tiene mano medida (comer, juntar, nadar, sentado…): el objeto no se dibuja.
pub fn cuadro_de(estado: &str, frame: u32) -> Option<(Cuadro, bool)> {
    let d = datos()?;
    let (pose, flip) = estado.split_once('#').map(|(p, f)| (p, f == "true")).unwrap_or((estado, false));
    let (verbo, dir) = pose.rsplit_once('_')?;
    let dir = match dir {
        "right" => 0,
        "up" => 1,
        "left" => 2,
        "down" => 3,
        _ => return None,
    };
    let anim = char_anim(verbo)?;
    let col = char_cell(anim, dir, frame) % STRIP_COLS;
    // Caminar con el arma tiene las manos de apuntar (mismo torso).
    let (clave, col_manos) = if verbo == "walk_gun" { ("gun_idle", col - WALK_GUN_COL0) } else { (anim.key, col) };
    let fila_manos = *d.filas.get(clave)?;
    let m = d.manos.get(&format!("{clave}:{col_manos}"))?;
    Some((
        Cuadro { fila_manos, col: col_manos, ancla: Vec2::new(m.ancla[0], m.ancla[1]), detras: m.detras },
        flip,
    ))
}

/// Un punto de la celda del personaje (px, y hacia abajo) en metros locales al centro del sprite.
pub fn px_a_local(p: Vec2, tam: Vec2, flip: bool) -> Vec2 {
    let x = (p.x - CELL[0] as f32 / 2.0) / CELL[0] as f32 * tam.x;
    Vec2::new(if flip { -x } else { x }, (CELL[1] as f32 / 2.0 - p.y) / CELL[1] as f32 * tam.y)
}

/// Lo que sostiene una persona. Se pone en el jugador (el ítem equipado) o en un aldeano (su herramienta).
#[derive(Component, Default, Clone, PartialEq)]
pub struct Sostiene {
    pub item: String,
    /// Piel (índice de `art/chargen/manos/NNN.png`); `None` = sin lámina de manos (el objeto va sin dedos).
    pub piel: Option<u32>,
    /// El interruptor de la luz del objeto (la antorcha apagada no ilumina).
    pub luz: bool,
}

/// El objeto en la mano (hijo del portador). Lleva su ancla de agarre (fracción del sprite, y hacia arriba).
#[derive(Component)]
pub struct ObjetoEnMano(Vec2);

/// Los dedos por encima del objeto (hijo del portador).
#[derive(Component)]
pub struct DedosSobreObjeto;

/// Dónde está la mano del JUGADOR este cuadro, para quien dibuje algo girando alrededor de ella (el arma que apunta).
#[derive(Resource, Default, Clone, Copy)]
pub struct ManoDelJugador {
    pub vigente: bool,
    /// Posición de render (x, y) de la mano.
    pub pos: Vec2,
    pub detras: bool,
    pub z: f32,
}

/// Separación en z entre el cuerpo, el objeto y los dedos: menor que un escalón de Y-sort (`layers::YSORT_SPAN`).
pub const DZ: f32 = 1.0e-5;

pub struct ManosPlugin;

impl Plugin for ManosPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ManoDelJugador>().add_systems(
            Update,
            (
                armar_portadores,
                colocar_objetos.after(armar_portadores).after(crate::sim::face_player_sprite),
                calcular_mano_del_jugador.after(crate::sim::face_player_sprite),
            ),
        );
    }
}

/// Crea (o rehace) el objeto y los dedos de quien cambió lo que sostiene.
fn armar_portadores(
    mut commands: Commands,
    art: Res<crate::art2d::ArtLibrary>,
    assets: Res<AssetServer>,
    cambiados: Query<(Entity, &Sostiene, Option<&Children>), Changed<Sostiene>>,
    hijos: Query<(), Or<(With<ObjetoEnMano>, With<DedosSobreObjeto>)>>,
) {
    for (e, s, kids) in &cambiados {
        for k in kids.into_iter().flat_map(|k| k.iter()) {
            if hijos.get(*k).is_ok() {
                commands.entity(*k).despawn_recursive();
            }
        }
        if s.item.is_empty() {
            continue;
        }
        let clave = format!("items/{}", s.item);
        let Some(ag) = objeto(&clave) else { continue };
        // Las armas las dibuja `armas.rs` (gira hacia donde apuntás); acá sólo van los dedos.
        let es_arma = crate::armas::es_arma(&s.item);
        let tam = ag.tam_en_mundo();
        commands.entity(e).with_children(|p| {
            if !es_arma {
                if let Some((mut sprite, anim)) = art.world_sprite(&clave, tam) {
                    sprite.custom_size = Some(tam);
                    let ancla = Vec2::new(ag.agarre[0] / ag.tam[0] - 0.5, 0.5 - ag.agarre[1] / ag.tam[1]);
                    sprite.anchor = Anchor::Custom(ancla);
                    let mut ent = p.spawn((
                        ObjetoEnMano(ancla),
                        sprite,
                        Transform::from_xyz(0.0, 0.0, DZ),
                        Visibility::Hidden,
                        Name::new(format!("EnMano_{}", s.item)),
                    ));
                    if let Some(a) = anim {
                        ent.insert(a);
                    }
                    if let Some(light) = crate::held_item::item_light(&s.item).filter(|_| s.luz) {
                        ent.insert(light);
                    }
                }
            }
            if let Some(piel) = s.piel {
                let img: Handle<Image> = assets.load(format!("art/chargen/manos/{piel:03}.png"));
                p.spawn((
                    DedosSobreObjeto,
                    Sprite { image: img, custom_size: Some(crate::scene::TAMANO_JUGADOR), ..default() },
                    Transform::from_xyz(0.0, 0.0, 2.0 * DZ),
                    Visibility::Hidden,
                    Name::new("DedosSobreObjeto"),
                ));
            }
        });
    }
}

/// El cuadro actual de un cuerpo: la pose elegida + el índice del atlas.
fn cuadro_actual(estado: &str, sprite: &Sprite) -> Option<(Cuadro, bool)> {
    let frame = sprite.texture_atlas.as_ref().map(|a| a.index as u32).unwrap_or(0);
    cuadro_de(estado, frame)
}

/// Pone el objeto y los dedos en la mano de cada portador, cuadro a cuadro.
#[allow(clippy::type_complexity)]
fn colocar_objetos(
    jugador: Query<(&Sprite, &crate::sim::CurrentPose, &Children), (With<crate::sim::Player>, With<Sostiene>)>,
    aldeanos: Query<(&Sprite, &crate::society::NpcPose, &Children), (With<crate::society::Npc>, With<Sostiene>, Without<crate::sim::Player>)>,
    mut objetos: Query<(&ObjetoEnMano, &mut Transform, &mut Visibility, &mut Sprite), (With<ObjetoEnMano>, Without<DedosSobreObjeto>, Without<crate::sim::Player>, Without<crate::society::Npc>)>,
    mut dedos: Query<(&mut Sprite, &mut Visibility), (With<DedosSobreObjeto>, Without<ObjetoEnMano>, Without<crate::sim::Player>, Without<crate::society::Npc>)>,
    mut ocultos: Local<Vec<Entity>>,
) {
    ocultos.clear();
    let mut poner = |sprite: &Sprite, estado: &str, kids: &Children| {
        let cuadro = cuadro_actual(estado, sprite);
        for k in kids.iter() {
            if let Ok((o, mut tf, mut vis, mut s)) = objetos.get_mut(*k) {
                match cuadro {
                    Some((c, flip)) => {
                        let pos = px_a_local(c.ancla, crate::scene::TAMANO_JUGADOR, flip);
                        tf.translation = Vec3::new(pos.x, pos.y, if c.detras { -DZ } else { DZ });
                        s.flip_x = flip;
                        // El agarre se espeja con el sprite: el ancla va del otro lado.
                        s.anchor = Anchor::Custom(Vec2::new(if flip { -o.0.x } else { o.0.x }, o.0.y));
                        *vis = Visibility::Inherited;
                    }
                    None => *vis = Visibility::Hidden,
                }
            }
            if let Ok((mut s, mut vis)) = dedos.get_mut(*k) {
                match cuadro.filter(|(c, _)| !c.detras) {
                    Some((c, flip)) => {
                        let x0 = c.col * CELL[0];
                        let y0 = c.fila_manos * CELL[1];
                        s.rect = Some(Rect::new(x0 as f32, y0 as f32, (x0 + CELL[0]) as f32, (y0 + CELL[1]) as f32));
                        s.flip_x = flip;
                        s.color = sprite.color;
                        *vis = Visibility::Inherited;
                    }
                    None => *vis = Visibility::Hidden,
                }
            }
        }
    };
    for (sprite, pose, kids) in &jugador {
        poner(sprite, pose.nombre(), kids);
    }
    for (sprite, pose, kids) in &aldeanos {
        poner(sprite, pose.aplicada(), kids);
    }
}

/// La mano del jugador este cuadro, para que `armas.rs` gire el arma alrededor de ella.
fn calcular_mano_del_jugador(
    jugador: Query<(&Transform, &Sprite, &crate::sim::CurrentPose), With<crate::sim::Player>>,
    mut mano: ResMut<ManoDelJugador>,
) {
    let Ok((tf, sprite, pose)) = jugador.get_single() else {
        mano.vigente = false;
        return;
    };
    match cuadro_actual(pose.nombre(), sprite) {
        Some((c, flip)) => {
            let local = px_a_local(c.ancla, crate::scene::TAMANO_JUGADOR, flip);
            *mano = ManoDelJugador {
                vigente: true,
                pos: tf.translation.truncate() + local * tf.scale.truncate(),
                detras: c.detras,
                z: tf.translation.z,
            };
        }
        None => mano.vigente = false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_json_de_manos_se_lee_y_trae_lo_que_el_juego_pide() {
        let d = datos().expect("manos.json");
        for k in ["idle", "walk", "gun_idle", "shoot", "hit"] {
            assert!(d.filas.contains_key(k), "falta la fila de manos `{k}`");
        }
        assert!(objeto("items/axe").is_some());
        assert!(objeto("items/gun_ak").is_some());
        assert!(objeto("items/lantern").is_some());
    }

    #[test]
    fn caminar_con_el_arma_usa_las_manos_de_apuntar() {
        let (a, _) = cuadro_de("walk_gun_right#false", 2).unwrap();
        let (b, _) = cuadro_de("gun_idle_right#false", 2).unwrap();
        assert_eq!(a, b);
        assert!(cuadro_de("hands_chest_down#false", 0).is_none(), "comer no agarra nada");
    }

    #[test]
    fn mirando_arriba_el_objeto_va_detras() {
        assert!(cuadro_de("walk_up#false", 0).unwrap().0.detras);
        assert!(!cuadro_de("walk_down#false", 0).unwrap().0.detras);
    }

    #[test]
    fn el_espejo_cambia_el_lado_de_la_mano() {
        let c = Vec2::new(24.0, 52.0);
        let a = px_a_local(c, Vec2::new(0.95, 1.9), false);
        let b = px_a_local(c, Vec2::new(0.95, 1.9), true);
        assert!((a.x + b.x).abs() < 1e-6 && (a.y - b.y).abs() < 1e-6 && a.x > 0.0);
    }
}
