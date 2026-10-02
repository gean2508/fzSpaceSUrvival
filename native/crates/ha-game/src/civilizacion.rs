//! Puente entre el juego y la CIVILIZACIÓN EMERGENTE (`ha_shared::civ`).
//!
//! El motor civil vive entero en `ha-shared`: pueblos, gobierno, guerras, ciencia,
//! cultos, expediciones. Acá sólo hay tres cosas:
//!
//! 1. **Reloj y disco**: la civilización corre SIEMPRE (1 día ≈ 30 s reales, un día por
//!    fotograma como mucho), se guarda sola y al abrir el juego se pone al día con lo que
//!    pasó mientras no estabas (tope: 8 años). Lo único que se dibuja es lo que está cerca
//!    del jugador (`society.rs`); el resto es puro dato, y barato.
//! 2. **El jugador es una persona más**: se lo inscribe en el pueblo más cercano; matar a un
//!    NPC encarnado (`matado_por_el_jugador`) le cuesta prestigio y abre sucesión; las bajas
//!    de la civilización (ejecuciones, tragedias) sacan al NPC de la escena.
//! 3. **Vista de dios** (botón MUNDO del menú): el pueblo, el mundo, la crónica y las cosas
//!    que el jugador puede hacer por las mismas puertas que cualquiera.
//!
//! Esta capa NO tiene espejo en TypeScript: el servidor no la conoce (es del cliente).

use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use ha_shared::civ::cronica::fecha;
use ha_shared::civ::obras::CUERPOS;
use ha_shared::civ::pueblo::Convencion;
use ha_shared::civ::saber::{self, DEFS, N_SABERES};
use ha_shared::civ::{sitios_tierra, Decreto, Mundo, DIAS_ANIO, JUGADOR_ID};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::inventory_persist::PlayerInventory;
use crate::society::{despedir_npc, Npc, Society};
use crate::ui_editor::UiEditorState;

const SEGUNDOS_POR_DIA: f32 = 30.0;
const TOPE_DIAS_AUSENTE: u64 = 160;
const DIAS_POR_FOTOGRAMA_AL_PONERSE_AL_DIA: u64 = 3;
const GUARDAR_CADA: f32 = 90.0;
const RADIO_PUEBLO: f32 = 140.0;

const CIAN: Color = Color::srgb(0.55, 0.95, 1.0);
const FONDO: Color = Color::srgb(0.03, 0.045, 0.08);
const BOTON: Color = Color::srgb(0.10, 0.14, 0.22);
const BOTON_ON: Color = Color::srgb(0.16, 0.28, 0.40);

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Pestania {
    #[default]
    Pueblo,
    Mundo,
    Cronica,
    Fisica,
    Acciones,
}

#[derive(Resource, Default)]
pub struct Civ {
    mundo: Option<Mundo>,
    acc: f32,
    guardar: f32,
    /// Días de "ponerse al día" que faltan (ausencia del jugador).
    pendientes: u64,
    jugador_visto: f32,
    pub panel: bool,
    pestania: Pestania,
    mensaje: String,
    refrescar: f32,
    sucio: bool,
    /// Se abrió tocando el Registro del pueblo: no depende del menú principal.
    independiente: bool,
    /// Dónde está el jugador (x, z), para las acciones que dependen del lugar.
    pub(crate) jugador_pos: (f32, f32),
    /// El cuerpo que el jugador tiene debajo (0 = Tierra, 1 = Luna…): decide qué pueblos son "los de acá".
    pub(crate) cuerpo: u8,
}

#[derive(Serialize, Deserialize)]
struct Archivo {
    unix: u64,
    mundo: Mundo,
}

#[derive(Serialize)]
struct ArchivoRef<'a> {
    unix: u64,
    mundo: &'a Mundo,
}

fn ahora() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn ruta() -> PathBuf {
    crate::inventory_persist::persist_dir().join("civ").join("civ.json")
}

impl Civ {
    #[cfg(not(target_os = "android"))]
    pub(crate) fn mundo_mut(&mut self) -> Option<&mut Mundo> {
        self.mundo.as_mut()
    }

    pub(crate) fn mundo_ref(&self) -> Option<&Mundo> {
        self.mundo.as_ref()
    }

    /// **El acabado de las casas de un pueblo, según lo que descubrió** (índice en `edificios::TEMAS`):
    /// ladrillo si el pueblo civil más cercano sabe hacer cerámica, azul si trabaja metales, hormigón
    /// si no. Lo descubierto se ve en la calle.
    pub(crate) fn tema_cerca(&self, x: f32, z: f32) -> usize {
        use ha_shared::civ::materia::Clase;
        let idx = |slug: &str| crate::edificios::TEMAS.iter().position(|(s, _)| *s == slug).unwrap_or(0);
        let Some(m) = self.mundo.as_ref() else { return idx("hormigon") };
        let Some(p) = m
            .pueblos
            .values()
            .filter(|p| p.vivo && p.cuerpo == 0)
            .min_by(|a, b| ((a.x - x).powi(2) + (a.z - z).powi(2)).total_cmp(&((b.x - x).powi(2) + (b.z - z).powi(2))))
        else {
            return idx("hormigon");
        };
        let clases: Vec<Clase> = p.cuaderno.producibles().iter().filter_map(|id| m.materia.get(*id).map(|x| x.clase)).collect();
        if clases.contains(&Clase::Ceramica) {
            idx("ladrillo")
        } else if clases.iter().any(|c| matches!(c, Clase::Metal | Clase::Aleacion)) {
            idx("azul")
        } else {
            idx("hormigon")
        }
    }

    /// El Registro es un objeto del pueblo: tocarlo abre la consola sin pasar por el menú.
    pub(crate) fn abrir_desde_registro(&mut self) {
        self.panel = true;
        self.independiente = true;
        self.pestania = Pestania::Pueblo;
        self.refrescar = 0.0;
    }

    fn cargar(&mut self) {
        let semilla = ha_shared::partida::semilla() as u64;
        let leido = std::fs::read_to_string(ruta()).ok().and_then(|s| serde_json::from_str::<Archivo>(&s).ok());
        match leido {
            Some(a) if a.mundo.semilla == semilla => {
                let mut m = a.mundo;
                m.reindexar();
                let ausente = ahora().saturating_sub(a.unix) as f32 / SEGUNDOS_POR_DIA;
                self.pendientes = (ausente as u64).min(TOPE_DIAS_AUSENTE);
                self.mundo = Some(m);
            }
            _ => {
                let m = Mundo::sembrar(semilla, &sitios_tierra());
                eprintln!("ha-civ: mundo sembrado ({} pueblos, {} personas)", m.pueblos.len(), m.personas.len());
                self.mundo = Some(m);
                self.sucio = true;
            }
        }
    }

