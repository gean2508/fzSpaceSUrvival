//! **El mapa de lo que ya vio, la ruta sobre ese mapa, y el orden de las
//! entregas.**
//!
//! Hasta la bitácora 7.46 el dron era puramente REACTIVO: esquivaba lo que
//! tenía adelante en el momento (`Guiado::consignas_esquivando`, cinco
//! sectores de un cono de 160°) y no recordaba nada. Pasar dos veces por el
//! mismo bosque de postes era descubrirlo dos veces, y un poste que quedaba
//! detrás de otro sólo aparecía cuando ya era tarde.
//!
//! Esto es la capa de arriba del piloto, la que la industria llama
//! *planificación global* (el mapa de ocupación de Skydio, el planner de
//! PX4/ArduPilot con su «object avoidance» sobre un mapa): no toca el cerebro
//! ni el piloto clásico. Le cambia el PUNTO al que vuela: en vez de ir
//! derecho al objetivo, va al próximo vértice de una ruta que rodea lo que ya
//! está en el mapa. El reactivo sigue abajo, para lo que todavía no se vio.
//!
//! **Sin GPS** (contrato §1): el mapa vive en el marco de la odometría —el
//! mismo en el que el dron cree estar—, así que la deriva lo desplaza igual
//! que desplaza al dron, y la ruta sigue siendo coherente con donde el dron
//! cree estar.

/// Un poste en el mapa: dónde, qué tan grueso, y cuántas veces se vio (un
/// poste visto una sola vez puede ser ruido de la cámara).
#[derive(Clone, Copy, Debug)]
pub struct Mapeado {
    pub x: f32,
    pub y: f32,
    pub r: f32,
    pub vistas: u32,
    /// Incertidumbre de la posición (1σ, m) para PLANIFICAR: baja con cada
    /// observación y crece con el tiempo (`envejecer`).
    pub sigma: f32,
    /// **Qué tan bien se sabe dónde está, respecto del punto donde el dron se
    /// prendió** (1σ, m): cada observación pesa según su error de medición MÁS
    /// la duda que tenía el dron sobre sí mismo en ese momento. Un poste visto
    /// al arrancar, cerca de casa, queda casi exacto: es una referencia fija,
    /// como la Tierra para una nave. Uno visto después de un minuto de deriva,
    /// no — y no puede arrastrar al que ya estaba bien anotado.
    pub sigma_abs: f32,
    /// **Ancla**: un poste que se anotó cuando el dron sabía bien dónde
    /// estaba (cerca de donde se prendió). Una vez ancla, su posición queda
    /// FIJA —nada anotado después con deriva lo mueve— y es lo único que se
    /// usa para ubicar al dron. Es la «Tierra» de la nave.
    pub ancla: bool,
}

/// Lo que el dron vio desde que empezó a volar esta sesión.
#[derive(Clone, Debug, Default)]
pub struct Mapa {
    pub obstaculos: Vec<Mapeado>,
}

/// Cuántas veces hay que ver un poste para que cuente para planificar.
pub const VISTAS_MIN: u32 = 3;
/// **Cuántas σ de incertidumbre se suman al margen de un poste.** Sin esto la
/// ruta se metía por huecos que en el mapa medían 1.65 m y en la realidad no
/// alcanzaban. Ida y vuelta de 16 m, clásico, 36 sorteos (bitácora 7.46):
///
/// ```text
///   postes          2    4    6    8   12
///   sólo reactivo  36   32   21   14    6
///   K = 0          ·    ·   29   18    8
///   K = 1.5        ·    ·   30   23   15
///   K = 3          ·    ·   27   29   24
///   K = 4.5       35   34   30   30   26
///   K = 6         36   34   33   27   24
/// ```
///
/// Meseta entre 4.5 y 6: se toma 5.
pub const K_SIGMA: f32 = 5.0;
/// Distancia que la ruta deja entre el dron y el BORDE de un poste mapeado:
/// el radio del aparato (~0.15 m), el error del mapa (~15% de la distancia a
/// la que se vio) y un colchón para la deriva lateral.
pub const MARGEN_RUTA: f32 = 0.7;
/// Cuánto crece por segundo la incertidumbre de un poste mapeado (m/s).
pub const DERIVA_MAPA: f32 = 0.05;
/// Cuántas veces hay que haber visto un poste para usarlo de referencia.
pub const VISTAS_REFERENCIA: u32 = 8;
/// Qué tan bien anotado tiene que estar un poste para ser ancla (1σ, m).
pub const SIGMA_ANCLA: f32 = 0.3;
/// Corrección máxima de la posición por observación (m).
pub const SALTO_MAX: f32 = 0.15;

