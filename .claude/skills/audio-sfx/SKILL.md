---
name: audio-sfx
description: OBLIGATORIO antes de crear, cambiar o agregar CUALQUIER sonido o música del juego (efectos, pasos, interfaz, ambiente, pistas, la splash). Fija EL LISTÓN de calidad del diseño de sonido (motor v2 de `ha-audio`, 2 oct 2026) — capas con transitorio + cuerpo + cola, ruido con color y filtro, material = timbre, sonoridad pareja, nada de pitidos de onda cruda — y el laboratorio (`scripts/sfx-lab.py`) que mide y dibuja cada sonido para auditarlo sin oído. Fija también cómo se COMPONE la música (Minecraft/C418 y Project Zomboid como referencia de oficio). Usar también al revisar sonido existente.
---

# El sonido del juego: nunca más un pitido

> Pedido del usuario (2 oct 2026): *«mejorá los sonidos de todo el juego… y dejalo como skill: lo mejor, lo nuevo»*.
> Esta skill es lo mínimo. Todo sonido nuevo tiene que estar **al nivel de los de `ha-audio/src/defaults.rs`, o mejor**.

## Qué era «de pitido»
Un efecto era UNA onda cruda (cuadrada, triangular) de 40-100 ms con un envolvente lineal: un `beep` a 22 kHz, aliasing
metálico, sin cuerpo ni cola, a un volumen cualquiera. Hacía que picar, abrir una puerta y recoger un ítem sonaran a
lo mismo y a juguete. Lo que lo arregla no es un buen número: es **estructura**.

## Las reglas (todas, siempre)
1. **Capas: transitorio + cuerpo + cola.** Un efecto nunca es un solo oscilador.
   - *Transitorio* (5-40 ms): lo que define el material al oído — `Pluck` (madera, golpe seco), ruido **rosa** filtrado
     (tela, hojas, arena), ruido blanco sólo en el chasquido del instante cero.
   - *Cuerpo*: `Sine` con glissando exponencial (`f0 → f1`, el motor barre en escala logarítmica), FM (`fm_ratio`/`fm_index`)
     para madera/vidrio/burbujas, `Bell` para metal y cristal, `Saw` desafinada (`detune` 12-30 cent) + filtro para
     voces y puertas.
   - *Cola* (opcional): `Reverb` corto (`size` 0,2-0,4) en el mundo; largo (0,8+) en el subsuelo.
2. **El ruido tiene color y filtro.** `Pink` para aire/tela/arena/agua; `Brown` para golpes, tierra, retumbes;
   `Noise` (blanco) casi nunca. Siempre con `cutoff0 → cutoff1` (el barrido es lo que lo hace «shhh» o «thud»).
3. **Material = timbre.** Madera = `Pluck` grave, `curve` 2,5-3,5. Piedra = golpe marrón + chasquido rosa agudo corto.
   Metal/cristal = `Bell` (parciales inarmónicos) o FM. Tela = rosa con ataque lento. Agua = FM + marrón filtrado.
   Cuerpo vivo (mordida, comer) = ráfagas rosas filtradas + golpe sinusoidal grave.
4. **Caída con forma.** `curve` ≥ 3 para golpes secos, 1,3-1,8 para sonidos que se apagan, `sustain` > 0 para colchones.
   El ataque nunca es 0 (el motor ya suaviza 1 ms de entrada y 4 ms de salida: no hay clicks).
5. **Sonoridad pareja con `NormalizeRms`** al final de cada efecto, por categoría: `RMS_UI` 0,10 · `RMS_MUNDO` 0,14 ·
   `RMS_IMPACTO` 0,19 · pasos 0,07-0,09 · ambiente 0,06. Así un click no se pierde y un derrumbe no revienta.
6. **Variación, no metralleta.** Lo que se repite (pasos, picar) lleva dos o más variantes (`step_hard1/2`) y el juego
   le aplica ±5 % de velocidad al reproducir. Una superficie = su propio paso (pasto, arena, nieve, madera, piedra).
7. **El silencio también suena.** El subsuelo tiene ambiente por bioma (gotas, cristales, derrumbes, ecos de mena):
   `audio::ambiente_subsuelo`. Un lugar sin sonido ambiente se siente roto.
8. **Música generativa con timbre real**: pad de sierras desafinadas bajo un filtro lento (`PadSpec.detune/cutoff*`),
   melodía de `Pluck`/`Bell`/FM, acordes sueltos (`chord_prob`), coro + reverb + compresor + `NormalizeRms` 0,08-0,09.

