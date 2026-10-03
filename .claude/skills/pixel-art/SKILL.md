---
name: pixel-art
description: OBLIGATORIO antes de crear, dibujar o generar CUALQUIER gráfico del juego (PNG, sprite, icono, tile, retrato, elemento de UI). Fija el estilo único —la grilla, la paleta, el sombreado, la sombra de contacto— medido sobre el pack de LimeZu, para que todo lo que se dibuje de acá en adelante se vea del mismo juego. Fija también EL LISTÓN de calidad (§0.0): nada de elipses/rectángulos planos «de Paint»; volumen con `scripts/lib/volumen.py` y comparación contra `scripts/galeria-arte.py`. Usar también al revisar arte existente o al decidir si un slot necesita pixel nuevo.
---

# El estilo del juego es UNO y está medido

Todo lo que se dibuje para Ki tiene que poder ponerse al lado de un asset de
LimeZu sin que se note cuál es cuál. Esto no es una guía de gusto: son números
sacados de medir 250-400 singles de 32x32 de Modern Exteriors y las 168 páginas
del pack que el juego dibuja hoy (75,9 millones de píxeles).

## 0.0 EL LISTÓN (2 oct 2026): nunca más «hecho en Paint»

> El usuario lo pidió así, textual: *«que nunca más hagan feo y que solo hagan como tú hiciste como
> mínimo o mejor»*. Esta sección manda sobre todo lo demás de la skill cuando se dibuja una pieza.

**Qué era «Paint».** Cada pieza era una elipse o un rectángulo rellenos con tres tonos: simétrica, de bordes
parejos, sin grano, sin brillo, a veces con la luz invertida. Pasaba el corsé de `lib/pixel.py` (paleta,
alfa, dithering) y aun así se veía de otro juego, porque **cumplir las reglas de color no da forma ni volumen**.

**Qué hay que hacer SIEMPRE** (y cómo, con las funciones que ya existen):
1. **Mirar la lámina de referencia antes de dibujar**: `python3 scripts/galeria-arte.py /tmp/g.png` y `Read`.
   Son las 87 piezas que fijan el nivel (comida, minerales, materias, granja, mundo, interfaz, máquinas). Tu pieza
   tiene que poder ponerse en esa lámina sin que se note el cambio de oficio. **Ese es el listón, no un número.**
2. **Silueta propia e irregular**: `lib.volumen.blob(...)` (elipse perturbada con ruido de bajo orden y semilla
   propia) o un polígono pensado. Nada de elipses ni rectángulos perfectos ni formas simétricas de plantilla.
3. **Volumen por cúpula iluminada**: `lib.volumen.esculpir(l, mask, rampa(base, 5), ...)` saca una cúpula de la
   silueta, la ilumina **desde arriba-izquierda**, la cuantiza a bandas de la rampa del pack y las junta en
   manchas (filtro de moda). Cinco tonos, no tres. El lado de abajo-derecha es el oscuro.
4. **Textura real del pack como relieve**: `textura=` (la roca `tiles/roca` en piedra, vetas de corteza en madera…)
   con `fuerza_textura` 0,35-0,6; las superficies grandes siguen yendo por `lib/materiales.py`.
5. **Detalle que la distingue**: granos de mineral con luz y sombra (`motas`), brillo puntual de 1-2 píxeles
   (blanco o tono más claro de la rampa) en el lado de la luz, juntas, nudos, costillas, marcas. Una pieza sin un
   solo detalle no está terminada.
6. **Familia correcta**: ítems de mochila/iconos = contorno de 1 px azul-violeta de TODA la silueta
   (`lib.iconos.silueta`); objetos del mundo = sin contorno, sombra de contacto al pie (`Lienzo.sombra_al_pie`) y
   apoyados al ras (`verificar(apoyado=True)`).
