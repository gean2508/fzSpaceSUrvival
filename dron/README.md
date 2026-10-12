# dron

Un dron simulado en Rust pilotado por una red binaria evolucionada (IAFZ-64,
11 KB, pensada para un chip de clase ESP32), **sin GPS**, con la meta de volar
en hardware real.

**La red no pilota: corrige a quien pilota.** Un piloto clásico basado en el
modelo físico (`Guiado`) vuela; la red suma una corrección encima. Con la red
en cero, el dron vuela exactamente como el clásico — y ése es el patrón contra
el que se mide todo (la «vara honesta»).

Sin GPS: la posición sale de flujo óptico + giróscopo, que derivan, como en un
dron de juguete de verdad. Los obstáculos y los objetivos se VEN con una cámara
frontal (looming), no se geolocalizan.

## Por dónde empezar

- **`CLAUDE.md`** — el contrato: qué se construye, las reglas y el estado de
  hoy (sección 7). Leerlo antes de tocar nada.
- **`BITACORA.md`** — el registro: qué se midió en cada sesión, qué dio y qué
  se descartó.

## Uso

```bash
cargo build --release
./target/release/dron visor                      # visor 3D en :8080 (lo levanta el servicio dron-visor)
./target/release/dron entrenarmision 1200 semilla=1 salida=RUTA
./target/release/dron sorteos 14 clasico n=24    # ¿la vara aprueba este peldaño?
./target/release/dron estres 14                  # la vara bajo averías reales
./target/release/dron capacidades                # qué sabe hacer, en castellano
./target/release/dron mision 14 0 detalle        # los 9 casos detrás de una aptitud
./target/release/dron                            # la lista completa de comandos
```

El visor: `http://localhost:8080` (y `/cabina` para manejar la moto y ver cómo
te sigue el dron).
