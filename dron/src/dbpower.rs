//! **Perfil de física del DBPOWER FPV** (placa `UD-U818H`, familia UDI U818A).
//!
//! Aparte del quad de persecución (`sim3d.rs` sección "EL APARATO CONCRETO")
//! por la regla del contrato: son dos cuerpos distintos, dos bancos de prueba
//! con roles distintos (ver CLAUDE.md 7.1). Este archivo no reemplaza nada, es
//! una segunda fuente de constantes — **todavía no está conectado al lazo de
//! vuelo**. Conectarlo (parametrizar `Dron3`/`Motores` en vez de usar los
//! `const` de `sim3d.rs` a secas) es trabajo aparte, más grande, y pendiente.
//!
//! ## Procedencia de cada número, y cuánto confiar en él
//!
//! Tres números que PARECÍAN contradecirse y en realidad se reconcilian:
//!
//! ```text
//!   balanza del usuario: aparato SIN carcasa, SIN batería     93 g
//!   búsqueda web #2:      dron completo (con carcasa) SIN batería   117 g
//!   → la carcasa real pesaría 117 − 93 = 24 g (no 15 g, la estimación a ojo)
//!   + batería (búsqueda, 15-16 g)                            ≈ 133 g total
//! ```
//!
//! Eso coincide con la búsqueda web #2 completa (133 g) y deja a la web #1
//! (158.6 g "de despegue con batería") como la que sobra — probablemente habla
//! de otra revisión del mismo producto genérico (UDI U818A tiene varias). El
//! aporte real del usuario acá fue avisar que su 93 g NO incluía la carcasa;
//! sin esa aclaración, 93+15(estimado)=108 parecía "cerca" de 133 y no lo era
//! por la razón correcta.
//!
//! **Sigue sin pesarse el aparato exacto, completo, con la batería puesta.**
//! 133 g es la mejor reconstrucción posible con lo que hay, no una medición.
//!
//! Lo que SÍ se usa con confianza es lo geométrico, porque coincide entre la
//! medición propia y la búsqueda: hélices ~15 cm (147 mm) y wheelbase, que el
//! usuario no había medido y la búsqueda da en 245 mm diagonal.

/// Masa de despegue, en kg. **PROVISORIO — ver el comentario de arriba.**
/// Reconstruida (93 g sin carcasa del usuario + 24 g de carcasa inferida +
/// 15-16 g de batería, todo de fuentes distintas) más que medida. Mucho más
/// confiable que antes de la reconciliación, pero sigue sin ser un pesaje
/// directo del aparato completo.
pub const MASA: f32 = 0.133;

/// Diámetro de hélice, en metros. Coincide entre la medición del usuario
/// (~15 cm) y la búsqueda (147 mm) — dato de confianza.
pub const DIAMETRO_HELICE: f32 = 0.147;

/// Distancia entre motores opuestos (M1↔M3 o M2↔M4), en metros. No estaba
/// medida; la búsqueda la da en 245 mm de diagonal. Único dato nuevo de
/// verdad que esta sesión aportó al perfil.
pub const WHEELBASE: f32 = 0.245;

/// Relación empuje/peso a potencia máxima. **PROVISORIO, y peor que la masa**:
/// es una cifra de la FAMILIA U818A en general (motores brushed MD0820 con
/// reductora piñón-corona), no una medición de esta unidad concreta. Sirve
/// para saber el ORDEN de magnitud (bajo, ~1.3-1.4×, muy lejos del 4.0× del
/// quad de persecución) pero no para calibrar un controlador.
pub const EMPUJE_PESO: f32 = 1.35;

/// Motores de escobillas con reductora (piñón-corona), no brushless directo.
/// Consecuencia física real, no un detalle: la reductora agrega inercia y
/// juego mecánico que un motor brushless directo no tiene, y la dinámica de
/// arranque/parada de un motor a escobillas es más lenta. El modelo de
/// `Motores` en `sim3d.rs` (retardo de primer orden + no linealidad cuadrática)
/// se calibró para brushless; puede no ser válido tal cual acá. Marcado como
/// pregunta abierta, no como bug.
pub const MOTOR_CON_REDUCTORA: bool = true;

/// Velocidad máxima reportada en calma, m/s (28 km/h de la búsqueda — sin
/// verificar). Sirve como techo de referencia: si un cálculo con las
/// constantes de arriba da mucho más que esto, alguna constante está mal.
pub const VELOCIDAD_MAX_REPORTADA: f32 = 7.8;

/// Peso legal límite (250 g) bajo el que cae este aparato en la mayoría de
/// regulaciones — dato de contexto, no de física.
pub const LIMITE_REGULATORIO_KG: f32 = 0.250;