    fn guardar(&mut self) {
        let Some(m) = self.mundo.as_ref() else { return };
        let p = ruta();
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if let Ok(s) = serde_json::to_string(&ArchivoRef { unix: ahora(), mundo: m }) {
            let tmp = p.with_extension("json.tmp");
            if std::fs::write(&tmp, s).is_ok() {
                let _ = std::fs::rename(&tmp, &p);
            }
        }
        self.sucio = false;
    }

    /// La persona del mundo que encarna el NPC `uid` (la asigna la primera vez que hace
    /// falta: es un notable del pueblo más cercano que todavía no tiene cuerpo).
    fn persona_para(&mut self, uid: u64, x: f32, z: f32) -> Option<u32> {
        let m = self.mundo.as_mut()?;
        let clave = (uid as u32).max(1);
        if let Some(p) = m.persona_de_npc(clave) {
            return Some(p);
        }
        let pueblo = pueblo_mas_cercano(m, self.cuerpo, x, z)?;
        let dia = m.dia;
        let libres: Vec<u32> = m.pueblos[&pueblo]
            .gente
            .iter()
            .copied()
            .filter(|g| *g != JUGADOR_ID)
            .filter_map(|g| m.personas.get(&g))
            .filter(|p| p.npc == 0 && p.adulta(dia))
            .map(|p| p.id)
            .collect();
        if libres.is_empty() {
            return None;
        }
        let id = libres[(clave as usize) % libres.len()];
        m.personas.get_mut(&id)?.npc = clave;
        Some(id)
    }

    /// El jugador mató al NPC `uid`: la civilización lo registra (prestigio, crimen, sucesión).
    pub fn matado_por_el_jugador(&mut self, uid: u64, x: f32, z: f32) {
        if let Some(p) = self.persona_para(uid, x, z) {
            if let Some(m) = self.mundo.as_mut() {
                m.notificar_muerte(p, true);
                self.sucio = true;
            }
        }
    }

    /// Una línea de contexto para el menú del NPC ("Marta Ríos, erudita de Rocaviva").
    pub fn ficha_npc(&mut self, uid: u64, x: f32, z: f32) -> Option<String> {
        let p = self.persona_para(uid, x, z)?;
        let m = self.mundo.as_ref()?;
        let per = m.personas.get(&p)?;
        let pu = m.pueblos.get(&per.pueblo)?;
        let cargo = if pu.gobernante == Some(p) { format!("gobierna {}", pu.nombre) } else { format!("{} de {}", per.oficio.etiqueta(), pu.nombre) };
        Some(format!("{}, {}", per.nombre, cargo))
    }

    /// Como `persona_para`, pero para los que se ven estudiando: si el NPC todavía no encarna a
    /// nadie, se le da un erudito o artesano libre del pueblo (si hay), no uno cualquiera.
    pub(crate) fn encarnar_estudioso(&mut self, uid: u64, x: f32, z: f32) -> Option<u32> {
        use ha_shared::civ::persona::Oficio;
        let m = self.mundo.as_mut()?;
        let clave = (uid as u32).max(1);
        if let Some(p) = m.persona_de_npc(clave) {
            return Some(p);
        }
        let pueblo = pueblo_mas_cercano(m, self.cuerpo, x, z)?;
        let dia = m.dia;
        let libres: Vec<u32> = m.pueblos[&pueblo]
            .gente
            .iter()
            .copied()
            .filter(|g| *g != JUGADOR_ID)
            .filter_map(|g| m.personas.get(&g))
            .filter(|p| p.npc == 0 && p.adulta(dia) && matches!(p.oficio, Oficio::Erudito | Oficio::Artesano))
            .map(|p| p.id)
            .collect();
        if libres.is_empty() {
            return None;
        }
        let id = libres[(clave as usize) % libres.len()];
        m.personas.get_mut(&id)?.npc = clave;
        Some(id)
    }

    /// Lo que el NPC `uid` sabe hacer (lo que su pueblo anotó en el cuaderno y él conoce) y cómo
    /// se llama cada material. `encarnar`: si todavía no es una persona, se le busca una (los que
    /// se ven estudiando prefieren eruditos/artesanos).
    pub(crate) fn saber_de_npc(&mut self, uid: u64, x: f32, z: f32, encarnar: bool) -> Option<SaberNpc> {
        use ha_shared::civ::persona::Oficio;
        let p = if encarnar { self.encarnar_estudioso(uid, x, z).or_else(|| self.persona_para(uid, x, z)) } else { self.persona_para(uid, x, z) }?;
        let m = self.mundo.as_ref()?;
        let per = m.personas.get(&p)?;
        let pu = m.pueblos.get(&per.pueblo)?;
        let estudia = matches!(per.oficio, Oficio::Erudito | Oficio::Artesano);
        let nombre = |id: u64| m.materia.get(id).map(|x| x.nombre.clone()).unwrap_or_else(|| "algo".into());
        let mut recetas = Vec::new();
        for (k, e) in pu.cuaderno.entradas.iter() {
            let Some(res) = e.resultado else { continue };
            // Los que estudian saben todo el cuaderno (hasta lo dudoso); el resto, sólo lo firme y
            // de a pedazos: cada uno sabe lo que le toca por su trabajo.
            let sabe = if estudia { true } else { e.firme() && (*k ^ p as u64) % 3 == 0 };
            if !sabe {
                continue;
            }
            let Some(mat) = m.materia.get(res).cloned() else { continue };
            recetas.push(Receta {
                entrada: e.clone(),
                resultado: mat,
                nombre_a: nombre(e.a),
                nombre_b: (e.b != ha_shared::civ::descubrimiento::SIN_B).then(|| nombre(e.b)),
            });
        }
        Some(SaberNpc {
            persona: p,
            oficio: per.oficio,
            curiosidad: per.curiosidad,
            nombre: per.nombre.clone(),
            pueblo: pu.nombre.clone(),
            recetas,
        })
    }

