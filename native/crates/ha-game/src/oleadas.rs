//! **Oleadas de zombis** (inspirado en el modo Zombis de Call of Duty: Black Ops 2).
//!
//! Cada noche de horda (`infectados::es_horda`) es una partida por rondas:
//!
//! * **Ronda N** trae `total_de_ronda(N)` infectados, que salen en **hordas** de 2 a 5
//!   desde un mismo rumbo, cada `cadencia_de_ronda(N)` segundos. Cuanto más alta la ronda,
//!   más salen y más seguido.
//! * **Tres clases** (`Clase`): el común, el **corredor** (desde la ronda 2, corre más) y el
//!   **tanque** (desde la ronda 5, aguanta el doble y pega más fuerte). Igual que en BO2, las
//!   clases raras aparecen primero en proporción baja y crecen con la ronda.
//! * **Puntos** por cada baja (`puntos_de`): más por clase y con un plus por ronda, como el
//!   marcador de BO2. Se muestran en pantalla con la ronda y los que quedan vivos.
//! * **Pausa entre rondas**: cuando se limpia la ronda, 12 s de respiro con el aviso
//!   «Ronda N completa». Es el momento de recargar, curarte y mirar la base.
//!
//! **Lo que le agregamos a BO2 (el plus):** la horda no sale de cualquier lado: entra
//! como grupo desde un mismo rumbo del anillo de aparición (`infectados::ANILLO_*`), así que
//! se la ve venir y se puede esperar en una puerta. Y el sistema vive en el mundo abierto
//! de la noche de horda, sin cambiar de pantalla.
//!
//! Costo: estado en un `Resource` de números. El spawn reusa `infectados` y el HUD es un
//! solo nodo de texto. Fuera de la superficie o de día, todo se reinicia.

use bevy::prelude::*;

use crate::game_phase::GamePhase;
use crate::input::TouchLayout;

/// Clase de infectado. Define velocidad, vida y lo que vale la baja.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Clase {
    Comun,
    Corredor,
    Tanque,
}

impl Clase {
    /// Multiplicador de velocidad sobre la base de `infectados` (`VEL`).
    pub fn velocidad(self) -> f32 {
        match self {
            Clase::Comun => 1.0,
            Clase::Corredor => 1.6,
            Clase::Tanque => 0.7,
        }
    }

    /// Multiplicador de vida sobre `infectados::VIDA`.
    pub fn vida(self) -> f32 {
        match self {
            Clase::Comun => 1.0,
            Clase::Corredor => 0.7,
            Clase::Tanque => 2.2,
        }
    }
}

/// Cuántos infectados trae la ronda `n` (1-based). 6, 10, 14… tope 60.
pub fn total_de_ronda(n: u32) -> u32 {
    (6 + 4 * n.max(1).saturating_sub(1)).min(60)
}

/// Segundos entre hordas: baja con la ronda, con piso para que no sea una metralleta.
pub fn cadencia_de_ronda(n: u32) -> f32 {
    (2.2 - 0.12 * n.max(1) as f32).max(0.7)
}

/// Cuántos salen juntos en cada horda: 2 en la primera, hasta 5.
pub fn tamano_de_horda(n: u32, sorteo: f32) -> u32 {
    let base = 2 + (n.max(1) - 1).min(3);
    // Un poco de azar: a veces una horda menos grande, nunca más de 5.
    if sorteo < 0.25 { base.saturating_sub(1).max(2) } else { base }
}

/// Clase de un infectado que sale en la ronda `n`, con un sorteo `r` en [0, 1).
pub fn clase_de(n: u32, r: f32) -> Clase {
    if n >= 5 && r < 0.10 {
        Clase::Tanque
    } else if n >= 2 && r < 0.30 {
        Clase::Corredor
    } else {
        Clase::Comun
    }
}

/// Lo que vale una baja: 50 el común, 70 el corredor, 120 el tanque, y +10 por ronda
/// después de la primera (con tope). Igual que en BO2, pegar fuerte paga más.
pub fn puntos_de(clase: Clase, ronda: u32) -> u32 {
    let base = match clase {
        Clase::Comun => 50,
        Clase::Corredor => 70,
        Clase::Tanque => 120,
    };
    base + 10 * ronda.saturating_sub(1).min(20)
}

/// Estado de la partida por rondas. Vive mientras dure la noche de horda.
#[derive(Resource, Default, Debug, Clone)]
pub struct Oleadas {
    /// Hay una partida en curso (es de noche de horda y estás en la superficie).
    pub activa: bool,
    /// Ronda en curso (1-based). 0 = no empezó.
    pub ronda: u32,
    /// Infectados que todavía tienen que salir en esta ronda.
    pub por_salir: u32,
    /// Segundos de pausa entre rondas (cuenta regresiva para «Ronda N» o el aviso).
    pub pausa: f32,
    /// Segundos hasta la próxima horda de la ronda.
    pub reloj_horda: f32,
    /// Puntos acumulados en la partida (de `Bajas::puntos`).
    pub puntos: u32,
    /// Segundos que le quedan al aviso grande «Ronda N».
    pub aviso: f32,
}

