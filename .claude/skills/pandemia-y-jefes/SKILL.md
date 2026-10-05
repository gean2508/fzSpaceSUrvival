---
name: pandemia-y-jefes
description: OBLIGATORIO antes de tocar infectados, la pandemia, jefes de foco, la policía a pie o el tránsito marítimo. Fija cómo se estructura una amenaza de mundo abierto con historia (descubrir, propagar, detener) en vez de rondas, con focos, jefes que escalan por jugadores, terreno enfermo por tinte y una cura de varios pasos. Sirve para replicarlo en otro proyecto Bevy/ECS.
---

# Pandemia en mundo abierto (Ki)

**No son rondas.** Black Ops 2 Zombies es *la sensación* (matar a lo loco, esquivar, que te sigan); el mundo abierto pide una historia.

## Estructura (`ha-game/src/pandemia.rs`)
1. **Etapas guardadas** (`etapa::*`): rumor → brote (paciente cero) → muestra → analizada → alfas caídos → antídoto → curada. Un `Resource` serializable (`pandemia.json` por partida).
2. **Descubrir**: nada sale hasta que existe el virus. El paciente cero (rol 1) suelta `virus_sample`.
3. **Propagar**: `avance` 0-100 sube por día; un **foco** nuevo por día cerca del anterior; los infectados salen alrededor de focos (`infectados::aparecer`), cupo `2 + avance/8` (+2 por jugador).
4. **Jefes de foco** (rol 3): vida × jugadores conectados → coop.
5. **Terreno enfermo sin arte nuevo**: mancha (degradado compartido tintado) + tinte RGB de `SurfaceDecor` cerca de un foco, a 1 Hz. El alfa lo manda otro sistema: no se toca.
6. **Cura**: laboratorio = comisaría (`city::comisaria_cercana`), alfas, antídoto, foco principal. Final secreto por rapidez.

## Reglas
- Roles en `Infectado.rol` (0 común, 1 paciente cero, 2 alfa, 3 jefe); clases en `oleadas.rs`.
- Todo cupo medido; sin foco a la vista no sale nadie (salvo avance ≥ 60).
- Probar en el celu con `debug.ha.pandemia 1` (arranca con virus descubierto y 2 focos) y `debug.ha.dayphase 0.02` (noche).
- Policía a pie (`policia_pie.rs`): sale de la comisaría más cercana; mismas paredes que el jugador.
- Lanchas: `Quien::Navegante` + `ha_shared::vehiculos::navegar` (con prueba de no tocar la costa).
