---
name: disparo-y-apuntado
description: OBLIGATORIO antes de tocar cómo se apunta, se dispara o se siente un impacto (armas.rs, infectados/fauna/society cuando reciben balas, mira, HUD de munición, retroceso, sonidos de tiro). Fija EL LISTÓN de «disparar se siente como Black Ops 2 Zombies» — el jugador elige a quién, el juego garantiza que la bala llegue, y cada bala paga con feedback — con números medidos para el táctil del Redmi, la asistencia de puntería (`apuntado.rs`), la cadena de impacto y cómo probarlo. Sirve igual para cualquier otro proyecto con disparos (twin-stick, top-down, móvil o mando).
---

# disparo-y-apuntado — que matar infectados se sienta como en Tranzit

> El pedido del usuario (3 oct 2026): «sentirme como si estuviera matando zombies en BO2 Tranzit: ahí disparar era
> satisfactorio porque **no luchaba apuntando** ni había **interferencia entre controles**».

Dos frases que mandan sobre todo lo demás:

1. **El jugador decide A QUIÉN le tira; el juego se encarga de que la bala llegue.** La puntería no es el desafío;
   el desafío es la horda, la munición, el espacio y el pánico.
2. **Cada bala paga.** Toda bala que conecta devuelve algo que se ve, se oye y se siente en menos de 50 ms.

## 1. Por qué BO2 Zombies se siente bien (lo que se copia)

| Lo que hacía | Por qué importa | Cómo se traduce acá |
|---|---|---|
| **Aim assist de mando**: *slowdown* (la mira se frena al pasar sobre un zombie) + *snap* al apuntar con L2 + rotación en hip-fire | Con el pulgar en un stick nadie es preciso; la asistencia hace que el pulgar alcance | `apuntado::elegir_blanco` + `rumbo_asistido` (§3) |
| **Zombies lentos y legibles** en rondas 1-5: caminan derecho hacia vos | Apuntar es trivial; el miedo viene de la cantidad, no de la puntería | Infectados que avanzan con rumbo legible; la dificultad sube con número y velocidad, NUNCA con hitbox chico |
| **+10 puntos por cada bala que pega**, +50/60 por baja, +100 a la cabeza, +130 a cuchillo | Recompensa por bala, no sólo por baja: hasta el tiro que no mata «paga» | Un numerito por impacto (o un tic de sonido+flash) — §4 |
| **Sangre, sacudida, extremidades que vuelan, zombies que se arrastran** | El cuerpo reacciona a cada bala: se lee el daño sin mirar barras | Flash blanco + empujón + mancha que queda (§4) |
| **Penetración**: una bala atraviesa la fila del «tren» de zombies | Juntar la horda y vaciarla en línea es la jugada maestra | `ArmaDef::atraviesa`; el AK atraviesa 2, el rayo 3 |
| **Controles separados**: un stick camina, otro apunta, R2 dispara, nada se pisa | Cero «quise disparar y abrí una puerta» | §2: con arma en la mano el mundo es sólo para tirar |
| **Sonido grave y seco por arma**, recarga en tres golpes, el *Pack-a-Punch* que suena a premio | El oído confirma antes que el ojo | Skill `audio-sfx` (capas transitorio + cuerpo + cola) |

## 2. El control en el táctil (lo que NO puede pasar)

El juego dispara apoyando el dedo en el mundo (`armas::apuntar_y_disparar`, `AimCursor`). Tres problemas medidos:

* **El dedo tapa al blanco.** A pie hay **36 px por metro** en el Redmi (`targeting::PICK_FORGIVENESS`) y la yema
  apoya 40-60 px: un infectado (radio de impacto 0,55 m ≈ 40 px de ancho) queda DEBAJO del dedo. → La mira de
  «en rojo» se dibuja **sobre el blanco elegido**, no bajo el dedo; y la bala se dobla hacia él (§3).
* **El mismo toque significa dos cosas.** Tocar el mundo también pica props/árboles/cadáveres (`targeting`,
  `infectados::cadaveres`). → **Con un arma en la mano, tocar el mundo SÓLO dispara.** Abrir/usar va por el botón
  USAR (que con arma dice RECARGAR) o enfundando. Nunca «quise tirar y abrí un cofre».
* **Los joysticks roban el toque** (memoria `touch-ui-stick-zones`): el stick de caminar vive abajo-izquierda; ningún
  botón de combate va en su zona.

Modelo de control, de lo que hay a lo que conviene (en orden; cada paso es jugable solo):

1. **Toque-dispara con asistencia** (lo actual + `apuntado.rs`): el dedo dice la intención, el cono elige el blanco,
   la bala se dobla hasta 9°. Semiautomática = un tiro por toque; automática = mantener.
2. **Botón de fuego con «toque rápido»** (*Brawl Stars*): tocar el botón tira al mejor blanco del cono hacia donde
   mirás; ARRASTRAR desde el botón apunta a mano con la línea de tiro visible y soltar dispara. Es el estándar de
   los shooters móviles exitosos porque el pulgar no tapa la acción.