    /// El jugador le enseña una receta suya al pueblo de este NPC. `Some(texto)` si el pueblo no la sabía.
    pub(crate) fn ensenar_al_pueblo(&mut self, uid: u64, x: f32, z: f32, e: &ha_shared::civ::descubrimiento::Entrada, m: &ha_shared::civ::materia::Material) -> Option<String> {
        let p = self.persona_para(uid, x, z)?;
        let mundo = self.mundo.as_mut()?;
        let pueblo = mundo.personas.get(&p)?.pueblo;
        if !mundo.pueblo_aprende_del_jugador(pueblo, e, m) {
            return None;
        }
        self.sucio = true;
        Some(mundo.pueblos.get(&pueblo)?.nombre.clone())
    }

    /// Un NPC encarnado murió por una causa del mundo (infectados, no el jugador): la civilización lo
    /// registra igual que una baja cualquiera (sucesión, duelo).
    pub(crate) fn muerte_de_npc(&mut self, uid: u64, x: f32, z: f32) {
        if let Some(p) = self.persona_para(uid, x, z) {
            if let Some(m) = self.mundo.as_mut() {
                m.notificar_muerte(p, false);
                self.sucio = true;
            }
        }
    }

    /// 0..1: cuánto sabe techar y sellar el pueblo civil más cercano — cerámica (tejas), aglutinante
    /// (mortero), metal (clavos, chapa) y vidrio (ventanas). Lo calcula de lo que el cuaderno dice que
    /// sabe PRODUCIR, no de una lista.
    pub(crate) fn saber_de_techar(&self, x: f32, z: f32) -> f32 {
        use ha_shared::civ::materia::Clase;
        let Some(m) = self.mundo.as_ref() else { return 0.0 };
        let Some(p) = m
            .pueblos
            .values()
            .filter(|p| p.vivo && p.cuerpo == 0)
            .min_by(|a, b| ((a.x - x).powi(2) + (a.z - z).powi(2)).total_cmp(&((b.x - x).powi(2) + (b.z - z).powi(2))))
        else {
            return 0.0;
        };
        let clases: Vec<Clase> = p.cuaderno.producibles().iter().filter_map(|id| m.materia.get(*id).map(|x| x.clase)).collect();
        let tiene = |f: &dyn Fn(&Clase) -> bool| clases.iter().any(f);
        let n = [
            tiene(&|c| *c == Clase::Ceramica),
            tiene(&|c| *c == Clase::Aglutinante),
            tiene(&|c| matches!(c, Clase::Metal | Clase::Aleacion)),
            tiene(&|c| *c == Clase::Vidrio),
        ];
        n.iter().filter(|b| **b).count() as f32 / 4.0
    }
}

/// Una receta que un NPC conoce, con los nombres ya resueltos.
#[derive(Clone)]
pub(crate) struct Receta {
    pub entrada: ha_shared::civ::descubrimiento::Entrada,
    pub resultado: ha_shared::civ::materia::Material,
    pub nombre_a: String,
    pub nombre_b: Option<String>,
}

/// Lo que un aldeano sabe, visto desde el juego (menú y taller visible).
pub(crate) struct SaberNpc {
    pub persona: u32,
    pub oficio: ha_shared::civ::persona::Oficio,
    pub curiosidad: f32,
    pub nombre: String,
    pub pueblo: String,
    pub recetas: Vec<Receta>,
}

fn pueblo_mas_cercano(m: &Mundo, cuerpo: u8, x: f32, z: f32) -> Option<u32> {
    m.pueblos
        .values()
        .filter(|p| p.vivo && p.cuerpo == cuerpo)
        .map(|p| (p.id, (p.x - x).powi(2) + (p.z - z).powi(2)))
        .filter(|(_, d)| *d < RADIO_PUEBLO * RADIO_PUEBLO)
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
        .map(|(id, _)| id)
}

// ─────────────────────────── plugin ───────────────────────────

#[derive(Component)]
pub struct BotonMundo;
#[derive(Component)]
struct PanelRaiz;
#[derive(Component)]
struct PanelTexto;
#[derive(Component)]
struct PanelAcciones;
#[derive(Component, Clone, Copy)]
struct BotonPestania(Pestania);
#[derive(Component)]
struct BotonCerrar;
#[derive(Component, Clone, Copy)]
struct BotonAccion(Accion);

#[derive(Clone, Copy, PartialEq)]
enum Accion {
    Donar,
    Pieza,
    Candidato,
    Retirarse,
    Ensenar,
    Mediar,
    Impuesto(i8),
    PenaRobo(i8),
    Culto(i8),
    Paz,
    Agitar(usize),
    RefugioConstruir,
    RefugioAbastecer,
    RefugioInvitar,
    RefugioFundar,
}

pub struct CivPlugin;

impl Plugin for CivPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            OnExit(crate::game_phase::GamePhase::Playing),
            (guardar_al_salir, crate::salir::reiniciar::<Civ>).chain(),
        );
        app.init_resource::<Civ>().add_systems(
            Update,
            (
                civ_cargar,
                civ_reloj,
                civ_jugador,
                civ_bajas,
                civ_guardar,
                civ_autoshot,
                panel_sincronizar,
                panel_botones,
                panel_texto,
            )
                .chain()
                .run_if(crate::game_phase::world_active),
        );
    }
}

fn guardar_al_salir(mut civ: ResMut<Civ>) {
    civ.guardar();
}

fn civ_cargar(mut civ: ResMut<Civ>, phase: Res<State<crate::game_phase::GamePhase>>) {
    if civ.mundo.is_none() && *phase.get() == crate::game_phase::GamePhase::Playing {
        civ.cargar();
    }
}

fn civ_reloj(time: Res<Time>, mut civ: ResMut<Civ>) {
    if civ.mundo.is_none() {
        return;
    }
    civ.acc += time.delta_secs();
    let mut dias = 0u64;
    if civ.pendientes > 0 {
        dias = civ.pendientes.min(DIAS_POR_FOTOGRAMA_AL_PONERSE_AL_DIA);
        civ.pendientes -= dias;
    } else if civ.acc >= SEGUNDOS_POR_DIA {
        civ.acc -= SEGUNDOS_POR_DIA;
        dias = 1;
    }
    if dias > 0 {
        if let Some(m) = civ.mundo.as_mut() {
            m.avanzar(dias);
        }
        civ.sucio = true;
    }
}