impl Mapa {
    /// Suma una observación con su incertidumbre `sigma` (1σ en metros: la
    /// cámara da 15% de la distancia, así que una vista de lejos vale mucho
    /// menos que una de cerca). Si cae dentro de la incertidumbre de un poste
    /// que ya estaba, se FUSIONA con pesos 1/σ² —el promedio que haría un
    /// filtro de Kalman—; si no, es un poste nuevo.
    ///
    /// La primera versión fusionaba por distancia fija (0.3 m + radios) y
    /// promediaba sin pesos: con 4 postes reales el visor terminó con 15 en el
    /// mapa, porque el mismo poste visto a 8 m (±1.2 m) caía fuera del radio
    /// de fusión y nacía como fantasma (bitácora 7.46).
    #[cfg(test)]
    pub fn registrar_con(&mut self, x: f32, y: f32, r: f32, sigma: f32) {
        self.registrar_desde(x, y, r, sigma, 0.0);
    }

    /// Igual, sabiendo cuánto dudaba el dron de su propia posición.
    pub fn registrar_desde(&mut self, x: f32, y: f32, r: f32, sigma: f32, sigma_odom: f32) {
        let var_abs = sigma * sigma + sigma_odom * sigma_odom;
        let mut mejor: Option<(usize, f32)> = None;
        for (i, o) in self.obstaculos.iter().enumerate() {
            let d = ((o.x - x).powi(2) + (o.y - y).powi(2)).sqrt();
            let tolerancia = 2.5 * (o.sigma * o.sigma + sigma * sigma).sqrt() + 0.5 * (o.r + r);
            if d < tolerancia && mejor.map_or(true, |(_, md)| d < md) {
                mejor = Some((i, d));
            }
        }
        match mejor {
            Some((i, _)) => {
                let o = &mut self.obstaculos[i];
                if o.ancla {
                    o.vistas += 1;
                    return;
                }
                // Posición: pesos por error ABSOLUTO (medición + deriva propia),
                // así lo anotado temprano no se lo lleva lo anotado tarde.
                let (a0, a1) = (1.0 / (o.sigma_abs * o.sigma_abs), 1.0 / var_abs);
                o.x = (o.x * a0 + x * a1) / (a0 + a1);
                o.y = (o.y * a0 + y * a1) / (a0 + a1);
                let (w0, w1) = (1.0 / (o.sigma * o.sigma), 1.0 / (sigma * sigma));
                o.r = (o.r * w0 + r * w1) / (w0 + w1);
                // Piso de 5 cm: la deriva de la odometría no deja saber mejor.
                o.sigma = (1.0 / (w0 + w1)).sqrt().max(0.05);
                o.sigma_abs = (1.0 / (a0 + a1)).sqrt().max(0.03);
                o.vistas += 1;
                if o.vistas >= VISTAS_REFERENCIA && o.sigma_abs < SIGMA_ANCLA {
                    o.ancla = true;
                }
            }
            None => self.obstaculos.push(Mapeado {
                x, y, r, vistas: 1, sigma, sigma_abs: var_abs.sqrt(), ancla: false,
            }),
        }
    }