## La música: se COMPONE, no se tira al azar (motor v3)
Referencias de OFICIO (no se copia ninguna melodía): **Minecraft** — C418 (minimalismo ambiental, armonía modal, piano de
fieltro con reverb, motivos de 3-5 notas que vuelven con variaciones, muchísimo silencio, nada de batería; «Sweden»,
«Wet Hands», «Living Mice») y Lena Raine/Aaron Cherof (colchones ambientales, piano, cuevas misteriosas); **Project Zomboid**
(Zach Beever) — piano, cuerdas y guitarra melancólicos, colchones oscuros que crecen despacio, séptimas y acordes suspendidos
para el miedo de la noche. Fuentes: Wikipedia (*Minecraft – Volume Alpha*, *Music of Minecraft*), vgmonline, craftdex y el blog de Zomboid.
- Una pista es una `ComposeSpec` (`ha-audio/src/compose.rs`): modo de 7 grados, `progression` de grados, `bass`/`arp`/`lead`/`pad`
  como `Voice` (timbre + octava + duración en tiempos), `arp_pattern`, `motif_len`/`motif_repeat`, `lead_phrase` (probabilidad de
  que una frase tenga melodía: el resto es SILENCIO), `seventh`/`add9`/`sus` para adornar los acordes, `humanize`/`dynamics`.
- **Un modo por clima**: jónico/lidio = día abierto; dórico = melancólico cálido; eólico/frigio = noche; locrio = tensión.
- **Instrumentos** (`Wave`): `Piano` (fieltro, 6 parciales, 2 cuerdas), `Guitar` (fingerstyle), `Bell` (cristal/cueva), FM seno
  = piano eléctrico, `Saw` desafinada con filtro lento = cuerdas/colchón, `Sine` grave = sub.
- Cadena de estudio: `Highpass → Chorus → Reverb (0,8-0,97) → Compress → NormalizeRms (0,07-0,09) → FadeOut`.
- **Contextos** (`TrackSpec::mood`): `surface` (día), `night`, `underground`, `cristal` (bioma), `space`, `tension` (infectados
  a menos de 28 celdas cortan lo que suene, con 150 s de enfriamiento), `menu` (en bucle en el menú). Mínimo una pista por contexto
  (lo vigila `el_director_tiene_musica_para_cada_contexto`).
- **Costo**: cada pista de ~2 min tarda ~2 s en la laptop y bastante más en el celu, así que **no se sintetiza en el boot**:
  `audio::MusicBank` compone UNA a la vez en un hilo y la guarda en `<datos>/musica/<nombre>-<hash>.wav`. Subir `MUSICA_VERSION`
  si cambia cómo suena una pista; cambiar la partitura cambia el hash solo.

## El laboratorio (obligatorio al terminar)
```sh
scripts/regenerar-audio.sh --sheet /tmp/sfx.png   # regenera audio.json + splash.wav y audita todo
python3 scripts/sfx-lab.py --sheet /tmp/sfx.png   # sólo auditar el spec actual (o --json otro.json)
```
- Mide por sonido: duración, pico, **RMS de la parte activa**, cresta, centroide espectral, continua; y **falla** si hay
  clipping, silencio, RMS fuera de la banda de su categoría, continua, o «es un pitido» (una sola línea espectral).
- `--sheet` dibuja el espectrograma de cada sonido: **mirá la lámina con `Read`**. Un sonido bueno muestra un
  transitorio ancho, un cuerpo que cae y una cola; una línea horizontal fina es un pitido. Los de referencia (`click`,
  `chop`, `craft`, `discover`, `touchdown`) son el listón.
- `cargo test -p ha-audio`: finitos, deterministas, sonoridad pareja, filtro estable, JSON viejo legible.

## Dónde se toca
- Diseño: `native/crates/ha-audio/src/defaults.rs` (un `put("clave", sfx(...))` por efecto). Motor: `ha-audio/src/lib.rs`.
- Un efecto nuevo = (1) `put` en `defaults.rs`, (2) variante en `ha-game/src/audio.rs` (`Sfx`, `key()`, `ALL`),
  (3) quien lo dispare (`PlaySfx` sin posición / `PlaySfxAt` con posición de render), (4) `scripts/sfx-lab.py` lo
  categoriza por nombre (agregalo a la lista de UI/ambiente si corresponde), (5) correr `regenerar-audio.sh`.
- El editor de sonido (`./build-y-publicar.sh sonido`) edita el mismo JSON; los campos avanzados están en «motor v2».
- La splash es un WAV (`assets/audio/splash.wav`) generado por `splash_logo`: sus «plops» caen cada 0,135 s desde 0,06 s
  y el acorde a 1,22 s; respetá esos tiempos o se desfasa de las manchas (`boot_ui::RETARDO_AUDIO`).

## Lo que NO se acepta
- una capa única de `Square`/`Triangle` cruda como efecto; ruido blanco sin filtro; ningún `NormalizeRms`;
- tonos sin glissando ni caída con forma; ataques instantáneos; reverb en TODO (embarra) o en nada;
- un sonido nuevo que no pase `sfx-lab.py`; un efecto sin variante cuando se repite seguido;
- agregar audio como archivo si se puede sintetizar (el APK no pesa; todo sale de `audio.json` al arrancar).

## Cómo medir el costo
`render_all` dice cuánto tarda en sintetizar (≈3 s en la laptop para 41 efectos + 5 pistas de ~1 min). En el celu
se paga en el boot: si sube, bajar `MUSIC_RATE`/duración de pistas antes que recortar capas de los efectos.
