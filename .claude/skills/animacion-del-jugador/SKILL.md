---
name: animacion-del-jugador
description: Cómo se anima el personaje según lo que HACE (golpear, juntar, comer, beber, disparar, recibir daño, nadar) y cuando NO hace nada (celular, libro, sentarse). Usar antes de agregar una acción nueva al jugador, una animación, un objeto en la mano, o de tocar `pose_jugador.rs`, `sim::face_player_sprite` o `chargen_index::CHAR_ANIMS`. Sirve para replicar el sistema en otro proyecto con hojas de LimeZu (o cualquier hoja por filas).
---

# El personaje hace lo que hace

**El error que corrige:** tener las animaciones en la hoja y no usarlas. Hasta el 3 oct 2026 el jugador tenía 21
animaciones registradas y usaba tres (respirar, caminar, nadar): talaba parado, comía sin mover un dedo y, quieto,
respiraba para siempre. Un personaje que no acusa lo que hace se siente roto aunque la simulación ande.

**Referencias:** Stardew Valley (cada herramienta mueve el cuerpo; comer se ve de frente con el objeto en la mano),
Project Zomboid (quieto un rato, el personaje se entretiene: se nota que está vivo) y Minecraft (un toque = un golpe
visible, aunque no pase nada).

## La arquitectura (copiala tal cual)

1. **Un recurso de acción con prioridad y vencimiento** (`AccionJugador`): `hacer(verbo, segundos)`,
   `hacer_hacia(verbo, s, desde, hacia)` para mirar al blanco, `golpe(item, …)`. El daño (3) pisa al golpe (2), el golpe
   a juntar/comer (1). La acción vence sola: nadie tiene que acordarse de apagarla.
2. **Observadores, no acoples.** Lo que ya existía se OBSERVA (deltas de vida/hambre/sed, `HoldInteractState`, el
   retroceso del arma, los `PlaySfxAt(Place)` cerca del jugador), igual que `audio.rs` saca los sonidos. Sólo se
   tocan los sistemas donde la señal no existe afuera (el golpe a infectados y animales: una línea
   `pose.golpe(...)`).
3. **Un verbo final con orden de mando** (`verbo_del_cuerpo`, función pura y testeada):
   agua > acción > caminar > arma en la mano > reposo > respirar.
4. **La dirección la pone otro** (`sim::face_player_sprite`): diagonal → cardinal → espejo → el mismo verbo de frente
   (filas de una sola dirección) → respirar. Nunca queda sin sprite.
5. **Reposo** (`Reposo`): 9 s quieto → celular (saca 0-2, usa 3-8) o libro, alternando; 40 s → sentado en el piso.
   Cualquier movimiento, acción, vuelo o nado lo despierta.
6. **Lo que la hoja no trae, como capa hija** en la MISMA celda (32×64) y arrancando en el mismo cuadro. Si dura lo
   mismo que la fila del cuerpo (o un múltiplo), queda sincronizada sin código extra.

## Números medidos (no los cambies a ojo)

| cosa | valor | de dónde |
|---|---|---|
| grilla real del pack de 32 | **16 px ×2** (cada píxel = bloque 2×2) | medido sobre `Bodies/Body_32x32_01.png` |
| manos en el pecho (fila 7) | fila 22 de 16; mentón 21 | ídem |
| cabeza que sube 1 px | cuadros 2, 3, 8, 9 de la fila 7 | bbox por cuadro |
| celular | fila 6: saca 0-2, usa 3-8, guarda 9-11 | `Spritesheet_animations_GUIDE.png` («4-9 loop») |
| leer | fila 7, 12 cuadros, libro en `Books/` | ídem (antes mal llamada «phone_intro») |
| comer/beber | `hands_chest` 6 cuadros a 8 fps × 2 = capa de 12 a 8 fps = 1,5 s | test `cada_verbo_que_pide_el_jugador_existe_en_la_hoja` |
| nadar | `walk` cortado a 32×40, brazada a 0,6× del paso | `art2d::registrar_poses_de_personaje` |
| contorno de lo que va en la mano | (58,58,80) | el del libro del pack |

## Dónde está

- `native/crates/ha-game/src/pose_jugador.rs` — acción, reposo, observadores, capas hijas, tests.
- `native/crates/ha-game/src/sim.rs::face_player_sprite` — dirección, cadena de respaldo, tamaño al nadar.
- `native/crates/ha-assets/src/chargen_index.rs::CHAR_ANIMS` — filas, tramos (`first`), fps.
- `native/crates/ha-game/src/art2d.rs::registrar_poses_de_personaje` — poses derivadas + `swim_*`.
- `scripts/dibujar-acciones-jugador.py` — capas `player/en_mano` (comer, beber), `player/ondas`, `player/libro`.
  `--ver <png>` deja la lámina de control sobre el cuerpo real del pack.
- biblia §8 «El jugador HACE lo que hace».

## Agregar una acción nueva

1. ¿La fila existe en `CHAR_ANIMS`? Mirá la guía del pack antes de dibujar nada.
2. Buscá la SEÑAL afuera (un delta, un recurso, un sonido con posición). Si no existe, una línea
   `ResMut<AccionJugador>` en el sistema que la produce.
3. Duración = cuadros / fps de la fila (que se vea entera) salvo los mantenidos (`MANTENIDO_S`, se renueva).
4. Si lleva un objeto que el cuerpo no trae: capa dibujada con la skill `pixel-art`, a 16 ×2, en la celda del cuerpo,
   con la misma cantidad de cuadros (o múltiplo) y los mismos fps.
5. Agregá el verbo al test `cada_verbo_que_pide_el_jugador_existe_en_la_hoja`.

## Cómo se prueba

- `cargo test -p ha-game --lib pose_jugador` y `cargo test -p ha-assets chargen_index`.
- En la compu: `HA_POSETEST=1 HA_AUTOPLAY=1 HA_SHOTDIR=<dir> ./ha-game` — 14 poses, dos capturas cada una, y una
  línea `ha-posetest: <nombre> pose=… escala_x=… rot=…` por captura (si dos capturas de una pose salen iguales,
  no anima; si `escala_x=-1` con una pose `_left`, volvió el doble espejo).
- En el celu (sólo cuando el usuario avisa): talar, comer una manzana, disparar, quedarse quieto.

## Qué NO hacer

- No pintar a 1 px de 32 lo que va encima del cuerpo: se ve de otro juego.
- No poner `sleep` sin cama: la fila es una cabeza sobre la almohada; sin la cama se ve una cabeza suelta.
- No colgar la animación de un sonido SIN posición (`PlaySfx`): no sabés de quién es. Con posición y alcance, sí.
- No espejar el personaje con `scale.x = -1` si la hoja ya trae el perfil izquierdo: los dos espejos se cancelan y
  camina al oeste mirando al este (pasó hasta el 3 oct 2026). Espejo sólo con `flip_x` y sólo cuando falta el lado.
- No bloquear el movimiento por una animación: si camina, camina (la acción sigue en curso y vuelve al soltar).
