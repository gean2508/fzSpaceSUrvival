//! **GENERADO por `scripts/import-casas-medidas.py` — no se edita a mano.**
//!
//! Celdas (ancho, alto) de cada dibujo de casa del pack, por programa de lote. La sim edifica el rectángulo
//! que mide el dibujo ([`crate::city::huella_de_dibujo`]) y el cliente dibuja ESA variante, sin estirarla.

pub const CASAS_DEL_PACK: &[(&str, &[(i64, i64)])] = &[
    ("building/casa_chica", &[(12, 15), (12, 16), (13, 10)]),
    ("building/casa_mediana", &[(15, 14), (16, 14)]),
    ("building/casa_grande", &[(18, 16)]),
    ("building/casa_country", &[(18, 22), (23, 14)]),
    ("building/villa", &[(9, 13)]),
    ("building/duplex", &[(16, 18)]),
];