/// Inscribe al jugador en el pueblo que tiene alrededor (y lo muda si se va a otro).
fn civ_jugador(
    time: Res<Time>,
    mut civ: ResMut<Civ>,
    soc: Res<Society>,
    player: Query<&Transform, With<crate::sim::Player>>,
    selected: Res<crate::game_phase::SelectedCharacter>,
) {
    if let Ok(pt) = player.get_single() {
        civ.jugador_pos = (pt.translation.x, -pt.translation.y);
    }
    civ.jugador_visto += time.delta_secs();
    if civ.jugador_visto < 2.0 {
        return;
    }
    civ.jugador_visto = 0.0;
    let Some(cuerpo) = soc.active_body() else { return };
    civ.cuerpo = cuerpo;
    let Ok(pt) = player.get_single() else { return };
    let (px, pz) = (pt.translation.x, -pt.translation.y);
    let Some(m) = civ.mundo.as_mut() else { return };
    let Some(cerca) = pueblo_mas_cercano(m, cuerpo, px, pz) else { return };
    if m.jugador.pueblo != Some(cerca) {
        m.entrar_jugador(cerca, &selected.name);
        civ.sucio = true;
    }
}

/// Lo que la civilización mata (ejecuciones, tragedias) se va de la escena.
fn civ_bajas(
    mut commands: Commands,
    mut civ: ResMut<Civ>,
    mut soc: ResMut<Society>,
    npcs: Query<(Entity, &Npc)>,
) {
    let Some(m) = civ.mundo.as_mut() else { return };
    if m.bajas.is_empty() {
        return;
    }
    for b in m.drenar_bajas() {
        if b.npc == 0 {
            continue;
        }
        if let Some((e, n)) = npcs.iter().find(|(_, n)| (n.uid as u32).max(1) == b.npc) {
            despedir_npc(&mut commands, &mut soc, e, n);
        }
    }
}

fn civ_guardar(time: Res<Time>, mut civ: ResMut<Civ>) {
    civ.guardar += time.delta_secs();
    if civ.guardar >= GUARDAR_CADA {
        civ.guardar = 0.0;
        if civ.sucio {
            civ.guardar();
        }
    }
}

// ─────────────────────────── panel ───────────────────────────

fn boton(padre: &mut ChildBuilder, etiqueta: &str, ancho: f32, comp: impl Bundle) {
    padre
        .spawn((
            Button,
            Node {
                width: Val::Px(ancho),
                height: Val::Px(36.0),
                border: UiRect::all(Val::Px(2.0)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BorderColor(CIAN),
            BackgroundColor(BOTON),
            comp,
        ))
        .with_children(|b| {
            b.spawn((Text::new(etiqueta), TextFont { font_size: 13.0, ..default() }, TextColor(CIAN)));
        });
}

fn fila() -> Node {
    Node {
        flex_direction: FlexDirection::Row,
        flex_wrap: FlexWrap::Wrap,
        column_gap: Val::Px(8.0),
        row_gap: Val::Px(6.0),
        ..default()
    }
}

fn panel_sincronizar(
    mut commands: Commands,
    editor: Res<UiEditorState>,
    mut civ: ResMut<Civ>,
    raices: Query<Entity, With<PanelRaiz>>,
) {
    if !civ.independiente && (!editor.menu_open || editor.settings_open) {
        civ.panel = false;
    }
    if !civ.panel {
        civ.independiente = false;
    }
    let hay = raices.iter().next().is_some();
    if civ.panel && !hay {
        civ.refrescar = 0.0;
        commands
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    top: Val::Px(0.0),
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(8.0),
                    padding: UiRect::all(Val::Px(12.0)),
                    ..default()
                },
                BackgroundColor(FONDO),
                GlobalZIndex(910),
                FocusPolicy::Block,
                PanelRaiz,
                Name::new("PanelMundo"),
            ))
            .with_children(|raiz| {
                let mut nodo = fila();
                nodo.justify_content = JustifyContent::FlexStart;
                raiz.spawn(nodo).with_children(|f| {
                    boton(f, "PUEBLO", 100.0, BotonPestania(Pestania::Pueblo));
                    boton(f, "MUNDO", 100.0, BotonPestania(Pestania::Mundo));
                    boton(f, "CRÓNICA", 100.0, BotonPestania(Pestania::Cronica));
                    boton(f, "FÍSICA", 100.0, BotonPestania(Pestania::Fisica));
                    boton(f, "ACCIONES", 100.0, BotonPestania(Pestania::Acciones));
                    boton(f, "VOLVER", 100.0, BotonCerrar);
                });
                raiz.spawn((
                    Text::new(""),
                    TextFont { font_size: 14.0, ..default() },
                    TextColor(Color::srgb(0.88, 0.94, 1.0)),
                    Node { flex_grow: 1.0, ..default() },
                    PanelTexto,
                ));
                let mut acciones = fila();
                acciones.display = Display::None;
                raiz.spawn((acciones, PanelAcciones)).with_children(|f| {
                    boton(f, "DONAR 5 MANZANAS", 170.0, BotonAccion(Accion::Donar));
                    boton(f, "ENTREGAR PIEZA DE NAVE", 200.0, BotonAccion(Accion::Pieza));
                    boton(f, "ENSEÑAR", 100.0, BotonAccion(Accion::Ensenar));
                    boton(f, "MEDIAR", 100.0, BotonAccion(Accion::Mediar));
                    boton(f, "PRESENTARME", 130.0, BotonAccion(Accion::Candidato));
                    boton(f, "RETIRARME", 110.0, BotonAccion(Accion::Retirarse));
                    boton(f, "AGITAR: VOTO", 130.0, BotonAccion(Accion::Agitar(Convencion::VOTO)));
                    boton(f, "AGITAR: FE", 120.0, BotonAccion(Accion::Agitar(Convencion::FE)));
                    boton(f, "AGITAR: FUERZA", 140.0, BotonAccion(Accion::Agitar(Convencion::FUERZA)));
                    boton(f, "IMPUESTO +", 110.0, BotonAccion(Accion::Impuesto(1)));
                    boton(f, "IMPUESTO -", 110.0, BotonAccion(Accion::Impuesto(-1)));
                    boton(f, "PENA ROBO +", 120.0, BotonAccion(Accion::PenaRobo(1)));
                    boton(f, "PENA ROBO -", 120.0, BotonAccion(Accion::PenaRobo(-1)));
                    boton(f, "CULTOS LIBRES +", 140.0, BotonAccion(Accion::Culto(1)));
                    boton(f, "CULTOS LIBRES -", 140.0, BotonAccion(Accion::Culto(-1)));
                    boton(f, "PEDIR PAZ", 110.0, BotonAccion(Accion::Paz));
                    boton(f, "CONSTRUIR REFUGIO", 170.0, BotonAccion(Accion::RefugioConstruir));
                    boton(f, "ABASTECER REFUGIO", 170.0, BotonAccion(Accion::RefugioAbastecer));
                    boton(f, "INVITAR AL REFUGIO", 180.0, BotonAccion(Accion::RefugioInvitar));
                    boton(f, "FUNDAR PUEBLO NUEVO", 190.0, BotonAccion(Accion::RefugioFundar));
                });
            });
    } else if !civ.panel && hay {
        for e in &raices {
            commands.entity(e).despawn_recursive();
        }
    }
}