/// Lo que hace el sistema con cada paso de tiempo.
#[derive(Debug, PartialEq, Eq)]
pub enum Paso {
    /// Nada que hacer.
    Quieto,
    /// Hay que sacar `n` infectados de una horda ahora.
    Horda(u32),
    /// Empezó una ronda nueva.
    RondaNueva(u32),
    /// Se limpió la ronda; pausa.
    RondaLimpia(u32),
}

pub const PAUSA_ENTRE_RONDAS: f32 = 12.0;
pub const AVISO_S: f32 = 3.0;

impl Oleadas {
    /// Avanza la partida `dt` segundos. `de_noche` = es noche de horda y estás afuera.
    /// `vivos` = infectados de rondas (no guardias) que siguen en pie.
    /// `sorteo` = un número en [0,1) para el tamaño de la horda.
    pub fn paso(&mut self, dt: f32, de_noche: bool, vivos: u32, sorteo: f32) -> Paso {
        if !de_noche {
            // Amaneció o salió de la superficie: se termina la partida.
            *self = Oleadas { puntos: self.puntos, ..Oleadas::default() };
            return Paso::Quieto;
        }
        if !self.activa {
            self.activa = true;
            self.ronda = 1;
            self.por_salir = total_de_ronda(1);
            self.pausa = AVISO_S;
            self.reloj_horda = 0.0;
            self.aviso = AVISO_S;
            return Paso::RondaNueva(1);
        }
        self.aviso = (self.aviso - dt).max(0.0);
        if self.pausa > 0.0 {
            self.pausa -= dt;
            if self.pausa <= 0.0 {
                self.reloj_horda = 0.0;
            }
            return Paso::Quieto;
        }
        if self.por_salir == 0 {
            if vivos == 0 {
                // Ronda limpia: pausa y la siguiente.
                let limpia = self.ronda;
                self.ronda += 1;
                self.por_salir = total_de_ronda(self.ronda);
                self.pausa = PAUSA_ENTRE_RONDAS;
                self.aviso = AVISO_S;
                return Paso::RondaLimpia(limpia);
            }
            return Paso::Quieto;
        }
        self.reloj_horda -= dt;
        if self.reloj_horda > 0.0 {
            return Paso::Quieto;
        }
        self.reloj_horda = cadencia_de_ronda(self.ronda);
        let n = tamano_de_horda(self.ronda, sorteo).min(self.por_salir);
        self.por_salir -= n;
        Paso::Horda(n)
    }

    /// Texto del HUD: la ronda, lo que queda y los puntos.
    pub fn texto(&self, vivos: u32) -> String {
        if !self.activa {
            return String::new();
        }
        let pausa = if self.pausa > 0.0 {
            format!("  ·  próxima ronda en {:.0} s", self.pausa.ceil())
        } else {
            String::new()
        };
        format!(
            "RONDA {}  ·  zombis {}  ·  quedan {}  ·  {} pts{}",
            self.ronda,
            vivos,
            self.por_salir + vivos,
            self.puntos,
            pausa
        )
    }
}

/// Marca del texto de HUD de las oleadas.
#[derive(Component)]
pub struct OleadasHud;

/// Marca del aviso grande «RONDA N».
#[derive(Component)]
pub struct OleadasAviso;

pub struct OleadasPlugin;

impl Plugin for OleadasPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Oleadas>().add_systems(Update, pintar_hud.run_if(in_state(GamePhase::Playing))).add_systems(OnExit(GamePhase::Playing), limpiar_hud);
    }
}

fn limpiar_hud(mut commands: Commands, q: Query<Entity, Or<(With<OleadasHud>, With<OleadasAviso>)>>, mut o: ResMut<Oleadas>) {
    for e in &q {
        commands.entity(e).despawn_recursive();
    }
    *o = Oleadas::default();
}