7. **Pasar el piso automático y mirar el resultado ampliado AL LADO de la lámina**:
   `lib.volumen.exigir_liston(im, nombre)` (corta si tiene menos de 4 tonos con peso, la luz viene de abajo-derecha
   o es una plantilla simétrica de pocos tonos) y `python3 scripts/auditar-arte.py` (audita todo `art/gen`).
   ⚠ **El piso mide lo peor, no lo bueno**: que pase no alcanza. Lo que decide es la lámina.

**Dónde están las recetas modelo** (copiá su forma de resolver cada cosa en vez de inventar otra):
`scripts/lib/volumen.py` (blob, esculpir, motas, el piso) · `scripts/lib/iconos.py` (bultos/menas, lingotes,
ladrillo, esquirla, sal, gota, comida, tronco, palos, frascos, hachas, picos, antorcha, escudo) ·
`scripts/lib/granja_v2.py` (suelo arado que repite sin costura, cultivos, huevo, hueso, cubos, corazón) ·
`scripts/lib/mundo_v2.py` (cactus, rocas, juncos, pozo, cofre, estantes, taxi) · `scripts/lib/ui_v2.py` (pestañas,
globo, máquinas) · `scripts/dibujar-minerales.py` (veta + cristales facetados). Cada `dibujar-*.py` publica su hoja
con `lib.pixel.publicar`; **un script nuevo se suma a la lista y al ⚠ de orden de scripts de la biblia §18**.

**Lo que NO se acepta** (si lo ves en una pieza, está mal aunque `verificar()` pase):
- una elipse/rectángulo relleno con una rampa corta; una forma simétrica izquierda-derecha de punta a punta;
- la luz de abajo o de la derecha; el borde de arriba-izquierda más oscuro que el de abajo-derecha;
- cero grano y cero brillo; tonos que cambian de a un escalón sin ninguna mancha menor (se ve «plastificado»);
- contorno de un solo color en una pieza del mundo, o ausencia de contorno en un ítem de mochila;
- pegar un mismo patrón en damero (dithering) para «dar textura»: la textura sale del pack, no de un damero.

**El proceso de entrega no cambia** (§3-§4), pero ahora el paso de «mirarla» es obligatorio contra la lámina, y
**la pieza vieja que se encuentre por debajo del listón se rehace, no se deja**.

## 0.1 REEMPLAZAR un asset del pack: el juez (3 oct 2026)

> Pedido: *«dibujar todos los assets para tener control total… pero solo si están a la altura o los superan»*.
> Respaldo de la skill anterior: `.claude/respaldos/pixel-art-2026-10-03/` y el tag `respaldo/pixel-art-skill-2026-10-03`.

Un asset comprado **sólo** se saca por uno nuestro que pase el juez:

```sh
python3 scripts/juez-arte.py nueva.png --contra pack1.png [pack2.png] --lamina /tmp/ciega.png
```

Mide **por celda de 32x32** (variedad, la celda más pobre p10, plano, bordes, tonos, salto de luz, colores,
luz, huecos, escala) y da `SUPERA` / `A LA ALTURA` / `POR DEBAJO`. La lámina es **a ciegas**: mirala y adiviná
cuál es la dibujada ANTES de abrir `.respuesta.json`; si se adivina, no reemplaza. Medido el 3 oct: los ítems de
una celda ya están casi a la par (salto de luz 27,5 contra 30,4 del pack), pero usan menos colores (7 contra 9) y
tonos (6 contra 7) → **subí a 7-9 colores**. El taxi dibujado quedó POR DEBAJO por escala (un cuarto de un auto del pack).

## 0.2 Edificios: a la medida de la sim, con el pixel del pack (`scripts/lib/edificio.py`)

Las casas «rotas» (interior que asoma, puerta dibujada fuera de la celda `Door`, fachada estirada ×1,4) vienen de
que el pack dibuja cada casa de un tamaño fijo. Dibujar casas desde cero NO llega al nivel del pack (tejas,
revestimiento, alféizares). Lo que sí llega: **re-componer el pixel del pack a la medida exacta**.