    /// **Dónde está el dron, según lo que ya conoce** (SLAM con referencias
    /// naturales, sin marcas ni sensores nuevos). Si la observación `(x, y)`
    /// —calculada con la odometría— cae sobre un poste bien conocido, la
    /// diferencia entre donde el mapa dice que está y donde se lo ve ahora es
    /// el error de la odometría. Devuelve cuánto corregir la posición del dron
    /// y con qué ganancia de Kalman: se le cree a la referencia en proporción
    /// a cuánto más segura es que la odometría. Los postes anotados al
    /// arrancar (cerca de donde se prendió) son casi exactos y mandan; los
    /// anotados tarde casi no corrigen. Ver bitácora 7.48.
    pub fn ubicar(&self, x: f32, y: f32, r: f32, sigma_obs: f32, sigma_odom: f32) -> Option<(f32, f32, f32)> {
        let mut mejor: Option<(&Mapeado, f32)> = None;
        for o in self.obstaculos.iter().filter(|o| o.ancla) {
            let d = ((o.x - x).powi(2) + (o.y - y).powi(2)).sqrt();
            let tol = 2.5 * (o.sigma * o.sigma + sigma_obs * sigma_obs + sigma_odom * sigma_odom).sqrt()
                + 0.5 * (o.r + r);
            if d < tol && mejor.map_or(true, |(_, md)| d < md) {
                mejor = Some((o, d));
            }
        }
        let (o, _) = mejor?;
        let ref2 = o.sigma_abs * o.sigma_abs;
        let s2 = sigma_odom * sigma_odom + ref2 + sigma_obs * sigma_obs;
        // Compuerta de 3σ: si la diferencia es más grande de lo que las dudas
        // permiten, es OTRO poste (o uno mal anotado), no un error propio.
        let innov = ((o.x - x).powi(2) + (o.y - y).powi(2)).sqrt();
        if innov > 3.0 * s2.sqrt() {
            return None;
        }
        let k = sigma_odom * sigma_odom / s2;
        let (mut dx, mut dy) = ((o.x - x) * k, (o.y - y) * k);
        // Nunca un salto grande por una sola observación (regla 3.20).
        let m = (dx * dx + dy * dy).sqrt();
        if m > SALTO_MAX {
            dx *= SALTO_MAX / m;
            dy *= SALTO_MAX / m;
        }
        Some((dx, dy, k))
    }

    /// El ancla más cercana a una observación, si cae dentro de la compuerta.
    pub fn ancla_de(&self, x: f32, y: f32, r: f32, tol: f32) -> Option<(f32, f32)> {
        self.obstaculos.iter()
            .filter(|o| o.ancla)
            .map(|o| (o, ((o.x - x).powi(2) + (o.y - y).powi(2)).sqrt()))
            .filter(|(o, d)| *d < tol + 0.5 * (o.r + r))
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .map(|(o, _)| (o.x, o.y))
    }

    /// **Posición y rumbo a la vez, por triangulación con dos o más anclas.**
    /// `vistos` son los puntos donde el dron ve cada ancla (calculados con su
    /// posición y rumbo creídos) y `anclas` dónde están en el mapa. El giro y
    /// el corrimiento que mejor superponen unos sobre otros (Kabsch en 2D,
    /// alrededor de la posición del dron) SON el error de rumbo y de posición.
    ///
    /// Por qué no con un poste solo: con uno, un error de posición se ve igual
    /// que un error de rumbo, y corregir el rumbo así divergía —30 m de error
    /// al final del recorrido, contra 8 m sin corregir nada (bitácora 7.48)—.
    /// Con dos, la distancia y el ángulo ENTRE ellas no dependen de dónde está
    /// el dron: es como una nave con dos estrellas.
    ///
    /// Devuelve `(dx, dy, dθ)` o `None` si el ajuste no cierra (residuo mayor
    /// a medio metro: alguna asociación está mal).
    pub fn pose_por_anclas(
        pos: (f32, f32), vistos: &[(f32, f32)], anclas: &[(f32, f32)],
    ) -> Option<(f32, f32, f32)> {
        let n = vistos.len();
        if n < 2 {
            return None;
        }
        let pc = vistos.iter().fold((0.0, 0.0), |a, p| (a.0 + p.0 / n as f32, a.1 + p.1 / n as f32));
        let qc = anclas.iter().fold((0.0, 0.0), |a, p| (a.0 + p.0 / n as f32, a.1 + p.1 / n as f32));
        let (mut sxx, mut sxy) = (0.0f32, 0.0f32);
        let mut extension = 0.0f32;
        for (p, q) in vistos.iter().zip(anclas) {
            let (a, b) = (p.0 - pc.0, p.1 - pc.1);
            let (c, d) = (q.0 - qc.0, q.1 - qc.1);
            sxx += a * c + b * d;
            sxy += a * d - b * c;
            extension = extension.max((a * a + b * b).sqrt());
        }
        if extension < 0.75 {
            return None; // anclas demasiado juntas: el ángulo no se puede medir
        }
        let th = sxy.atan2(sxx);
        let (s, c) = (th.sin(), th.cos());
        let rot = |p: (f32, f32)| (c * (p.0 - pc.0) - s * (p.1 - pc.1) + qc.0, s * (p.0 - pc.0) + c * (p.1 - pc.1) + qc.1);
        let mut residuo = 0.0f32;
        for (p, q) in vistos.iter().zip(anclas) {
            let r = rot(*p);
            residuo += (r.0 - q.0).powi(2) + (r.1 - q.1).powi(2);
        }
        if (residuo / n as f32).sqrt() > 0.5 {
            return None;
        }
        let nueva = rot(pos);
        Some((nueva.0 - pos.0, nueva.1 - pos.1, th))
    }