const PIEZAS: [(&str, u8); 5] = [
    ("part_engine", saber::COHETERIA),
    ("part_tank", saber::SOPORTE_VITAL),
    ("part_lander", saber::ASTRONAUTICA),
    ("part_nose", saber::ASTRONAUTICA),
    ("part_fin", saber::MECANICA_ORBITAL),
];

#[allow(clippy::type_complexity)]
fn panel_botones(
    mut commands: Commands,
    mut civ: ResMut<Civ>,
    mut inv: ResMut<PlayerInventory>,
    mut soc: ResMut<Society>,
    npcs: Query<(Entity, &Npc, &Transform)>,
    botones_mundo: Query<&Interaction, (Changed<Interaction>, With<BotonMundo>)>,
    pestanias: Query<(&Interaction, &BotonPestania), Changed<Interaction>>,
    cerrar: Query<&Interaction, (Changed<Interaction>, With<BotonCerrar>)>,
    acciones: Query<(&Interaction, &BotonAccion), Changed<Interaction>>,
) {
    if botones_mundo.iter().any(|i| *i == Interaction::Pressed) {
        civ.panel = true;
        civ.refrescar = 0.0;
    }
    if !civ.panel {
        return;
    }
    for (i, p) in &pestanias {
        if *i == Interaction::Pressed {
            civ.pestania = p.0;
            civ.mensaje.clear();
            civ.refrescar = 0.0;
        }
    }
    if cerrar.iter().any(|i| *i == Interaction::Pressed) {
        civ.panel = false;
        return;
    }
    for (i, a) in &acciones {
        if *i != Interaction::Pressed {
            continue;
        }
        let msg = match a.0 {
            Accion::RefugioInvitar => invitar_al_refugio(&mut commands, &mut civ, &mut soc, &npcs),
            otra => ejecutar(&mut civ, &mut inv, otra),
        };
        civ.mensaje = msg;
        civ.refrescar = 0.0;
        civ.sucio = true;
    }
}

fn ejecutar_refugio(civ: &mut Civ, inv: &mut PlayerInventory, a: Accion) -> String {
    match a {
        Accion::RefugioConstruir => {
            if inv.qty("stone") < REFUGIO_PIEDRA || inv.qty("wood") < REFUGIO_MADERA {
                return format!("Un refugio pide {REFUGIO_PIEDRA} de piedra y {REFUGIO_MADERA} de madera.");
            }
            let (x, z) = civ.jugador_pos;
            let Some(m) = civ.mundo.as_mut() else { return "El mundo todavía no cargó.".into() };
            inv.remove("stone", REFUGIO_PIEDRA);
            inv.remove("wood", REFUGIO_MADERA);
            m.registrar_refugio(x, z, REFUGIO_CAPACIDAD, 0.0);
            "Cavás y sellás un refugio bajo tierra. Falta abastecerlo e invitar a la gente.".into()
        }
        Accion::RefugioAbastecer => {
            let (x, z) = civ.jugador_pos;
            let manzanas = inv.qty("apple").min(20);
            let Some(m) = civ.mundo.as_mut() else { return "El mundo todavía no cargó.".into() };
            let Some(id) = refugio_cercano(m, x, z) else { return "No hay ningún refugio a mano.".into() };
            if manzanas == 0 {
                return "No llevás comida para guardar.".into();
            }
            inv.remove("apple", manzanas);
            m.abastecer_refugio(id, manzanas as f32 * 6.0);
            format!("Guardás {manzanas} manzanas en el refugio.")
        }
        Accion::RefugioFundar => {
            let (x, z) = civ.jugador_pos;
            let Some(m) = civ.mundo.as_mut() else { return "El mundo todavía no cargó.".into() };
            let Some(id) = refugio_cercano(m, x, z) else { return "No hay ningún refugio a mano.".into() };
            match m.fundar_desde_refugio(id, None, x + 40.0, z) {
                Ok(p) => format!("Los sobrevivientes salen a la luz y fundan {}.", m.nombre_pueblo(p)),
                Err(e) => e.into(),
            }
        }
        _ => String::new(),
    }
}