- `redimensionar(im, ancho, proteger_x=aberturas(im))`: reparte el cambio entre tiras de columnas idénticas
  (pesadas por largo²); puertas y ventanas (detectadas por el vidrio: g>r+14, b>g+5) no se tocan.
- `fachada_a_medida(modulos, ancho_px)`: combina módulos del pack lado a lado (como las casas adosadas), cada uno
  re-medido a lo sumo ×1,3, y **cada candidata pasa por el juez**; devuelve la primera A LA ALTURA o se planta.
  Con `Terraced_House_Side_View_Base_1/2`: entran 8-11, 17-23 y 25-30 celdas; 7, 12-16 y 24 no.
- `mover_franja`: lleva la puerta a la columna de la sim. `apilar(techo, fachada)`.
- ⚠ Descartado: el camino óptimo por grafo de columnas (las bahías del pack coinciden al 77 % como mucho y el
  camino repetía UNA columna; el juez lo aprobaba porque salteaba celdas vacías → ahora mide `huecos`).

**Falta** (próximo paso): un importador que use `fachada_a_medida` con la huella de `city::edificado_de_lote`
y la puerta de `city::puerta_del_frente`, y más familias de módulos para cubrir los anchos que hoy no entran.

## 0. Antes de dibujar: ¿de verdad falta?

> ⚠ **EL ERROR MÁS CARO DE ESTA SKILL, Y YA PASÓ.** El 14 ago 2026 se dibujaron
> 132 piezas para tapar slots vacíos. **Cuarenta y seis no hacían falta**: el arte
> del pack no sólo existía, **ya estaba importado en el manifest**. Se dibujó
> `vehicle/bus` teniendo `vehicle/colectivo_abajo`; `agua/estrella_mar` teniendo
> seis estrellas de mar en dos tamaños; `furniture/inodoro` teniendo los 180
> muebles de baño de Modern Interiors.
>
> Dos causas, y las dos se evitan con un comando:
>
> 1. **Se buscó en inglés.** El importador masivo **traduce los nombres al
>    castellano**: el autobús es `colectivo`, el muelle es `muelle`, el cardumen
>    es `pesca`. Buscar `bus`, `dock` o `fish` no encuentra nada.
> 2. **Se buscó sólo en `PRUEBAS-CODIGOS/`**, que son los PNG crudos, y no en el
>    **manifest**, que es donde está lo que el juego ya puede dibujar (13.413
>    claves).
>
> Un slot vacío casi nunca significa "falta pixel". Significa **"nadie dijo cuál
> de las 13.413 claves le toca"**.

**El pixel comprado gana siempre.** El orden no se negocia:

0. **Buscá en el MANIFEST, en castellano y en inglés.** Es lo primero y lo más
   barato:

   ```sh
   python3 scripts/ver-claves.py boya bote muelle    # lámina de lo ya importado
   ```

   Si aparece, no hay nada que importar ni dibujar: se apunta el slot canónico a
   esa clave en `scripts/asignar-del-pack.py`.
1. **Buscalo en los packs crudos.** Son tres y están en `PRUEBAS-CODIGOS/`: Modern
   Exteriors (6.141 singles + 26 autotiles, con nombres descriptivos), Modern
   Interiors (muebles, comida, personajes, UI — **numerados por tema**, hay que
   mirarlos) y Modern User Interface.

   ```sh
   python3 scripts/ver-pack.py "*Yard_Toy*"          # lámina de los PNG crudos
   ```

   "Están numerados y mirarlos cuesta horas" fue justamente la excusa que llevó al
   error de arriba. Cuesta un minuto y la lámina lleva el nombre debajo.
2. **Derivalo de lo que ya está en uso.** Teñir, anclar por luminancia o recortar
   una tira de un asset asignado no es inventar: es
   `atlas-suelo.py::derivar` y `import-acantilado.py::mapa_por_losa`. Así se
   consiguieron la nieve, la sabana, la arena, el regolito y el basalto.