    /// **El mapa envejece.** Sin GPS la odometría deriva (sobre todo el
    /// rumbo, ~0.5°/s), así que un poste visto hace un minuto está, en el
    /// marco del dron, más lejos de donde se lo anotó. La incertidumbre crece
    /// con el tiempo —el «ruido de proceso» de un Kalman— y así una
    /// observación nueva del mismo poste se fusiona con la vieja y la corrige
    /// en vez de nacer como un poste fantasma.
    pub fn envejecer(&mut self, dt: f32) {
        for o in self.obstaculos.iter_mut() {
            o.sigma += DERIVA_MAPA * dt;
        }
    }

    /// Una observación con incertidumbre por defecto (pruebas y usos sin
    /// distancia conocida).
    #[cfg(test)]
    pub fn registrar(&mut self, x: f32, y: f32, r: f32) {
        self.registrar_con(x, y, r, 0.3);
    }

    /// Los postes que cuentan para planificar.
    pub fn confirmados(&self) -> impl Iterator<Item = &Mapeado> {
        self.obstaculos.iter().filter(|o| o.vistas >= VISTAS_MIN)
    }

    /// ¿Se puede ir de `a` a `b` en línea recta sin pasar a menos de
    /// `margen` del borde de ningún poste confirmado?
    pub fn libre(&self, a: (f32, f32), b: (f32, f32), margen: f32) -> bool {
        self.confirmados()
            .all(|o| distancia_a_segmento((o.x, o.y), a, b) > o.r + margen + K_SIGMA * o.sigma)
    }

    /// La ruta más corta de `desde` a `hasta` que rodea lo mapeado: la lista
    /// de vértices (sin `desde`, terminando en `hasta`). Grafo de visibilidad
    /// sobre ocho puntos alrededor de cada poste, y Dijkstra. Si no hay forma
    /// de llegar sin rozar nada, devuelve el tramo directo y el reactivo se
    /// arregla — nunca deja al dron sin punto al que ir.
    pub fn ruta(&self, desde: (f32, f32), hasta: (f32, f32), margen: f32) -> Vec<(f32, f32)> {
        if self.libre(desde, hasta, margen) {
            return vec![hasta];
        }
        let conf: Vec<&Mapeado> = self.confirmados().collect();
        let mut nodos = vec![desde, hasta];
        for o in &conf {
            let rr = (o.r + margen + K_SIGMA * o.sigma) * 1.12;
            for k in 0..8 {
                let a = k as f32 * std::f32::consts::TAU / 8.0;
                let p = (o.x + rr * a.cos(), o.y + rr * a.sin());
                // Un vértice adentro de OTRO poste inflado no sirve.
                if conf.iter().all(|q| ((q.x - p.0).powi(2) + (q.y - p.1).powi(2)).sqrt() > q.r + margen + K_SIGMA * q.sigma) {
                    nodos.push(p);
                }
            }
        }
        let n = nodos.len();
        let mut dist = vec![f32::INFINITY; n];
        let mut previo = vec![usize::MAX; n];
        let mut hecho = vec![false; n];
        dist[0] = 0.0;
        for _ in 0..n {
            let mut u = usize::MAX;
            let mut mejor = f32::INFINITY;
            for i in 0..n {
                if !hecho[i] && dist[i] < mejor {
                    mejor = dist[i];
                    u = i;
                }
            }
            if u == usize::MAX || u == 1 {
                break;
            }
            hecho[u] = true;
            for v in 0..n {
                if hecho[v] || v == u {
                    continue;
                }
                let d = ((nodos[u].0 - nodos[v].0).powi(2) + (nodos[u].1 - nodos[v].1).powi(2)).sqrt();
                if dist[u] + d < dist[v] && self.libre(nodos[u], nodos[v], margen) {
                    dist[v] = dist[u] + d;
                    previo[v] = u;
                }
            }
        }
        if !dist[1].is_finite() {
            return vec![hasta];
        }
        let mut camino = Vec::new();
        let mut i = 1;
        while i != 0 {
            camino.push(nodos[i]);
            i = previo[i];
        }
        camino.reverse();
        camino
    }