fn ejecutar(civ: &mut Civ, inv: &mut PlayerInventory, a: Accion) -> String {
    if matches!(a, Accion::RefugioConstruir | Accion::RefugioAbastecer | Accion::RefugioFundar) {
        return ejecutar_refugio(civ, inv, a);
    }
    let Some(m) = civ.mundo.as_mut() else { return "El mundo todavía no cargó.".into() };
    let Some(pueblo) = m.jugador.pueblo else {
        return "Acercate a un pueblo: hasta que no vivas en uno, no sos de nadie.".into();
    };
    let nombre = m.nombre_pueblo(pueblo);
    match a {
        Accion::Donar => {
            if inv.qty("apple") < 5 {
                return "Te faltan manzanas (hacen falta 5).".into();
            }
            inv.remove("apple", 5);
            m.donar(pueblo, 20.0, 0.0);
            format!("Donaste comida a {nombre}. Te lo van a recordar.")
        }
        Accion::Pieza => {
            let Some((item, s)) = PIEZAS.iter().copied().find(|(it, _)| inv.qty(it) > 0) else {
                return "No llevás ninguna pieza de nave.".into();
            };
            inv.remove(item, 1);
            m.entregar_artefacto(pueblo, s);
            format!("{nombre} recibe una pieza de tecnología ajena: ya saben por dónde empezar.")
        }
        Accion::Candidato => {
            m.presentarse(pueblo);
            format!("Te presentás como candidato en {nombre}. La próxima elección dirá qué piensan de vos.")
        }
        Accion::Retirarse => {
            m.retirarse();
            "Retirás tu candidatura.".into()
        }
        Accion::Ensenar => {
            let saberes = m.pueblos[&pueblo].saberes;
            let proximo = (0..N_SABERES as u8)
                .filter(|s| !saberes.tiene(*s) && saber::prerrequisitos_ok(*s, saberes) && !DEFS[*s as usize].frontera)
                .min_by(|a, b| DEFS[*a as usize].costo.partial_cmp(&DEFS[*b as usize].costo).unwrap());
            let Some(s) = proximo else { return "No queda nada que puedas enseñarles ahora.".into() };
            match m.ensenar(pueblo, s) {
                Ok(()) => format!("Les enseñás {}.", DEFS[s as usize].nombre),
                Err(e) => e.into(),
            }
        }
        Accion::Mediar => {
            let en_guerra = m.guerras.iter().find_map(|g| {
                if g.bando_a.contains(&pueblo) {
                    g.bando_b.first().copied()
                } else if g.bando_b.contains(&pueblo) {
                    g.bando_a.first().copied()
                } else {
                    None
                }
            });
            let Some(otro) = en_guerra else { return "Tu pueblo no está en guerra.".into() };
            m.mediar(pueblo, otro);
            format!("Mediás entre {nombre} y {}.", m.nombre_pueblo(otro))
        }
        Accion::Paz => {
            let en_guerra = m.guerras.iter().find_map(|g| {
                if g.bando_a.contains(&pueblo) {
                    g.bando_b.first().copied()
                } else if g.bando_b.contains(&pueblo) {
                    g.bando_a.first().copied()
                } else {
                    None
                }
            });
            let Some(otro) = en_guerra else { return "Tu pueblo no está en guerra.".into() };
            match m.decretar(pueblo, Decreto::Paz(otro)) {
                Ok(()) => "Decretás la paz.".into(),
                Err(e) => e.into(),
            }
        }
        Accion::Agitar(f) => {
            m.incitar(pueblo, f);
            "Movés los ánimos. La gente lo nota, y a algunos no les gusta.".into()
        }
        Accion::Impuesto(d) => {
            let v = m.pueblos[&pueblo].leyes.impuesto + 0.05 * d as f32;
            resultado(m.decretar(pueblo, Decreto::Impuesto(v)), format!("Impuesto al {:.0}%.", v.clamp(0.0, 0.6) * 100.0))
        }
        Accion::PenaRobo(d) => {
            let v = (m.pueblos[&pueblo].leyes.pena_robo as i8 + d).clamp(0, 3) as u8;
            resultado(m.decretar(pueblo, Decreto::PenaRobo(v)), format!("Pena por robo: {}.", pena(v)))
        }
        Accion::RefugioConstruir | Accion::RefugioAbastecer | Accion::RefugioInvitar | Accion::RefugioFundar => unreachable!(),
        Accion::Culto(d) => {
            let v = m.pueblos[&pueblo].leyes.libertad_culto + 0.2 * d as f32;
            resultado(m.decretar(pueblo, Decreto::LibertadCulto(v)), format!("Libertad de cultos al {:.0}%.", v.clamp(0.0, 1.0) * 100.0))
        }
    }
}

const REFUGIO_PIEDRA: u32 = 60;
const REFUGIO_MADERA: u32 = 20;
const REFUGIO_CAPACIDAD: u16 = 24;
const ALCANCE_REFUGIO: f32 = 30.0;
const ALCANCE_INVITACION: f32 = 45.0;

fn refugio_cercano(m: &Mundo, x: f32, z: f32) -> Option<u32> {
    m.refugios
        .iter()
        .map(|r| (r.id, ((r.x - x).powi(2) + (r.z - z).powi(2)).sqrt()))
        .filter(|(_, d)| *d <= ALCANCE_REFUGIO)
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
        .map(|(id, _)| id)
}

/// Los NPC que rodean al jugador aceptan bajar al refugio; se van de la escena y quedan a
/// resguardo en el mundo. Además llegan hasta seis notables del pueblo más cercano que no
/// tienen cuerpo en pantalla (oyeron la convocatoria).
fn invitar_al_refugio(
    commands: &mut Commands,
    civ: &mut Civ,
    soc: &mut Society,
    npcs: &Query<(Entity, &Npc, &Transform)>,
) -> String {
    let (px, pz) = civ.jugador_pos;
    let Some(rid) = civ.mundo.as_ref().and_then(|m| refugio_cercano(m, px, pz)) else {
        return "No hay ningún refugio a mano.".into();
    };
    let mut metidos = 0;
    let mut motivo = "";
    for (e, n, tf) in npcs.iter() {
        let (nx, nz) = (tf.translation.x, -tf.translation.y);
        if ((nx - px).powi(2) + (nz - pz).powi(2)).sqrt() > ALCANCE_INVITACION {
            continue;
        }
        let Some(p) = civ.persona_para(n.uid, nx, nz) else { continue };
        let Some(m) = civ.mundo.as_mut() else { break };
        match m.refugiar(p, rid) {
            Ok(()) => {
                metidos += 1;
                despedir_npc(commands, soc, e, n);
            }
            Err(e) => {
                motivo = e;
                break;
            }
        }
    }
    if motivo.is_empty() {
        if let Some(m) = civ.mundo.as_mut() {
            let dia = m.dia;
            let pueblo = m.pueblos.values().filter(|p| p.vivo && p.cuerpo == 0).min_by(|a, b| {
                let da = (a.x - px).powi(2) + (a.z - pz).powi(2);
                let db = (b.x - px).powi(2) + (b.z - pz).powi(2);
                da.partial_cmp(&db).unwrap()
            });
            let libres: Vec<u32> = pueblo
                .map(|p| {
                    p.gente
                        .iter()
                        .copied()
                        .filter(|g| *g != JUGADOR_ID)
                        .filter_map(|g| m.personas.get(&g))
                        .filter(|q| q.npc == 0 && q.refugio == 0 && q.adulta(dia))
                        .map(|q| q.id)
                        .take(6)
                        .collect()
                })
                .unwrap_or_default();
            for p in libres {
                if m.refugiar(p, rid).is_ok() {
                    metidos += 1;
                }
            }
        }
    }
    civ.sucio = true;
    match (metidos, motivo) {
        (0, "") => "No hay nadie cerca a quien invitar.".into(),
        (0, e) => e.into(),
        (n, "") => format!("{n} personas bajan al refugio."),
        (n, e) => format!("{n} personas bajan al refugio. {e}"),
    }
}

fn resultado(r: Result<(), &'static str>, ok: String) -> String {
    match r {
        Ok(()) => ok,
        Err(e) => e.into(),
    }
}

fn pena(v: u8) -> &'static str {
    match v {
        0 => "multa",
        1 => "cárcel",
        2 => "destierro",
        _ => "muerte",
    }
}