3. **Adaptá lo que el juego pide.** Si el pack no sabe dibujar una pared de diez
   pisos, la pregunta legítima es si el juego necesita paredes de diez pisos.
4. **Recién ahí, dibujá el PNG.** Con la librería de abajo, que hace cumplir el
   estilo por construcción.

Para saber qué falta de verdad, medilo — no lo supongas:

```sh
cargo test -p ha-game inventario_de_arte -- --ignored --nocapture
```

Recorre el catálogo canónico (`ha_assets::game_slots`) y dice, por categoría, qué
claves no tienen una sola entrada en el manifest. **Ojo con leerlo mal**: ese test
dice qué slot está vacío, NO que el pixel no exista. Casi siempre existe con otro
nombre.

## 0.5 Dos familias, dos estilos (corrección del 1 oct 2026)

La tabla de abajo se midió sobre **Modern Exteriors** (el mundo: sin contorno). Los
**ítems, muebles, ropa e iconos** vienen de **Modern Interiors**, que **sí lleva un
contorno oscuro azul-violeta de 1 px**, colores más vivos y volumen por sombreado de
2-3 tonos. Dibujar un ítem sin contorno "con las reglas del Exteriors" es lo que hacía
que mis iconos se vieran de otro juego. Antes de dibujar, saber a qué familia pertenece:

| familia | qué es | contorno | se compara con |
|---|---|---|---|
| Exteriors | terreno, fachadas, árboles, vehículos, props de calle | ninguno | `critico-pixel.py --ext` |
| Interiors | ítems de mochila, muebles, ropa, comida, herramientas, iconos | 1 px oscuro | `critico-pixel.py` (por defecto) |

## 0.6 El crítico y las texturas reales (obligatorios)

* `python3 scripts/critico-pixel.py pieza.png [--celda 32 --k 3,4] [--ext]` mide **plano**
  (% de píxeles lisos: mediana del pack ≈ 9 %, techo 32 %), **variedad** local y colores contra
  el pack, y deja una lámina con tus 7 vecinos más parecidos. **Hay que mirarla (`Read`).**
  Mis iconos de ropa dieron 35-52 % de plano: demasiado lisos, y la lámina lo mostraba.
* Las superficies grandes no se rellenan con una rampa: se sintetizan con
  `scripts/lib/materiales.py` (Image Quilting sobre cuadros reales del pack, con la
  luminancia de tu forma como guía). Sale con el grano, las juntas y la paleta del pack.
* Para trabajar con método: el subagente **`pixel-artist`** (`.claude/agents/pixel-artist.md`)
  hace todo este ciclo.

## 1. Las reglas, con su medición al lado

| regla | de dónde sale |
|---|---|
| **Grilla de 32x32.** Nada de 16. | la del pack; mezclar rompe la grilla del editor |
| **9 colores** por sprite; 16 en el p90, **30 es el techo** | mediana de 300 singles de 1 celda |
| **Cero dithering.** Ni un damero. | 0,0 % de píxeles en patrón alternado — mediana *y* p90 |
| **CERO CONTORNO.** Ni negro ni de ningún color: la silueta es donde se termina el material. | 26.327 pares borde↔interior: la mediana del salto de luminancia es **0** y en el **94 %** el borde es *exactamente el mismo color* que su vecino interior |
| Lo único que va más oscuro es el borde de **abajo y derecha** (el lado en sombra), un escalón de rampa | se deduce de la dirección de luz medida |
| **Paso de 12 de luminancia** entre tonos vecinos de una rampa | mediana (p25 6, p75 18) |
| El tono más oscuro va **tintado**, jamás gris ni negro puro | 246 de 250 sprites |
| **Saturación 0,28** de mediana. El pack es apagado. | 250 sprites |
| Luz **suave** de arriba-izquierda: sólo **+5** arriba y **+4** izquierda | media de 400 sprites |
| **Sombra de contacto (58,58,80) a alfa 100/255** | 104 de 400 singles la traen, y ninguno usa otro color |
| El objeto se **apoya al ras** del borde de abajo de su celda | mediana de 0 filas vacías debajo |
| Ocupa **~48 %** de la celda | mediana (p10 14 %, p90 100 %) |

