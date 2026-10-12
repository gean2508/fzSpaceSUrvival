---
name: dron
description: Trabajar en el proyecto dron (dron simulado en Rust pilotado por la red binaria evolucionada IAFZ-64, sin GPS, meta hardware real). Usar al tocar cualquier cosa en dron/: simulador, Guiado, Estabilizador3, genomas, entrenamiento, visor o mediciones.
---

# Skill: dron

Esta skill es un indice. Todo el conocimiento vive en dos archivos de esta carpeta,
que funcionan con cualquier herramienta o a mano, no solo con Claude:

1. **`CLAUDE.md`** (el contrato): que se construye, reglas, infraestructura, mapa del
   codigo y estado vigente (seccion 7). Leerlo ENTERO antes de tocar nada.
2. **`BITACORA.md`** (el registro): que se midio en cada sesion, que dio, que se
   descarto. Leer la entrada del area que vas a tocar antes de empezar.
   Las mediciones ya hechas no se repiten.

## Resumen de lo que no se negocia (detalle en CLAUDE.md seccion 3)

- **3.1** La red nunca toca los motores: emite consignas de navegacion.
  Cadena: `Guiado` (pid3d.rs) -> `Estabilizador3` -> `Motores`.
- **3.2** La vara honesta es sagrada: `Guiado::consignas` es el piloto clasico contra el
  que se mide el genoma. No se toca para maquillar numeros. Solo se arregla si tiene un
  bug real de fisica. Si la red no le gana, se dice.
- La red no pilota, corrige a quien pilota: con la red en cero, el dron vuela igual que el clasico.
- Sin GPS: posicion por flujo optico + giroscopo; obstaculos y objetivos se ven con camara frontal.

## Como se trabaja (CLAUDE.md seccion 5)

- Hacer, no narrar: reportar al final con numeros.
- Diagnosticar antes de tocar: instrumentar, mirar la traza, hallar la causa.
- Antes de mirar el visor, verificar piloto (RED vs CLASICO), objetivo/partida, mision y
  `md5sum genomes/DRON3D_64.adn` (el bloque de comandos esta en CLAUDE.md 5).
- Experimentos cortos en copia del repo; corridas largas NO en /tmp (tmpfs, se vacia al reiniciar).
- Si CLAUDE.md queda viejo, se actualiza en el mismo cambio.

## Comandos utiles

```bash
cargo build --release
./target/release/dron                  # lista completa de comandos
./target/release/dron visor            # visor 3D en :8080
./target/release/dron sorteos 14 clasico n=24
./target/release/dron estres 14
./target/release/dron capacidades
```

## Mapa rapido

`src/pid3d.rs` (Guiado, Estabilizador3), `src/iafz.rs` (red), `src/sim3d.rs`/`gym3d.rs`
(simulacion y entrenamiento), `src/mision.rs`, `src/web.rs` + `visor.html`/`cabina.html`
(visor), `genomes/` (resultados de experimentos), `respaldo/` (copias).