    /// El punto al que hay que volar AHORA para ir de `desde` a `hasta`.
    pub fn proximo(&self, desde: (f32, f32), hasta: (f32, f32), margen: f32) -> (f32, f32) {
        let r = self.ruta(desde, hasta, margen);
        // Si el primer vértice está prácticamente encima, el siguiente: si no,
        // el dron se quedaría clavado en un vértice que ya alcanzó.
        for p in &r {
            if ((p.0 - desde.0).powi(2) + (p.1 - desde.1).powi(2)).sqrt() > 0.6 {
                return *p;
            }
        }
        hasta
    }

    /// Largo de la ruta planificada (para ordenar entregas con el mapa puesto).
    pub fn largo(&self, desde: (f32, f32), hasta: (f32, f32), margen: f32) -> f32 {
        let mut total = 0.0;
        let mut a = desde;
        for p in self.ruta(desde, hasta, margen) {
            total += ((p.0 - a.0).powi(2) + (p.1 - a.1).powi(2)).sqrt();
            a = p;
        }
        total
    }
}

fn distancia_a_segmento(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let l2 = dx * dx + dy * dy;
    let t = if l2 < 1e-9 { 0.0 } else { (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / l2).clamp(0.0, 1.0) };
    let (cx, cy) = (a.0 + t * dx, a.1 + t * dy);
    ((p.0 - cx).powi(2) + (p.1 - cy).powi(2)).sqrt()
}

/// Un trabajo de entrega: dónde está la caja y adónde va.
#[derive(Clone, Copy, Debug)]
pub struct Trabajo {
    pub caja: (f32, f32),
    pub destino: (f32, f32),
    pub kg: f32,
}