Dos consecuencias que se olvidan seguido:

- **Sin anti-alias.** Los únicos alfas del pack son 255 y 100 (la sombra). Un
  borde suavizado se ve borroso al lado de todo lo demás.
- **El sombreado cambia de tono de golpe**, no interpola. Un degradado es de otro
  juego.

> ⚠ **La trampa del promedio, que costó dos versiones de `contorno()`.**
> Medir `mean(píxeles de borde)` contra `mean(píxeles de interior)` da **18 de
> luminancia**, y de ahí salió la conclusión —falsa— de que el pack oscurece el
> contorno. Es un artefacto: los bordes caen más seguido en la parte baja y
> lateral del objeto, que ya son las zonas oscuras. La medición correcta es
> **cada píxel de borde contra su vecino interior**, y ahí el salto es 0. Si una
> medida va a decidir cómo se dibuja algo, tiene que comparar lo que de verdad
> está al lado, no dos promedios.

## 2. La librería lo hace cumplir

`scripts/lib/pixel.py`. No es un motor de dibujo, es un corsé: cada color pasa
por la paleta del pack y `verificar()` se planta si la pieza rompe una regla.

```python
import sys; sys.path.insert(0, "scripts")
from lib.pixel import Lienzo, rampa

l = Lienzo(32, 32)
r = rampa((150, 110, 70), 4)      # 4 tonos DEL PACK cerca de ese marrón
l.sombra_al_pie(8, 16)            # (58,58,80) @ alfa 100, debajo de todo
l.volumen(8, 14, 16, 17, r, tapa=2)   # relleno + tapa clara + columna derecha oscura
l.contorno(r[0])                  # NO es un contorno: oscurece abajo y derecha
l.verificar()                     # ← el guardián
l.guardar("native/crates/ha-game/assets/art/gen/items/wood.png")
```

- `ajustar(c)` fuerza cualquier RGB al color más cercano de la paleta medida
  (`scripts/lib/paleta_limezu.json`: los **501 colores que cubren el 95 %** de
  los píxeles que el juego dibuja hoy). Se regenera con
  `python3 scripts/lib/pixel.py --paleta`.
- `rampa(base, n)` no interpola en RGB: pide a la paleta el tono más parecido en
  cada escalón de luminancia, así el oscuro sale tintado como el del pack.
- `verificar()` corta por: demasiados colores, color fuera de paleta, alfa raro,
  negro neutro, dithering y objeto flotando.

**No escribas píxeles con PIL directo.** Si te falta una primitiva, agregala a la
librería —así la próxima pieza la hereda— en vez de saltearte el corsé.

## 3. Dónde va lo dibujado

- El PNG, en `native/crates/ha-game/assets/art/gen/<categoría>/<nombre>.png`.
  `art/pack/` es del pixel comprado; `art/gen/`, del dibujado. Que se distingan
  de un vistazo es lo que permite reemplazar un dibujo por arte real después.
- La clave lógica y el rect, al manifest — **los publica el importador**, nunca
  se escriben a mano (regla 7 del proyecto: los rects se miden).
- El slot tiene que existir en `ha-assets/src/slots.rs` con su nombre humano en
  castellano, o falla `every_slot_has_a_human_label`.
- Si el arte real llega después, **gana el arte real**: el manifest manda y el
  dibujo se reemplaza sin tocar código.

## 4. Antes de dar una pieza por hecha

1. `l.verificar()` en verde.
2. **Mirala ampliada, sobre el fondo donde va a vivir** (pasto, asfalto, el
   inventario). Un sprite que se lee sobre blanco puede desaparecer sobre pasto.
3. Ponela **al lado de un asset del pack** en la misma imagen. Si se distingue
   cuál dibujaste vos, no está lista.
4. Los tests de arte del juego en verde:
   `cargo test -p ha-game --lib art2d`.
