# Guion para el video de donaciones

Regla de oro: **mostrá el visor en vivo, no digas números de memoria.** Si algo
falla en cámara, decilo — es más creíble un "esto todavía no funciona" que un
video sin un solo tropiezo.

---

## 1. Qué es (30 s, hablado)

"Estoy construyendo el cerebro de un dron que se las arregla solo **sin GPS**
y con un chip de dos dólares. Un piloto con la física programada vuela; una
red neuronal de 11 KB lo corrige. Se le dice una tarea en palabras —'llevá
estas cinco cajas'— y decide solo el orden, arma un mapa de lo que ve mientras
vuela y esquiva lo que encuentra. Todo esto es simulado. Nunca voló en un
aparato real, y para eso es el video."

## 2. DEMO EN VIVO (2-3 min)

Abrí `http://localhost:8080` en pantalla completa. Antes de grabar:

```bash
curl -s localhost:8080/estado | python3 -c "
import json,sys; d=json.load(sys.stdin)
print('piloto:', 'RED' if d['ia'] else 'CLASICO', '· compatible:', d['compatible'])"
```

Si el genoma nuevo todavía no está instalado, grabá con el piloto CLÁSICO
(botón del piloto) y decilo: "esto es el piloto clásico; la red se está
entrenando para estas tareas".

**Orden:**

1. **Tirar tres o cuatro postes** con un click en el piso. "Obstáculos puestos
   ahora, no un escenario preparado."
2. **Decirle una tarea** (cajón → «decirle una tarea»):
   `caja 6 2 a -4 5; caja -3 -4 a 8 -3; caja 2 7 a 9 4; volver` → «hacer esta
   tarea». Decí: "no le digo en qué orden: lo decide él, por el camino más
   corto".
3. **Mostrar el mapa**: los anillos celestes que aparecen en el piso son lo
   que el dron recuerda haber visto. "Cuando vuelva por acá, ya sabe dónde
   está cada poste."
4. **El cerebro en vivo** (panel de la red): los 96 bits de sensores y las 4
   salidas. "Esto es literalmente lo que piensa."

## 3. Lo que NO funciona todavía (15 s, de frente)

"Nunca voló en un aparato real. Sin GPS, el dron se va desviando: se corrige
con lo que vio al arrancar, pero con sensores muy malos todavía se pierde.
Y aterrizar con sensores muy ruidosos falla una de cada cinco veces. Todo eso
hay que medirlo en un dron de verdad — el simulador ya no me puede decir más."

## 4. Para qué es la plata (con precio)

| Qué | Cuánto | Para qué |
|---|---|---|
| LiteWing (dron con ESP32-S3, programable) + módulo de flujo óptico | ~60-70 USD | El chip donde va el cerebro, y el sensor que reemplaza al GPS |
| Baterías 1S extra, cargador, hélices | ~15-25 USD | Poder probar más de 5 minutos seguidos |

**Total: ~80-100 USD.** "Con esto, lo primero que hago es medir cuánto se
desvía el giróscopo con el dron quieto: es el número que decide todo lo demás.
Publico el resultado, salga como salga."

## 5. Cierre (10 s)

"Cada decisión del proyecto está medida y escrita, incluidas las que fallaron.
El cerebro ya está compilado para el chip: entra en 13 KB y decide en menos
de un milisegundo. Falta el aparato."

---

## Antes de grabar

- [ ] El visor abre y responde; probaste la tarea de cajas una vez.
- [ ] Sabés qué piloto está activo y lo decís.
- [ ] Ensayaste el orden de clicks una vez.