fn rango(r: u8) -> &'static str {
    match r {
        0 => "aldea",
        1 => "villa",
        _ => "ciudad",
    }
}

/// **Lo que el pueblo descubrió experimentando** (`ha_shared::civ::descubrimiento`): cuántas recetas
/// firmes tiene en el cuaderno, qué materiales sabe producir y cuánto le sirven.
fn texto_tecnica(m: &Mundo, p: &ha_shared::civ::pueblo::Pueblo) -> String {
    use ha_shared::civ::materia::{utilidad, USOS};
    let firmes = p.cuaderno.firmes().count();
    let mut mats: Vec<(f32, String)> = p
        .cuaderno
        .producibles()
        .iter()
        .filter_map(|id| m.materia.get(*id))
        .map(|x| (USOS.iter().map(|u| utilidad(x, *u)).fold(0.0f32, f32::max), x.nombre.clone()))
        .collect();
    mats.sort_by(|a, b| b.0.total_cmp(&a.0));
    let top: Vec<String> = mats.iter().take(3).map(|(u, n)| format!("{n} ({:.0}%)", u * 100.0)).collect();
    let mitos = p.cuaderno.entradas.values().filter(|e| e.ritual && e.resultado.is_some()).count();
    let mut t = format!("Experimentos: {firmes} recetas firmes");
    if !top.is_empty() {
        t += &format!("; lo mejor que sabe hacer: {}", top.join(", "));
    }
    if mitos > 0 {
        t += &format!("; cree en {mitos} ritual{} que todavía no probó sin él", if mitos == 1 { "" } else { "es" });
    }
    t += ".\n";
    t
}

fn texto_pueblo(m: &Mundo) -> String {
    let Some(id) = m.jugador.pueblo else {
        return "Todavía no vivís en ningún pueblo.\nAcercate a uno: la gente te va a ir conociendo.".into();
    };
    let p = &m.pueblos[&id];
    let gob = match p.gobernante {
        Some(g) if g == JUGADOR_ID => "vos".to_string(),
        Some(g) => m.nombre_persona(g),
        None => "nadie (vacante)".into(),
    };
    let culto = m.cultos.iter().find(|c| c.id == p.culto).map(|c| c.nombre.clone()).unwrap_or_else(|| "ninguno".into());
    let guerra = m
        .guerras
        .iter()
        .find(|g| g.bando_a.contains(&id) || g.bando_b.contains(&id))
        .map(|g| {
            let otros = if g.bando_a.contains(&id) { &g.bando_b } else { &g.bando_a };
            let ventaja = if g.bando_a.contains(&id) { g.avance } else { -g.avance };
            format!("EN GUERRA contra {} ({})", otros.iter().map(|o| m.nombre_pueblo(*o)).collect::<Vec<_>>().join(", "), if ventaja > 0.15 { "van ganando" } else if ventaja < -0.15 { "van perdiendo" } else { "parejo" })
        })
        .unwrap_or_else(|| "en paz".into());
    let mut t = format!(
        "{} ({}), {}\nRégimen: {}. Gobierna: {}.\nGente: {:.0}. Comida: {:.0}. Materiales: {:.0}. Tesoro: {:.0}.\n",
        p.nombre, rango(p.rango), fecha(m.dia), p.regimen(), gob, p.pob, p.comida, p.mat, p.tesoro,
    );
    t += &format!(
        "Legitimidad {:.0}%. Moral {:.0}%. Orden {:.0}%. Hambre {:.0}%. Crimen {:.0}%.\n",
        p.legit * 100.0, p.moral * 100.0, p.orden * 100.0, p.hambre * 100.0, p.crimen * 100.0
    );
    t += &format!(
        "Culto: {}. Saberes: {} de {}. Estado: {}.\n",
        culto, p.saberes.cuenta(), N_SABERES, guerra
    );
    t += &texto_tecnica(m, p);
    t += &format!(
        "Leyes: impuesto {:.0}%, robo = {}, homicidio = {}, cultos libres {:.0}%, servicio {:.0}%.\n",
        p.leyes.impuesto * 100.0, pena(p.leyes.pena_robo), pena(p.leyes.pena_homicidio), p.leyes.libertad_culto * 100.0, p.leyes.servicio * 100.0
    );
    if let Some(pr) = p.proyecto {
        let destino = if pr.obra.nombre() == "nave" { format!(" hacia {}", CUERPOS[pr.destino as usize]) } else { String::new() };
        t += &format!("Obra en marcha: {}{} ({:.0}%).\n", pr.obra.nombre(), destino, 100.0 * pr.prog / pr.costo.max(1.0));
    }
    let prest = m.prestigio(id);
    t += &format!("Tu prestigio acá: {:+.0}%.", prest * 100.0);
    if p.gobernante == Some(JUGADOR_ID) {
        t += " Sos quien manda.";
    } else if m.jugador.candidato == Some(id) {
        t += " Sos candidato.";
    }
    t
}

fn texto_mundo(m: &Mundo) -> String {
    let vivos: Vec<_> = m.pueblos.values().filter(|p| p.vivo).collect();
    let pob: f32 = vivos.iter().map(|p| p.pob).sum();
    let mut t = format!("{}. {} pueblos vivos, {:.0} habitantes, {} personas con nombre.\n", fecha(m.dia), vivos.len(), pob, m.personas.len());
    t += &format!("Saberes descubiertos en el mundo: {} de {}. Cultos: {}.\n", m.saberes_mundo.cuenta(), N_SABERES, m.cultos.len());
    let mut cuerpos: Vec<String> = Vec::new();
    for c in 1..CUERPOS.len() as u8 {
        let n = vivos.iter().filter(|p| p.cuerpo == c).count();
        if n > 0 {
            cuerpos.push(format!("{} ({})", CUERPOS[c as usize], n));
        }
    }
    t += &format!("Fuera de la Tierra hay gente en: {}.\n", if cuerpos.is_empty() { "ningún lado todavía".to_string() } else { cuerpos.join(", ") });
    for e in &m.expediciones {
        t += &format!("En viaje: {} rumbo a {}, llega en {} días.\n", m.nombre_pueblo(e.origen), CUERPOS[e.destino as usize], e.llegada.saturating_sub(m.dia));
    }
    if m.guerras.is_empty() {
        t += "No hay guerras.\n";
    } else {
        t += &format!("Guerras en curso ({}):\n", m.guerras.len());
        for g in m.guerras.iter().take(5) {
            let a: Vec<_> = g.bando_a.iter().map(|x| m.nombre_pueblo(*x)).collect();
            let b: Vec<_> = g.bando_b.iter().map(|x| m.nombre_pueblo(*x)).collect();
            t += &format!("  {} contra {} (desde el año {})\n", a.join(", "), b.join(", "), g.inicio / DIAS_ANIO + 1);
        }
    }
    let mut grandes = vivos.clone();
    grandes.sort_by(|a, b| b.pob.partial_cmp(&a.pob).unwrap());
    t += "Mayores: ";
    t += &grandes.iter().take(4).map(|p| format!("{} ({:.0})", p.nombre, p.pob)).collect::<Vec<_>>().join(", ");
    t
}