/// **En qué orden hacer las entregas.** El dron lleva una caja por vez (un
/// imán), así que cada trabajo es caja → destino, y lo único que se elige es
/// el orden. Se minimiza el recorrido total, incluida la vuelta a `base` si
/// la hay. Hasta 7 trabajos se prueban TODOS los órdenes (5040 como mucho,
/// nada para una computadora de a bordo); con más, el más cercano primero.
///
/// Elegir la caja más cercana en cada paso —lo intuitivo— no es óptimo: una
/// caja cerca pero cuyo destino queda lejos de todo lo demás conviene dejarla
/// para el final. Hay una prueba que lo fija. Con más de 7: la más cercana
/// primero y después 2-opt (dar vuelta tramos mientras acorte).
pub fn orden_de_entregas(
    inicio: (f32, f32),
    trabajos: &[Trabajo],
    base: Option<(f32, f32)>,
    dist: &dyn Fn((f32, f32), (f32, f32)) -> f32,
) -> Vec<usize> {
    let n = trabajos.len();
    let costo = |orden: &[usize]| -> f32 {
        let mut c = 0.0;
        let mut a = inicio;
        for &i in orden {
            c += dist(a, trabajos[i].caja) + dist(trabajos[i].caja, trabajos[i].destino);
            a = trabajos[i].destino;
        }
        if let Some(b) = base {
            c += dist(a, b);
        }
        c
    };
    if n <= 7 {
        let mut idx: Vec<usize> = (0..n).collect();
        let mut mejor = idx.clone();
        let mut mejor_c = costo(&idx);
        // Permutaciones por el algoritmo de Heap.
        let mut c = vec![0usize; n];
        let mut i = 0;
        while i < n {
            if c[i] < i {
                if i % 2 == 0 { idx.swap(0, i) } else { idx.swap(c[i], i) }
                let k = costo(&idx);
                if k < mejor_c {
                    mejor_c = k;
                    mejor = idx.clone();
                }
                c[i] += 1;
                i = 0;
            } else {
                c[i] = 0;
                i += 1;
            }
        }
        mejor
    } else {
        let mut quedan: Vec<usize> = (0..n).collect();
        let mut orden = Vec::new();
        let mut a = inicio;
        while !quedan.is_empty() {
            let (k, _) = quedan.iter().enumerate()
                .map(|(k, &i)| (k, dist(a, trabajos[i].caja)))
                .fold((0, f32::INFINITY), |m, x| if x.1 < m.1 { x } else { m });
            let i = quedan.remove(k);
            a = trabajos[i].destino;
            orden.push(i);
        }
        // Mejora local: dar vuelta tramos del orden (2-opt) mientras acorte
        // el recorrido. «La más cercana primero» sola puede dejar la peor
        // caja para el final; esto la corrige sin probar los n! órdenes.
        let mut mejor_c = costo(&orden);
        let mut mejoro = true;
        while mejoro {
            mejoro = false;
            for i in 0..n {
                for j in i + 1..n {
                    let mut o = orden.clone();
                    o[i..=j].reverse();
                    let c = costo(&o);
                    if c + 1e-4 < mejor_c {
                        orden = o;
                        mejor_c = c;
                        mejoro = true;
                    }
                }
            }
        }
        orden
    }
}