3. **Fuego automático al tener blanco** (*CoD Mobile «modo simple»*): opción de accesibilidad, apagada por defecto.

Prohibido: mira que persigue al dedo con retraso, disparar al soltar si el arma es automática, que el disparo frene
al personaje (caminar y tirar a la vez es el núcleo del *kiting* de Tranzit), zona muerta axial en cualquier stick.

## 3. Asistencia de puntería — `native/crates/ha-game/src/apuntado.rs`

Matemática pura con tests. Números por defecto (`Asistencia::default()`), medidos para la escala del juego:

| Perilla | Valor | Qué es | Fuente |
|---|---|---|---|
| `cono_grados` | 15° por lado | Candidatos dentro de un cono de 30° | dual-stick: «cono de ~30° evita saltar entre blancos» |
| `lateral_max` | 1,6 m | Error lateral máximo aunque el ángulo entre (lejos el cono se abre de más) | «proyectá la intención a la distancia del blanco, después compará» |
| `dedo_factor` | 1,8 × radio | Dedo a menos de ~1 m de un infectado = lo eligió el jugador, sin discusión | el dedo elige, no el algoritmo |
| `histeresis` | 1,35 | El blanco anterior sigue mientras no sea 35 % peor: la ráfaga no salta entre dos pegados | *sticky aim* |
| `doblez_max` | 9° | Lo máximo que se dobla la bala; más se nota y «el juego apunta por mí» | Halo: *red reticle range*; «sutil o nada» |
| adelanto | intercepción exacta, tope 1,5 s | Le pega a lo que cruza (perros, ciervos) | — |
| primera bala | 25 % de la dispersión | *First-shot accuracy*: la ráfaga abre en 0,4 s, correr +35 % | CoD hip-fire |
| zona muerta | radial **escalada** 0,10-0,25 | Sin escalón en el borde ni imán a los ejes | Sutphin, *Doing thumbstick dead zones right* |

Reglas que no se negocian:

* **Sólo enemigos.** A `elegir_blanco` se le pasan infectados y fauna hostil. Nunca aldeanos: la ayuda no puede
  convertir un tiro errado en un crimen (`society` cobra caro dispararle a alguien).
* **Sólo lo que se ve.** El llamador filtra blancos detrás de pared (`choque_con_el_mundo` origen→blanco).
* **Nunca hacia atrás, nunca fuera del alcance del arma.**
* **La asistencia no cambia el daño ni la cadencia**, sólo el rumbo. La dispersión se aplica DESPUÉS del doblez.

Así está enchufado en `armas::apuntar_y_disparar` (resumen; el código real filtra además por alcance y usa
`a_la_vista`, que parte la línea de vista en tramos de 5 m porque `choque_con_el_mundo` marcha 12 pasos como mucho):

```rust
// 1) Juntar blancos: infectados vivos + fauna hostil, al alcance y a la vista.
let blancos: Vec<apuntado::Blanco> = infectados.iter()
    .filter(|(_, i)| i.vida > 0.0)
    .map(|(e, i)| apuntado::Blanco { id: e.to_bits(), pos: Vec2::new(i.x, i.z), vel: Vec2::ZERO, radio: 0.55 })
    // (Infectado no guarda velocidad: caminan lento y el adelanto no hace falta; la fauna sí la pasa.)
    .filter(|b| a_la_vista(pos, b.pos, body, tiles.z, &tiles, &walls))
    .collect();
// 2) Elegir con histéresis (estado.blanco: Option<u64> vive en EstadoArmas).
let cfg = apuntado::Asistencia::default();
let i = apuntado::elegir_blanco(pos, dir, punto, def.alcance, &blancos, estado.blanco, &cfg);
estado.blanco = i.map(|i| blancos[i].id);
// 3) Rumbo de salida + dispersión de ráfaga.
let dir = apuntado::rumbo_asistido(pos, dir, i.map(|i| &blancos[i]), def.vel, &cfg);
let disp = apuntado::dispersion_efectiva(def.dispersion, estado.rafaga_s, corriendo);
// 4) estado.mira_en = (posición, en_rojo): visual_del_arma dibuja MiraDeArma SOBRE el blanco y apunta el arma ahí.
```

Costo: el cono recorre los blancos UNA vez por cuadro con arma en mano (decenas, no miles); nada se asigna si no hay
arma. Cumple la regla de los 60 fps del Redmi.

## 4. La cadena de impacto — «cada bala paga»

Orden y tiempos (medidos en estudios de *hit feedback*: vibración+sonido a 0 ms, flash ~20 ms, partículas ~50 ms,
texto ~100 ms):

