# La maqueta de la interfaz — las imágenes de referencia y cómo se hicieron

Esto es **la verdad de cómo tiene que verse la interfaz de Ki**. Son las
imágenes que el usuario aprobó el 15 ago 2026 (el documento *Interfaz de Ki*) y
los scripts que las compusieron.

> *"Las imágenes de referencia están ahí porque se supone que el resultado final
> va a ser tal cual es."* — el usuario, 16 ago 2026.

## Por qué viven en el repo

Se compusieron en un directorio temporal y ahí se iban a perder. Sin ellas, la
única forma de saber si una pantalla está bien es la memoria de alguien — y la
consigna fue *"no quiero variaciones"*. Acá están para poder comparar.

## Qué es cada cosa

| Archivo | Qué muestra |
|---|---|
| `00-*` | La splash de FZ STUDIOS (las manchas de pintura) |
| `01-*` | El menú principal |
| `02-partidas.png` | La lista de mundos |
| `03-personajes.png` | La lista de personajes |
| `04-nueva-partida.png` | Nombre, semilla y modo |
| `05-*`, `06-*` | El creador de personaje y sus cinco pestañas |
| `07-la-consola-escondida.png` | La consola de `/creativo` |
| `08-los-botones-aparecen-cuando-sirven.png` | El HUD en juego, con y sin objetivo |
| `09-inventario-en-creativo.png` | El catálogo creativo |
| `10-mochila-en-supervivencia.png` | La mochila |

Los `.py` son los que las dibujan. `uikit.py` es el importante: de ahí salen las
**medidas de las piezas del pack** (`PIEZAS`), que son las que publica
`scripts/import-ui-kit.py` y usa `ha-game/src/ui_pack.rs`.

## Cómo se comparan contra el juego

El juego dibuja estas pantallas en coordenadas de **2400 × 1080** —el Redmi
apaisado, que es la resolución en la que se compuso la maqueta— y las escala con
un solo factor (`ui_pack::k`). Para capturar:

```bash
cd native
HA_MENUSHOT=/tmp/menu.png            ./target/release-native/ha-game   # el menú
HA_LISTASHOT=/tmp/lista              ./target/release-native/ha-game   # las tres listas
HA_CCSHOT=/tmp/cc                    ./target/release-native/ha-game   # el creador
HA_AUTOPLAY=1 HA_BAGSHOT=/tmp/juego  ./target/release-native/ha-game   # HUD y mochila
```

Los scripts necesitan `PRUEBAS-CODIGOS/` (el pack de LimeZu) y el manifest del
juego; se corren con `python3 scripts/maqueta/mock_menu.py <carpeta-salida>`.