pub fn recta(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn con(ps: &[(f32, f32, f32)]) -> Mapa {
        Mapa { obstaculos: ps.iter().map(|&(x, y, r)| Mapeado { x, y, r, vistas: 9, sigma: 0.1, sigma_abs: 0.1, ancla: false }).collect() }
    }

    #[test]
    fn sin_nada_en_el_mapa_va_derecho() {
        assert_eq!(Mapa::default().ruta((0.0, 0.0), (10.0, 0.0), MARGEN_RUTA), vec![(10.0, 0.0)]);
    }

    #[test]
    fn la_ruta_rodea_lo_mapeado_sin_rozarlo() {
        let m = con(&[(5.0, 0.0, 0.6), (8.0, 0.8, 0.5)]);
        let r = m.ruta((0.0, 0.0), (12.0, 0.0), MARGEN_RUTA);
        assert!(r.len() > 1, "tenía que rodear");
        let mut a = (0.0, 0.0);
        for p in &r {
            assert!(m.libre(a, *p, MARGEN_RUTA - 1e-3), "el tramo {a:?}→{p:?} roza un poste");
            a = *p;
        }
        assert_eq!(*r.last().unwrap(), (12.0, 0.0));
    }

    #[test]
    fn un_poste_visto_una_vez_no_cuenta() {
        let mut m = Mapa::default();
        m.registrar(5.0, 0.0, 0.6);
        assert_eq!(m.ruta((0.0, 0.0), (10.0, 0.0), MARGEN_RUTA), vec![(10.0, 0.0)]);
        m.registrar(5.1, 0.05, 0.6);
        m.registrar(4.95, -0.05, 0.6);
        assert_eq!(m.obstaculos.len(), 1, "tres vistas del mismo poste son UN poste");
        assert!(m.ruta((0.0, 0.0), (10.0, 0.0), MARGEN_RUTA).len() > 1);
    }

    /// Dos anclas vistas con el rumbo girado 0.2 rad y la posición corrida:
    /// la triangulación devuelve exactamente el giro y el corrimiento.
    #[test]
    fn la_triangulacion_recupera_posicion_y_rumbo() {
        let anclas = [(4.0f32, 3.0f32), (-3.0, 5.0), (2.0, -4.0)];
        let (verdad, creida, err) = ((1.0f32, 1.0f32), (1.8f32, 0.4f32), 0.2f32);
        // Lo que el dron ve: cada ancla desde la verdad, puesta en su marco
        // creído (posición creída, rumbo girado `err`).
        let vistos: Vec<(f32, f32)> = anclas.iter().map(|q| {
            let (dx, dy) = (q.0 - verdad.0, q.1 - verdad.1);
            let (s, c) = (err.sin(), err.cos());
            (creida.0 + c * dx - s * dy, creida.1 + s * dx + c * dy)
        }).collect();
        let (dx, dy, dth) = Mapa::pose_por_anclas(creida, &vistos, &anclas).unwrap();
        assert!((dth + err).abs() < 1e-3, "giro {dth}, tenía que ser {}", -err);
        assert!((creida.0 + dx - verdad.0).abs() < 1e-3 && (creida.1 + dy - verdad.1).abs() < 1e-3,
            "posición corregida ({}, {})", creida.0 + dx, creida.1 + dy);
    }

    /// Con muchas cajas (sin probar todos los órdenes), la mejora local
    /// nunca deja un recorrido peor que «la más cercana primero».
    #[test]
    fn con_muchas_cajas_la_mejora_local_no_empeora() {
        let mut t = Vec::new();
        let mut x = 12345u32;
        let mut r = || { x ^= x << 13; x ^= x >> 17; x ^= x << 5; (x % 2000) as f32 / 100.0 - 10.0 };
        for _ in 0..10 {
            t.push(Trabajo { caja: (r(), r()), destino: (r(), r()), kg: 0.2 });
        }
        let d = |a, b| recta(a, b);
        let costo = |o: &[usize]| {
            let mut c = 0.0; let mut a = (0.0, 0.0);
            for &i in o { c += recta(a, t[i].caja) + recta(t[i].caja, t[i].destino); a = t[i].destino; }
            c
        };
        // «La más cercana primero» a mano, como referencia.
        let mut quedan: Vec<usize> = (0..10).collect();
        let (mut voraz, mut a) = (Vec::new(), (0.0, 0.0));
        while !quedan.is_empty() {
            let k = (0..quedan.len()).min_by(|&p, &q| recta(a, t[quedan[p]].caja)
                .partial_cmp(&recta(a, t[quedan[q]].caja)).unwrap()).unwrap();
            let i = quedan.remove(k);
            a = t[i].destino;
            voraz.push(i);
        }
        let o = orden_de_entregas((0.0, 0.0), &t, None, &d);
        let mut visto = o.clone(); visto.sort();
        assert_eq!(visto, (0..10).collect::<Vec<_>>(), "todas las cajas, una vez cada una");
        assert!(costo(&o) <= costo(&voraz) + 1e-3);
    }

    /// Lo intuitivo (la caja más cercana primero) no es lo óptimo.
    #[test]
    fn el_orden_optimo_le_gana_al_mas_cercano() {
        let t = [
            // Cerca de la base pero con destino lejísimos, del otro lado.
            Trabajo { caja: (1.0, 0.0), destino: (-20.0, 0.0), kg: 0.2 },
            Trabajo { caja: (5.0, 0.0), destino: (8.0, 0.0), kg: 0.2 },
            Trabajo { caja: (9.0, 0.0), destino: (12.0, 0.0), kg: 0.2 },
        ];
        let d = |a, b| recta(a, b);
        let optimo = orden_de_entregas((0.0, 0.0), &t, None, &d);
        let costo = |o: &[usize]| {
            let mut c = 0.0; let mut a = (0.0, 0.0);
            for &i in o { c += recta(a, t[i].caja) + recta(t[i].caja, t[i].destino); a = t[i].destino; }
            c
        };
        assert!(costo(&optimo) < costo(&[0, 1, 2]), "el óptimo {optimo:?} tiene que ganarle a ir por la más cercana");
        assert_eq!(optimo[2], 0, "la caja cercana de destino lejano va al final");
    }
}
