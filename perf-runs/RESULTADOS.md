# Medición de rendimiento — Redmi 10 (21121119VL, Android 13)

Pantalla 2400x1080, `blit off` (escala de render 1.0 — el mundo se dibuja a
resolución nativa). Capturado por adb con el celu manejado desde la PC
(`input motionevent`), no a ojo.

## ANTES (v1.0.274 — el APK que estaba instalado)

| escenario | tirones (>50 ms) | peor frame | fps en régimen |
|---|---|---|---|
| 1 · quieto en el pasto (30 s) | 3 | 88 ms | 32–39 |
| 2 · caminar en línea recta (45 s) | 39 | 209 ms | 46–47 |
| 3 · subir al espacio (60 s) | 73 | **2 895 ms** | 3–22 |
| 4 · reingreso, bajando (90 s) | 283 | **4 150 ms** | — |
| 6 · abrir/cerrar menú (24 s) | 164 | 222 ms | — |

### El hallazgo

Al bajar desde órbita, los tirones grandes salen con una regularidad de reloj:

```
t=522.6s  frame=4023ms  (5.5 s desde el anterior)
t=528.1s  frame=4150ms  (5.5 s)
t=533.2s  frame=4002ms  (5.0 s)
t=538.5s  frame=3817ms  (5.3 s)
t=543.7s  frame=3958ms  (5.2 s)
t=547.9s  frame=2683ms  (4.2 s)
```

**Unos 4 s congelado de cada 5,2 s: el juego pasa ~76 % del descenso parado.**
Subir da lo mismo con picos de 2,9 s. Es exactamente el reporte del usuario —
"al salir al espacio los gráficos pasan a ser como dibujos, y al salir dan
tirones en cada cambio".

La cadencia regular apunta a los escalones de `lod_tile_m`, que redondea a
potencias de dos: cada cruce rehace la malla adaptativa entera
(`LOD_QUADS² = 192²` quads, ~150 000 muestras de terreno) en UN frame. §10 ya
lo tenía medido en escritorio a 318 ms y anotado como pendiente; en el Redmi son
4 000 ms.

### Por qué esto NO alcanza para acusar a nadie

En los frames de 4 s **todos los spans dan cero**:

```
t=528.13s frame=4150.3ms chunk=0.0 terrain=0.0 lights=0.0 atmo=0.0 part=0.0
```

§10 es explícito: *"spans ≈ 0 y `frame` enorme → no concluir nada"*. `terrain`
no está cableado — nadie escribe ese span —, así que `sync_terrain_lod` es
invisible en esta vista, igual de invisible que si fuera gratis. Ese atajo ya
mintió una vez y costó dos meses.

Por eso el APK nuevo lleva `MENU -> SONDA`, que enciende `ha-stages`/`ha-sys` en
el teléfono. Recién ahí el número tiene nombre y apellido.

## Caveats de esta corrida

- El escenario 6 (menú) quedó **contaminado**: el personaje estaba en órbita y
  lo que se abrió fue el mapa orbital, no el menú de superficie. No se puede
  leer como "costo de la UI".
- El escenario 5 (clima) no se corrió: `HA_WEATHER` es variable de entorno y no
  llega al APK, así que forzar lluvia en el celu hoy no se puede.