| t | Qué | Dónde en el código | Notas |
|---|---|---|---|
| 0 ms | **Sonido de impacto** (carne ≠ pared ≠ rayo) + vibración corta (Android, 15-25 ms) | `PlaySfxAt(b.impacto)` en `balas_a_*` | El de pared ya existe (`BulletWall`) |
| 0-20 ms | **Flash** del sprite: 1-2 cuadros BLANCOS y después el rojo | módulo dueño (`infectados::herir` ya pone `destello = 0.15` rojo) | El efecto de mejor valor por esfuerzo; el blanco primero se lee como «conecté» |
| 0 ms | **Empujón** del blanco en el sentido de la bala (0,15-0,3 m, más con escopeta) | `herir(..., desde, empuje)` | Ya hay empuje (las balas pasan 0,25); escalarlo por arma |
| 20-50 ms | **Chispa/sangre** con color por material | `armas::chispa_de_impacto` | 4 partículas alcanzan |
| ~100 ms | **«+10»** o tic de puntos sobre el blanco; al morir «+50/+100» | HUD | Lo que BO2 tenía en vez de *hitmarker* |
| baja | **Hitstop** 30-60 ms SÓLO en la baja (nunca por bala: con subfusil congela el juego) + cadáver que queda | `infectados` | 0,1-0,2 s es el óptimo para golpes; en tiros va más corto |
| siempre | **Lo que queda**: casquillos, mancha, cadáver (*permanence*, Vlambeer) | — | Con cupo: los casquillos se reciclan |

Del lado del que dispara (*The Art of Screenshake*, Jan Willem Nijman): fogonazo (hecho), **patada de cámara**
hacia atrás 4-8 px que vuelve rápido (no sólo sacudida aleatoria), el arma recula en la mano (`kick`, hecho),
sacudida que decae rápido y escalada por arma (`retroceso`), bala grande y rápida que se vea (las estelas, hecho).

## 5. El enemigo tiene que dejarse matar lindo

* **Tiempo hasta la baja**: 2-4 balas de pistola en la primera hora (`ArmaDef::dano` vs 12 de vida). Esponjas de
  balas matan la sensación más que cualquier control.
* **Legible**: el infectado avanza en línea, se tambalea al recibir, telegrafía el golpe (`ATAQUE_RADIO`).
* **La dificultad sube por cantidad, velocidad y flanqueo**, nunca achicando el hitbox ni con zig-zag aleatorio.
* **Punto débil** (equivalente a la cabeza en top-down): bala que pega en los primeros 0,3 s de un tambaleo, o
  desde < 3 m, hace ×1,5 y suena distinto. Premia la calma, no la precisión de píxel.

## 6. Cómo se prueba (sin oído y sin celu, y después en el celu)

* `apuntado.rs` tiene tests puros. Corren con el resto (`cargo test -p ha-game --lib apuntado`) y también sueltos sin
  cargo (sirve con el disco lleno):
  `rustc --edition 2021 --test main.rs --extern bevy=libbevy.rlib` con un shim mínimo de `Vec2` (ver el commit que
  agregó la skill).
* **Protocolo de sensación** en el Redmi (regla 12): 30 infectados a 8-20 m; contar en 60 s balas tiradas vs balas
  que pegaron. Listón: **≥ 70 % de acierto con pistola sin intentarlo especialmente** y CERO veces «toqué para tirar
  y pasó otra cosa». Si el acierto pasa del 95 % a 25 m, la asistencia está muy fuerte: bajar `doblez_max`.
* Grabar la pantalla y mirar cuadro a cuadro: el flash del blanco tiene que aparecer en el cuadro siguiente al tiro.

## 7. Lo que NO se hace

* Auto-aim que gira al personaje solo o elige blanco sin que el jugador apunte (eso es otro juego: *Archero*).
* Hitbox más grande que el sprite en el **daño** recibido por el jugador (la generosidad va de él hacia la horda).
* Sacudida de cámara continua en ráfaga (marea en el celu): por disparo, corta, con tope de trauma.
* Ayuda contra aldeanos o animales domésticos.

## Fuentes

* Nick Weihs (Insomniac), GDC 2013 — *Techniques for Building Aim Assist in Console Shooters*:
  https://www.gdcvault.com/play/1017942/Techniques-for-Building-Aim-Assist
* Halopedia — Aim assist (auto-aim/magnetismo, fricción, *red reticle*): https://www.halopedia.org/Aim_assist
* *Everything I learned about dual-stick shooter controls* (Game Developer):
  https://gamedeveloper.com/design/everything-i-learned-about-dual-stick-shooter-controls
* Josh Sutphin — *Doing thumbstick dead zones right*:
  https://www.gamedeveloper.com/disciplines/doing-thumbstick-dead-zones-right
* CoD: tipos de asistencia (slowdown, rotación, precisión, focusing): https://www.ggrecon.com/guides/mw3-aim-assist-types/
* Jan Willem Nijman (Vlambeer) — *The Art of Screenshake*; resumen: https://rpgplayground.com/research-making-a-juicy-game/
* Hit feedback (hitstop 0,1-0,2 s, flash, orden de efectos):
  https://uhiyama-lab.com/en/notes/unity/unity-game-feel-hit-feedback/