/// Mantiene el texto de la ronda arriba a la derecha, debajo del reloj, y el aviso grande.
fn pintar_hud(
    mut commands: Commands,
    layout: Res<TouchLayout>,
    o: Res<Oleadas>,
    vivos: Query<&crate::infectados::Infectado>,
    mut hud: Query<(Entity, &mut Text), With<OleadasHud>>,
    mut aviso: Query<(Entity, &mut Text), (With<OleadasAviso>, Without<OleadasHud>)>,
) {
    let vivos_n = vivos.iter().filter(|i| !i.guardia).count() as u32;
    let texto = o.texto(vivos_n);
    let k = crate::ui_pack::k(&layout);
    if texto.is_empty() {
        for (e, _) in &hud {
            commands.entity(e).despawn_recursive();
        }
    } else if let Ok((_, mut t)) = hud.get_single_mut() {
        if t.0 != texto {
            t.0 = texto.clone();
        }
    } else {
        commands.spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(layout.insets.right as f32 + 12.0),
                top: Val::Px(layout.insets.top as f32 + 96.0 * k),
                ..default()
            },
            GlobalZIndex(60),
            OleadasHud,
            Text::new(texto.clone()),
            TextFont { font_size: (18.0 * k).max(11.0), ..default() },
            TextColor(Color::srgb(0.98, 0.93, 0.80)),
            Name::new("OleadasHud"),
        ));
    }

    // Aviso grande al empezar una ronda o al limpiarla.
    if o.activa && o.aviso > 0.0 {
        let frase = if o.pausa > 0.0 && o.ronda > 1 {
            format!("RONDA {} COMPLETA", o.ronda - 1)
        } else {
            format!("RONDA {}", o.ronda)
        };
        if let Ok((_, mut t)) = aviso.get_single_mut() {
            if t.0 != frase {
                t.0 = frase;
            }
        } else {
            commands.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    right: Val::Px(0.0),
                    top: Val::Px(layout.height * 0.30),
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                GlobalZIndex(70),
                OleadasAviso,
                Text::new(frase),
                TextFont { font_size: (56.0 * k).max(26.0), ..default() },
                TextColor(Color::srgb(0.98, 0.93, 0.80)),
                Name::new("OleadasAviso"),
            ));
        }
    } else {
        for (e, _) in &aviso {
            commands.entity(e).despawn_recursive();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_ronda_crece_y_tiene_tope() {
        assert_eq!(total_de_ronda(1), 6);
        assert_eq!(total_de_ronda(2), 10);
        assert!(total_de_ronda(3) > total_de_ronda(2));
        assert_eq!(total_de_ronda(100), 60);
    }

    #[test]
    fn la_cadencia_baja_pero_no_pasa_del_piso() {
        assert!(cadencia_de_ronda(5) < cadencia_de_ronda(1));
        assert!(cadencia_de_ronda(200) >= 0.7);
    }

    #[test]
    fn las_hordas_van_de_2_a_5() {
        for n in 1..=20 {
            for s in [0.0, 0.3, 0.99] {
                let t = tamano_de_horda(n, s);
                assert!((2..=5).contains(&t), "ronda {n} sorteo {s} dio {t}");
            }
        }
    }

    #[test]
    fn las_clases_raras_aparecen_cuando_toca() {
        assert_eq!(clase_de(1, 0.0), Clase::Comun);
        assert_eq!(clase_de(2, 0.1), Clase::Corredor);
        assert_eq!(clase_de(4, 0.05), Clase::Corredor);
        assert_eq!(clase_de(5, 0.05), Clase::Tanque);
        assert_eq!(clase_de(9, 0.5), Clase::Comun);
    }

    #[test]
    fn el_tanque_paga_mas_y_la_ronda_suma() {
        assert!(puntos_de(Clase::Tanque, 1) > puntos_de(Clase::Corredor, 1));
        assert!(puntos_de(Clase::Comun, 3) > puntos_de(Clase::Comun, 1));
        assert_eq!(puntos_de(Clase::Comun, 1), 50);
    }

    #[test]
    fn la_partida_recorre_rondas_y_pausas() {
        let mut o = Oleadas::default();
        assert_eq!(o.paso(0.1, true, 0, 0.5), Paso::RondaNueva(1));
        // Aviso y pausa inicial: no sale nadie todavía.
        // (el aviso dura AVISO_S = 3 s: 25 cuadros de 0,1 s quedan dentro)
        for _ in 0..25 {
            assert_eq!(o.paso(0.1, true, 0, 0.5), Paso::Quieto);
        }
        // Ahora salen hordas hasta vaciar la ronda 1.
        let mut salieron = 0;
        for _ in 0..2000 {
            if let Paso::Horda(n) = o.paso(0.1, true, 0, 0.5) {
                salieron += n;
            }
            if o.por_salir == 0 {
                break;
            }
        }
        assert_eq!(salieron, total_de_ronda(1));
        // Con vivos en pie no hay pausa todavía.
        assert_eq!(o.paso(0.1, true, 3, 0.5), Paso::Quieto);
        // Limpia la ronda: pausa y ronda 2.
        assert_eq!(o.paso(0.1, true, 0, 0.5), Paso::RondaLimpia(1));
        assert_eq!(o.ronda, 2);
        assert!((o.pausa - PAUSA_ENTRE_RONDAS).abs() < 1e-6);
    }

    #[test]
    fn amanecer_corta_la_partida_y_conserva_los_puntos() {
        let mut o = Oleadas { activa: true, ronda: 3, puntos: 420, ..Oleadas::default() };
        assert_eq!(o.paso(0.1, false, 0, 0.5), Paso::Quieto);
        assert!(!o.activa);
        assert_eq!(o.ronda, 0);
        assert_eq!(o.puntos, 420);
    }
}