fn texto_fisica(m: &Mundo) -> String {
    let Some(id) = m.jugador.pueblo else {
        return "Todavía no vivís en ningún pueblo: no sabés qué creen de la física.".into();
    };
    const NIVELES: [&str; 7] = ["a ciegas", "intuición", "aproximado", "medido", "preciso", "muy preciso", "exacto"];
    let mut t = format!("Lo que {} cree de las leyes de ESTE universo (y lo que es de verdad):\n", m.nombre_pueblo(id));
    for l in m.ver_fisica(id) {
        if !l.conoce && l.obs == 0 {
            t += &format!("  {}: ni se lo plantean.\n", l.nombre);
            continue;
        }
        t += &format!(
            "  {}: creen {:.3} {u}, es {:.3} {u} ({}, {} observaciones)\n",
            l.nombre, l.creido, l.verdad, NIVELES[(l.nivel as usize).min(6)], l.obs, u = l.unidad
        );
    }
    let p = &m.pueblos[&id];
    if p.radiacion > 0.01 {
        t += &format!("Radiación en el pueblo: {:.0}%.\n", p.radiacion * 100.0);
    }
    let arsenal = p.arsenal;
    if arsenal > 0 {
        t += &format!("Arsenal nuclear: {arsenal}.\n");
    }
    if m.invierno > 0.05 {
        t += &format!("Invierno nuclear en el mundo: {:.0}%.\n", m.invierno * 100.0);
    }
    t += &format!("Refugios en el mundo: {}.", m.refugios.len());
    t
}

fn texto_cronica(m: &Mundo) -> String {
    let hechos = m.cronica.ultimos(14, 1);
    if hechos.is_empty() {
        return "Todavía no pasó nada digno de contarse.".into();
    }
    hechos.iter().rev().map(|h| format!("Año {}: {}", h.dia / DIAS_ANIO + 1, h.texto)).collect::<Vec<_>>().join("\n")
}

fn panel_texto(
    time: Res<Time>,
    mut civ: ResMut<Civ>,
    mut textos: Query<&mut Text, With<PanelTexto>>,
    mut acciones: Query<&mut Node, With<PanelAcciones>>,
    mut botones: Query<(&BotonPestania, &mut BackgroundColor)>,
) {
    if !civ.panel {
        return;
    }
    civ.refrescar -= time.delta_secs();
    if civ.refrescar > 0.0 {
        return;
    }
    civ.refrescar = 0.5;
    let Some(m) = civ.mundo.as_ref() else { return };
    let cuerpo = match civ.pestania {
        Pestania::Pueblo => texto_pueblo(m),
        Pestania::Mundo => texto_mundo(m),
        Pestania::Cronica => texto_cronica(m),
        Pestania::Fisica => texto_fisica(m),
        Pestania::Acciones => {
            let base = texto_pueblo(m);
            let primera = base.lines().next().unwrap_or("").to_string();
            let mut t = format!("{primera}\nLo que hagas entra por las mismas puertas que las de cualquiera: el mundo responde con sus reglas.\nLas leyes sólo las puede cambiar quien manda.");
            if !civ.mensaje.is_empty() {
                t += &format!("\n\n{}", civ.mensaje);
            }
            t
        }
    };
    for mut t in &mut textos {
        if t.0 != cuerpo {
            t.0 = cuerpo.clone();
        }
    }
    for mut n in &mut acciones {
        n.display = if civ.pestania == Pestania::Acciones { Display::Flex } else { Display::None };
    }
    for (p, mut bg) in &mut botones {
        *bg = BackgroundColor(if p.0 == civ.pestania { BOTON_ON } else { BOTON });
    }
}

/// QA de dev: `HA_CIVSHOT=<prefijo>` (con `HA_AUTOPLAY=1`) adelanta la civilización
/// `HA_CIVDIAS` días (200 por defecto), abre la vista de dios y captura cada pestaña.
fn civ_autoshot(
    mut civ: ResMut<Civ>,
    mut editor: ResMut<UiEditorState>,
    mut commands: Commands,
    mut exit: EventWriter<bevy::app::AppExit>,
    mut cuadros: Local<u32>,
) {
    let Ok(prefijo) = crate::flags::var("HA_CIVSHOT") else { return };
    if civ.mundo.is_none() {
        return;
    }
    *cuadros += 1;
    let n = *cuadros;
    if n == 30 {
        let dias: u64 = crate::flags::var("HA_CIVDIAS").ok().and_then(|v| v.parse().ok()).unwrap_or(200);
        let m = civ.mundo.as_mut().unwrap();
        let t0 = std::time::Instant::now();
        m.avanzar(dias);
        if m.jugador.pueblo.is_none() {
            if let Some(id) = m.pueblos.values().filter(|p| p.vivo && p.cuerpo == 0).max_by(|a, b| a.pob.partial_cmp(&b.pob).unwrap()).map(|p| p.id) {
                m.entrar_jugador(id, "Tester");
            }
        }
        eprintln!("ha-civshot: {dias} días en {:?}; {} hechos en la crónica", t0.elapsed(), m.cronica.hechos.len());
        editor.menu_open = true;
        civ.panel = true;
    }
    let capturas = [(60, Pestania::Pueblo, "pueblo"), (100, Pestania::Mundo, "mundo"), (140, Pestania::Cronica, "cronica"), (180, Pestania::Acciones, "acciones")];
    for (cuadro, pest, nombre) in capturas {
        if n == cuadro - 15 {
            civ.pestania = pest;
            civ.refrescar = 0.0;
        }
        if n == cuadro {
            commands
                .spawn(bevy::render::view::screenshot::Screenshot::primary_window())
                .observe(bevy::render::view::screenshot::save_to_disk(format!("{prefijo}-{nombre}.png")));
        }
    }
    if n >= 230 {
        exit.send(bevy::app::AppExit::Success);
    }
}
