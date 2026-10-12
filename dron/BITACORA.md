# Bitácora: cómo se llegó hasta acá

Este archivo es el REGISTRO de mediciones del proyecto. No es el contrato —
ése es `CLAUDE.md`, que se lee entero antes de tocar nada. Acá vive el detalle
de cada sesión: qué se probó, qué dio, qué se descartó y con qué número.

**Por qué existe separado.** La biblia había crecido a 2009 líneas, de las
cuales 1253 (el 62%) eran esta historia. Se cargaba entera en cada sesión y,
peor, se contradecía sola: tres tablas de `capacidades` con números distintos
sin marcar cuál era la vigente, y la frase «la deuda #1 —el orbitar— está
CERRADA» todavía en pie 100 líneas DESPUÉS de la sección que la desmiente. Un
contrato que se contradice deja de ser un contrato.

**Nada se borró en la separación.** Todo lo que sigue estaba en `CLAUDE.md` y
está acá igual. La regla 3.4 (los intentos fallidos valen tanto como los que
salieron) es la razón: un registro que se poda para quedar lindo es
exactamente lo que este proyecto no hace.

**Cuándo leer esto.** Cuando vayas a tocar un área concreta, leé su entrada
acá antes de empezar — es donde están las medicionesque ya se hicieron y los
caminos que ya se descartaron, para no volver a gastarlos. El contrato te dice
las reglas; la bitácora te dice qué pasó cuando alguien las aplicó.

**Lo que NO vale hacer con este archivo**: citar un número de acá como si
fuera el estado de hoy. Cada entrada es una medición con fecha y condiciones.
El estado vigente vive en la sección «Estado hoy» de `CLAUDE.md`, en un solo
lugar y verificado. Si una entrada de acá contradice esa sección, gana esa
sección — y si la entrada es la que tiene razón, se corrige la sección, no se
citan las dos.

---

## Índice

| Entrada | De qué va |
|---|---|
| 7.0 | Cambio de rumbo a hardware real y persecución de un móvil |
| 7.1 | El DBPOWER real, y por qué hay DOS bancos de prueba y no uno |
| 7.2 | El peldaño del semáforo: tres bugs de calibración y un límite genuino |
| 7.3 | El piloto manual: cuatro bugs, ninguno en la física |
| 7.4 | La mezcla de expertos: especializar no pagó, apagar la red sí |
| 7.5 | La aptitud no mira la sacudida del mando |
| 7.6 | El visor reescrito: la interfaz era parte de la vara |
| 7.7 | El visor volaba OTRO piloto que el que se entrena |
| 7.8 | El orbitar escondido adentro de la tolerancia |
| 7.9 | El experimento final de la mosca: negativo, y el código se borró |
| 7.10 | La auditoría de reglas, y el instrumento que no medía lo que decía |
| 7.11 | El grafo dibujaba un haz de cuatro, y media capa de estrategia sin usar |
| 7.12 | Veredicto del reentrenamiento de esquive: cambió el vuelo, no mejoró |
| 7.13 | El visor nunca reseteaba la memoria recurrente entre vuelos |
| 7.14 | El dron no aterrizaba nunca: el descenso se apagaba antes de tocar |
| 7.15 | La red estrellaba la posada que el modelo ya hacía perfecta |
| 7.16 | El ojo: percepción real, y la cámara de la máquina es una RealSense muerta |
| 7.17 | El «3/3 esquivando» era el conjunto de entrenamiento |
| 7.18 | Gradiente en la selección, y el canal de elevación que faltaba |
| 7.19 | El ojo real alimentando al dron en vivo, y grabación reproducible |
| 7.33 | El currículo enchufado al repertorio: una celda por peldaño |
| 7.34 | El A/B del repertorio no se replica en la segunda semilla, y la vara honesta no miraba los postes |

---

## 7. Estado y deuda conocida

### 7.0 Cambio de rumbo: hardware real y seguimiento de un móvil (última sesión)

El usuario pidió explícitamente pasar de "simulador que anda" a "camino a
hardware real", con el caso de uso de seguir una moto. Se hizo, en orden, lo
que bloqueaba eso por FÍSICA (no por entrenamiento):

1. **El aparato pasó de 0.8 kg/4 m/s a un quad de persecución de 5" real**
   (0.90 kg, 4×peso de empuje, arrastre cuadrático que antes NO existía). Techo
   medido: **31.8 m/s = 114 km/h** (`sim3d::velocidad_techo`,
   `inclinacion_sostenible`). Sin el arrastre cuadrático la velocidad terminal
   no tenía techo — ver comentario de cabecera de `sim3d.rs`.
2. **El vehículo perseguido existe** (`movil.rs`): cinemática de bicicleta con
   límites reales de aceleración (7 m/s²), frenada (8 m/s²) y curva (0.8 g), 6
   recorridos de referencia de ciudad a carretera. No es una lista de
   waypoints — seguir un móvil es un problema de persecución, no de llegada
   (ver comentario de cabecera de `persecucion.rs`).
3. **Los sensores ahora ven la velocidad del objetivo** (`gym3d::Guia`,
   `sensores_guia`), no sólo su posición. Se reordenó el presupuesto de 96 bits
   —tabla completa en el comentario de `sensores_guia`— porque sin la velocidad
   relativa ningún piloto puede anticipar hacia dónde va algo que se mueve.
4. **El guiado clásico de persecución (`Guiado::persecucion`) funciona,
   medido**: 0 caídas en 6 recorridos × 3 niveles de ruido, 81% del recorrido a
   tiro, sin chocar nunca. Costó tres rondas de diagnóstico real — la
   secuencia completa está en la regla 3.13, léela antes de tocar
   `Estabilizador3` o `Guiado` otra vez.
5. **El genoma instalado NO sirve para nada de esto.** Se invalidó DOS veces en
   esta sesión: una por el cambio de física (la relación velocidad→ángulo que
   aprendió ya no aplica) y otra por el cambio de sensores (bits reordenados).
   `capacidades` da 0/3 en casi todo con el genoma viejo — es exactamente lo
   esperado, no una regresión (ver sección 4: "cambiar el código de sensores
   los invalida").
6. **`dron perseguir`** es el informe nuevo, en castellano, con el mismo
   formato que `capacidades`: 6 recorridos, red vs. clásico, aprobado con 60%
   a tiro sin estrellarse.

**Lo que sigue faltando para hardware de verdad** (0% hoy, no se tocó en esta
sesión): puente MAVLink/MSP a un controlador de vuelo real, medir masa/inercia
del aparato físico en vez de suponerlas, un piloto de seguridad, y una cámara
real en vez de `RasgosVisuales` calculado analíticamente del estado exacto del
simulador.

**Actualización, misma sesión — el usuario reordenó prioridades a mano.**
El experimento de 5 semillas del Complejo Central terminó: el conectoma real
de *Drosophila* perdió limpio contra su propio control nulo (498.0 vs 1040.9
de aptitud media), y el desempate que corría al cierre del mensaje anterior
—forzar al circuito real a la MISMA latencia rápida del nulo, para descartar
que el nulo ganara por trampa de diseño— **no lo salvó**: 440.1 contra 1040.9,
t de Welch = −3.50. La anatomía real del Complejo Central, conectada tal como
está a este problema de rumbo, no aporta sobre puro ruido. Es un resultado
negativo limpio, no un experimento fallido — la hipótesis de la regla 3.10 se
prueba y se descarta con evidencia. **La línea de investigación de la mosca
queda abandonada** (en ese momento, no borrada — decisión revocada después:
ver 7.9, donde el experimento final con el instrumento arreglado confirmó el
mismo veredicto y ahí sí se borró el código).

Con eso descartado, el usuario dijo explícitamente "vamos por el que sí
funcione": persecución pasa a ser la tarea ACTIVA, por encima de la entrega de
paquetes (deuda de abajo, que queda pausada, no cerrada — ver nota al final de
esta sección). Se construyó el entrenador evolutivo:

7. **`Tarea::Persecucion`** conectado al mismo bucle evolutivo que ya usan
   `Punto` y `Mision` (`gym3d::entrenar`), con la misma disciplina de
   currículo que `mision.rs` aprendió por las malas (reglas 3.7/3.8) desde el
   primer entrenamiento: 6 peldaños (uno por recorrido), sólo se sube
   aprobando, con examen de repaso de los peldaños anteriores. El umbral de
   aprobación de cada peldaño es 60% de lo que saca el guiado CLÁSICO en ese
   recorrido — la misma vara que ya usa `informe()`.
8. **`dron entrenarpersecucion`** (CLI), análogo a `entrenarmision`, guarda en
   `genomes/DRON3D_PERSECUCION.adn` por defecto (nunca en `GENOMA3D` — son
   sensores distintos, pisarlo lo dejaría ilegible para las otras tareas).
   Verificado con una corrida de humo (5 generaciones, sin panics). Lanzado un
   entrenamiento real de 3000 generaciones en background al cierre de esta
   sesión (`nohup`, semilla=1, log en `/tmp/entrenar_persecucion.log`); si
   volvés a esto sin contexto, lo primero es mirar en qué peldaño quedó:
   ```bash
   grep -E "peldaño [0-9]+/6|TRABADO|APROBADO" /tmp/entrenar_persecucion.log | tail -20
   ```
   Sin traza ASCII en vivo todavía (`dibujar_campeon` no sirve para esto, ver
   el comentario en `gym3d::entrenar` — es deuda anotada, no descuido) y sin
   `telemetria_estancado` propia si se traba (el diagnóstico automático de
   "cuál caso, cómo falla, está olvidando" que tiene `mision.rs` todavía no
   tiene equivalente acá).

**No confundir con "deuda #1 — el orbitar" de abajo, que es de la tarea de
entrega, sigue vigente tal cual estaba, y quedó PAUSADA (no cerrada) por la
decisión de arriba.**

### 7.1 El aparato de referencia para hardware real, y por qué son DOS bancos de prueba, no uno

El usuario mostró el hardware que ya tiene: un **DBPOWER FPV** (placa marcada
`UD-U818H`, familia de electrónica UDI/U818A — la que usan varias marcas
recreativas rebrandeadas), batería LiPo **1S, 350 mAh** (formato 402545), con
**cámara FPV analógica ya montada** en un brazo. Fotos en la sesión, no hay
archivo — si hace falta volver a mirarlas, pedírselas de nuevo al usuario.

**Decisión explícita del usuario**: éste es el aparato de referencia para
hardware real. Reemplazar su placa de control por una reprogramable (una FC
tipo Betaflight/iNav, ~15-20 USD) es el plan, pero **no ahora — no hay
presupuesto**. La prioridad inmediata es **perfeccionar el software todo lo
que se pueda** con lo que ya existe, y recién ahí pasar a hardware.

**La tensión que hay que tener siempre presente**: este aparato es clase
juguete — con esa batería no va a volar más de 5-8 minutos ni pasar de
unos pocos m/s. **No puede perseguir una moto.** Eso no invalida nada de lo
construido esta sesión, porque el proyecto ya tiene, sin haberlo dicho así
hasta ahora, DOS bancos de prueba con roles distintos:

1. **El quad rápido de persecución (simulado, `sim3d.rs` con los números de
   hoy — 0,90 kg, 4× peso de empuje, 31,8 m/s)**: existe para estresar el
   ENFOQUE de control (física programada + red que corrige encima) contra
   algo exigente. Sigue siendo el banco de pruebas correcto para eso — no se
   cambia.
2. **El DBPOWER real**: va a ser el cuerpo de referencia para las primeras
   pruebas en hardware de verdad — despegar, sostenerse, navegar, aterrizar,
   con la física MODESTA que le corresponde a un aparato de juguete, no la
   del quad de persecución.

**El objetivo declarado por el usuario, a más largo plazo, es más grande que
cualquiera de los dos**: que el mismo enfoque —cerebro binario barato +
modelo físico programado + corrección aprendida— sirva algún día para
"controlar hasta un helicóptero autónomo o algún robot volador en Marte".
Eso es la razón de fondo para que la física del aparato (`sim3d.rs`) esté
aislada en constantes y no mezclada con el cerebro (`iafz.rs`) ni con el
enfoque híbrido (`gym3d::consignas_hibridas`): **el cuerpo cambia, el
enfoque no.** Cuando haga falta un tercer perfil de vehículo (el DBPOWER,
o más adelante otra cosa), la forma correcta es un perfil de física nuevo
y aislado, no reescribir encima del que ya funciona para el quad de
persecución — exactamente el error que la regla 3.4 prohíbe.

**Mediciones reales, tomadas por el usuario (balanza de cocina, foto
verificada):**

- **Masa: 93 g** el aparato solo (placa, motores, brazos, hélices, sin la
  carcasa protectora de hélices) **+ ~15 g** de carcasa si se usa ⇒
  **~108 g volando**. Comparar con el quad de persecución simulado (900 g):
  **8-9× más liviano**.
- **Hélices: ~15 cm** de punta a punta (diámetro, no radio — el usuario no
  estaba seguro del dato exacto, así que esto es la mejor lectura, no un
  número cerrado). Un quad de 5" (el simulado) tiene hélices de ~12.7 cm:
  el DBPOWER las tiene MÁS grandes en un chasis mucho más liviano, así que
  la relación empuje/peso puede ser generosa pese al tamaño de juguete —
  falta medir empuje real para saberlo, no se puede inferir del tamaño solo.
- **Falta todavía**: wheelbase (distancia entre motores opuestos) y algún
  indicio de empuje/motor (aunque sea "levanta o no levanta X gramos de
  peso extra") — sin eso no se puede fijar `EMPUJE_PESO` para este perfil.

Aun con estos tres datos, **todavía no existe un perfil de física aparte
para el DBPOWER** en `sim3d.rs` — es la construcción que sigue, no algo que
se pueda inferir de memoria de estos tres números. Hasta que se arme, no usar
los números del quad de persecución (900 g, 4× empuje) para nada que
involucre al DBPOWER: son de cuerpos distintos, por regla 7.1.

### 7.2 El peldaño del semáforo: tres bugs de calibración reales, y un límite genuino

Entrenando persecución con el currículo nuevo (7.0/7.1), el peldaño 3
("semáforo" — frenar a cero, esperar, arrancar de nuevo) se trabó y dio
lugar a la secuencia de diagnóstico más larga de esta sesión. Vale la pena
dejar el resumen completo porque mezcla tres bugs reales (ya arreglados) con
un límite que parece real y todavía no.

**Los tres bugs, todos con la misma forma — pedirle a una red binaria más
precisión de la que un corrector por bits puede dar** (ver regla 3.16, que
ya cubría el primero):

1. El umbral de aprobación podía pedir más que el techo teórico del
   recorrido (matemáticamente imposible). Arreglado con el tope
   `techo * 0.995`.
2. Aun con ese tope, el umbral podía pedir cerrar más del hueco a la
   perfección de lo que una red binaria puede afinar por mutación al azar
   (medido: pedir 57% del hueco clavó un peldaño 780 generaciones). Arreglado
   con `FRACCION_MAX_DEL_HUECO = 0.35`.
3. El EXAMEN DE REPASO comparaba contra el umbral viejo con precisión de
   punto flotante exacta: un genoma que sostenía 99.94% de un peldaño
   aprobado se consideraba "olvidándolo". Arreglado con `MARGEN_REPASO = 0.99`.

Cada uno se encontró con `persecucion::telemetria_estancado` (nueva, hecha
permanente — mismo rol que `mision::telemetria_estancado`) y verificado con
una prueba de regresión antes de aceptarlo.

**Después de los tres arreglos, el peldaño 3 SIGUIÓ trabado — 1300
generaciones, sin mover un decimal**, con la mutación ya escalada a un
tercer escalón (0.35, agregado esta sesión — ver `gym3d::entrenar`) y con
DOS hipótesis más medidas y descartadas: la corrección no estaba saturando
su límite (pedía 0.158 de un tope de 0.30) y la mutación sí tenía efecto
medible (mutar el campeón cambiaba el número, aunque para peor). El genoma
se quedó por debajo del piloto clásico en esa maniobra específica —no
"casi lo alcanza", genuinamente peor— durante toda la búsqueda.

**La lectura, sin forzarla**: parar del todo y volver a arrancar puede ser
una maniobra estructuralmente distinta de la conducción continua para un
corrector que piensa tick a tick sin planificación — la corrección que ayuda
mientras se avanza puede no tener nada útil que decir durante el tramo
parado, y encontrar una que sirva para las dos cosas a la vez puede ser un
óptimo mucho más difícil de topar por mutación simple. Es una hipótesis, no
una conclusión: no se investigó más porque el costo de seguir mirando esto
un peldaño más ya no valía la pena frente a ver qué pasa en los peldaños 4-6.

**Decisión tomada**: en vez de seguir ajustando el umbral a mano una cuarta
vez sin evidencia nueva, se usó el mecanismo que el proyecto ya tiene para
esto — `saltar`, la escape valve explícita de la regla 3.7. Relanzado con
`entrenarpersecucion ... saltar`: el peldaño 3 avanza tras ~15 generaciones
de estancamiento (no de entrada), y **queda gritado en el log como
"SALTADO A MANO — queda deuda"**, no escondido. Si volvés a esto sin
contexto: `grep "SALTADO A MANO" /tmp/entrenar_persecucion.log` te dice
exactamente qué quedó sin aprobar y por qué (repasar esta sección).

**Actualización, sesión de la prealimentación de aceleración (3.13/6a84635):**
se relanzó `entrenarpersecucion` desde cero con el binario que ya tiene ese
arreglo, semilla 1. Llegó al semáforo en la **generación 53** (antes: recién
se acercaba después de cientos) — la prealimentación sí ayuda a los peldaños
1-2. Pero una vez ADENTRO del semáforo, se clavó otra vez, esta vez en
12177.6, y sigue clavada 477+ generaciones al momento de escribir esto. **No
contradice la hipótesis de arriba, la refuerza**: la prealimentación de
aceleración ataca el error en curva (donde SÍ hay velocidad de referencia que
prealimentar); parar del todo y esperar es una velocidad de referencia CERO,
así que no hay nada que ese término pueda aportar ahí. Si volvés a esto:
`grep -E "peldaño 3|TRABADO" /tmp/entrenar_persecucion.log | tail -20`.

---

### 7.3 El piloto manual: cuatro bugs reales, ninguno en la física

El usuario pidió poder volar el dron a mano con la MISMA superficie de
control que la red (`roll_sp/pitch_sp/yaw_sp/alt_sp`), para juzgar si esos
cuatro números son controles razonables. Se construyó (`web.rs`:
`control_manual`, `MandoDron`; `cabina.html`: botón "volar dron", dos
sticks) y aparecieron cuatro bugs reales, ninguno en `Estabilizador3` ni en
la física — los cuatro eran de la capa que traduce estado↔pantalla↔stick. Se
dejan documentados porque **son la clase de bug que vuelve** cada vez que se
toca `cabina.html` o se agrega un modo de control nuevo.

1. **`alt_sp` heredaba el setpoint del piloto ANTERIOR.** Es una variable
   del hilo de física (vive desde que arranca el servicio, no del `Mundo`).
   El stick de altura es una TASA que integra sobre ella, así que al entrar
   a manual heredaba lo último que había pedido la IA o el clásico —
   cualquier `oz` vieja de otra tarea, no la altura real del dron. Medido:
   con el stick en cero, z caía de 2.9 a 2.15 m y se quedaba ahí para
   siempre. Es justo el bug que reportó el usuario como "nunca se queda
   quieto". Arreglado re-anclando `alt_sp = m.d.z` cada vez que se
   reinician los lazos de control (`reiniciar_control`), y haciendo que
   entrar a manual dispare ese reinicio. **Blindado con prueba**:
   `web::pruebas_manual::mando_en_reposo_no_cambia_nada` fija que mando en
   cero ⇒ salida en cero, para cualquier `alt_sp` previo.
2. **La guiñada del stick estaba invertida.** `yaw_sp` positivo gira a la
   IZQUIERDA (`Guiado::consignas`, convención ya establecida: ψ creciente
   lleva la nariz de +x hacia +y). El stick mandaba `yaw = +stick.x`, así
   que empujar a la izquierda giraba a la derecha. Arreglado con
   `yaw = -stick.x` en `cabina.html`. Sin cobertura de prueba — vive en JS,
   este proyecto no tiene arnés de pruebas para eso; si se vuelve a tocar el
   mapeo de sticks, releer este punto ANTES.
3. **Guiñada espejada en el render.** El mapeo mundo→escena de `cabina.html`
   es `(x, z, −y)`, una rotación PROPIA (det = +1), así que el ángulo pasa
   directo: `rotation.y = +ψ`. Estaba en `−ψ`: el modelo apuntaba al lado
   contrario del rumbo real salvo en 0° y 180° exactos — por eso giros
   "chicos" se notaban raros y giros de 180° parecían no pasar nada.
   `visor.html` ya usaba `+ψ` en el mismo repo: convivían dos convenciones
   opuestas sin que nadie lo notara hasta que hubo una cámara colgada del
   cuerpo que lo hiciera evidente.
4. **Ejes de actitud cruzados + FPV adentro de su propio dron.** La nariz
   del modelo es +X local (el LED); cabeceo tiene que ir sobre Z y banqueo
   sobre X, y estaban al revés (el dron "rolaba" cuando cabeceaba). Y la
   cámara FPV, al colgarla del grupo 3D del dron, quedaba dibujando los
   propios brazos y el aro del "puesto" de persecución por encima — hay que
   esconder ambos (`dron.visible=false; puesto.visible=false`) mientras se
   renderiza esa vista.

**La lección, no específica de este feature**: los cuatro bugs vivían en la
convención de signos/ejes entre `Dron3` (física), `cabina.html` (dibujo,
independiente de `visor.html` — dos archivos con la MISMA transformación
mundo→escena escrita dos veces, y ya divergieron una vez) y el JS del stick.
Ninguno era detectable leyendo un archivo solo: los cuatro salieron de
COLGAR una cámara del cuerpo real y mirar qué pasaba, no de revisar código.
Si se agrega un tercer lugar que dibuje el dron (o un tercer esquema de
control), medir la orientación en vivo contra `/estado`, no asumir que la
convención de `visor.html` ya está replicada en el nuevo lugar.

### 7.4 La mezcla de expertos: especializar en vez de agrandar

**Por qué.** Dos corridas independientes de persecución, con binarios
distintos, dieron el mismo resultado: **aprueba los dos peldaños de recta y
ninguno de los que tienen curva o frenada**. En el semáforo se clavó en
12266.4 necesitando 12272, con 100% del tiempo a tiro y 0.3 m de error medio,
y el mismo número exacto volvió a aparecer después. Un número que se repite
entre corridas no es mala suerte: es un límite estructural.

La biblia ya tenía el diagnóstico sin la cura. Sección 4: *"si algo no
aprende, preguntate cuántas cosas le estás pidiendo a la vez"* — y el
currículo separa las capacidades **en el tiempo**, pero al final queda UN
genoma sosteniéndolas todas a la vez. Y 7.2 lo dijo casi con todas las
letras: *"parar del todo puede ser una maniobra estructuralmente distinta...
encontrar una que sirva para las dos cosas a la vez puede ser un óptimo mucho
más difícil de topar por mutación simple."*

**Lo que hace la industria** (regla 3.17): esto es *gain scheduling* / control
conmutado, el estándar para "un solo controlador no cubre todo el sobre" —
cómo cambia de control cualquier VTOL entre estacionario y crucero. Del lado
del aprendizaje es una mezcla de expertos. Los dos riesgos conocidos están
resueltos desde el diseño, no descubiertos volando: **histéresis** más
permanencia mínima contra el *chattering*, y **transición suave** contra el
salto del mando al conmutar.

**Medir antes de construir** (`dron regimenes`, tres diagnósticos nuevos y
permanentes). Lo que salió, y que redirigió el plan:

```
  régimen      ticks    clásico        red      Δ     techo   hueco
  recta        30647    43945.7    44336.3  +390.6   45970.5  2024.8  ↑ ayuda
  curva        10588    14766.4    14789.8   +23.4   15882.0  1115.6  ↑ ayuda
  transitorio   3735     5301.2     5263.6   -37.5    5602.5   301.3  ↓ ESTORBA
```

- En **transitorio la red ESTORBA**: en la ruta del semáforo, −61.2 puntos y
  el error medio pasa de 0.19 m a **0.72 m**. Esa ruta queda en 12080.8
  contra un clásico de 12149.6: **el genoma es peor que no tener red**. No le
  faltaban 5.6 puntos para aprobar, estaba en negativo.
- En **curva NO estorba**: +23.4 sobre 14766, o sea +0.15%. La hipótesis de
  interferencia en curva **no se confirmó** — ahí el problema es otro (el
  umbral pide cerrar 27-34% del hueco y la red encuentra 2-5%).

**El árbitro miraba el eje equivocado, y la aritmética lo delató.** La primera
versión clasificaba sólo por aceleración LATERAL, así que la ruta del
semáforo —que es casi toda frenar y arrancar EN RECTA— caía entera en
"recta" salvo los ticks totalmente parados. Y ahí está la trampa: los ticks
parados tienen **24.7 puntos** de margen sobre el clásico (ya saca 2310.8 de
un máximo de 2335.5), mientras el umbral del peldaño pide **+122.6**. *Un
experto del "parado" no puede aprobar ese peldaño ni siendo perfecto.*
Agregando la aceleración LONGITUDINAL el régimen pasa a 3735 ticks y su hueco
de 24.7 a 301.3. **La lección general: antes de poner un experto en un
régimen, calcular cuántos puntos hay para ganar ahí. Un régimen sin hueco no
se arregla con más generaciones.**

**La propiedad que hace segura toda la arquitectura.** Un régimen sin experto
no es un agujero: por la identidad de la regla 3.15 (red en cero ⟺ clásico
exacto) **es el piloto clásico**. Entonces la mezcla vacía es idéntica al
controlador de hoy, el peor caso de cualquier mezcla es la vara honesta, y un
experto sólo entra si le gana al clásico en su régimen. Hay prueba que lo fija
bit a bit (`la_mezcla_vacia_es_exactamente_el_clasico`) — y no es cosmética:
si el árbitro por sí solo moviera el control, cualquier mejora posterior
estaría inflada por el ruteo en vez de ganada por los expertos.

**Entrenamiento: ascenso por coordenadas.** Un experto por vez, con el árbitro
vivo y los demás congelados. El ruteo da la asignación de crédito gratis: una
mutación sólo puede cambiar los ticks de su régimen. El peldaño elegido ES el
muestreador de régimen — el experto transitorio entrena en el semáforo, no en
una recta donde su régimen no se activa nunca (ahí todos los individuos
puntúan igual, no hay gradiente y el campeón sale sorteado).

**Umbrales congelados antes de entrenar nada**, sacados de los rangos del
piloto clásico. Si se mueve un umbral después, todos los expertos entrenados
con el viejo quedan inválidos (co-adaptación experto↔frontera).

**Presupuesto del chip, medido:** 5.078 µs la decisión de mezcla contra 5.637
µs la monolítica — corre UN experto por decisión. Flash 3 × 11392 = 33.4 KB.
Ver 3.14.

**Se puede VER** (regla 3.3): `/estado` publica `regimen` y la cabina lo
muestra como etiqueta de color al lado del piloto. Verificado en vivo, no
supuesto: con la moto doblando a 13.7 m/s dice `curva`.

**Tres bugs encontrados construyendo esto, ninguno de ellos el que se fue a
buscar.** Valen aparte porque los tres son de la clase que vuelve:

1. **Carrera de datos en los expertos congelados.** El "mejor" de la
   generación BAJABA, lo cual con elitismo es imposible. `evaluar_poblacion`
   reparte la población entre hilos y todos usaban el MISMO slot por experto
   congelado, mientras cada episodio arranca con `reset_memoria(slot)`: un
   hilo le borraba la memoria recurrente al experto que otro estaba usando a
   mitad de episodio. La aptitud había dejado de ser función del genoma.
   Ahora cada hilo tiene sus copias (96..127, libres durante la evaluación).
   **Señal a recordar: una aptitud que empeora con elitismo es siempre no
   determinismo, nunca "mala suerte".**
2. **Un slot de experto AL AZAR no es neutro.** Vacío es el clásico exacto;
   al azar emite basura acotada y empeora. La población arrancaba por debajo
   del piso (11660.3 contra 12188.2) y gastaba decenas de generaciones
   aprendiendo a callarse. Se siembra `ELITE` de `POBLACION` — no los 48, que
   dejaría la élite entera en una cuenca.
3. **La telemetría medía otra cosa que la aptitud.** `telemetria_estancado`
   evaluaba al campeón como piloto monolítico mientras la aptitud lo medía
   dentro de la mezcla: reportaba 11951.0 donde el número real era 12182.6.
   Un diagnóstico que contradice al número que dice diagnosticar es peor que
   no tener diagnóstico.

**Resultado de la primera ronda: la mezcla NO gana todavía, y hay que decirlo
con la tabla en la mano.** Cuatro brazos, expertos entrenados cada uno sobre
UN recorrido:

```
  peldaño              umbral   clásico     vacía  monolít.   clavado    MEZCLA
  recta de ciudad     12405.1   12353.3   12353.3   12291.7   12291.2   12291.2
  recta de carretera  12076.8   11848.1   11848.1   12121.2   12199.9   12199.9 ←
  semáforo            12272.2   12149.6   12149.6   12080.8   12033.3   12183.9
  curva de carretera  12034.9   11798.9   11798.9   11835.3   11632.8   11921.1
  zigzag en tráfico    7552.6    7404.5    7404.5    7416.3    7295.5    7330.0
  ruta completa       11688.1   11458.9   11458.9   11644.3   11079.0   11236.7
  manejado             4006.8    3928.2    3928.2    4107.1    3297.1    3881.5

  aprobados:  monolítico 2/7 · mezcla 1/7
```

La columna **«vacía» es idéntica a «clásico» en los siete**, así que el brazo
de control valida la medición: el árbitro no mueve el control por su cuenta y
el resto de los números son reales.

Y lo que dicen es inequívoco: **la mezcla gana donde el experto entrenó
(semáforo +103.1 sobre el monolítico, recta de carretera +78.7, curva +85.8) y
pierde feo donde no (ruta completa −407.6, manejado −225.6)**, al punto de
perder una aprobación. La causa no es la arquitectura sino el protocolo: cada
experto se entrenó sobre un solo recorrido, así que aprendió *esa* versión de
su régimen. **La lección, que vale para cualquier especialista: un experto
entrenado en un solo recorrido no aprende un régimen, aprende un recorrido.**
El régimen tiene que muestrearse de varias rutas — por eso la segunda ronda
entrena sobre «ruta completa», que es la que mezcla los tres.

**El peldaño del semáforo tiene un techo medido, reproducido tres veces.** El
umbral pide cerrar el 35% del hueco a la perfección
(`FRACCION_MAX_DEL_HUECO`). Lo que se logra, en corridas independientes y con
arquitecturas distintas:

```
  monolítico sin límite de sacudida   12266.4   33.3% del hueco
  monolítico con límite               12262.6   32.2%
  (las dos con la mutación en su escalón máximo, 400+ generaciones clavadas)
```

**No se baja el umbral para que pase** — eso es exactamente lo que la regla
3.2 prohíbe, y `FRACCION_MAX_DEL_HUECO` está calibrado con la medición de
otro recorrido donde el mismo mecanismo cierra el 41.8%. Lo que queda anotado
es el dato: en ESTE recorrido el mecanismo topa entre 32 y 33%, así que quien
retome esto no tiene que volver a gastar 1500 generaciones para descubrirlo.
Si alguna vez se mueve esa constante, que sea con una medición de varios
recorridos y varias semillas, no para destrabar éste.

**VEREDICTO DE LA SESIÓN: la especialización NO pagó; la compuerta SÍ.**

La segunda ronda (experto de recta entrenado sobre «ruta completa», que
mezcla los tres regímenes) se clavó 150 generaciones en 11524.0 contra un
umbral de 11688.1 — y el monolítico saca 11644.3 en esa misma ruta. Ahí está
la lección: **el monolítico YA es el mejor experto de recta disponible**,
porque se entrenó sobre el currículo entero. Poner a un especialista nuevo a
ganarle a un generalista siendo generalista es la batalla equivocada, y se
perdió dos veces.

Lo que sí funcionó es mucho más chico y mucho mejor soportado: **apagar la red
donde estorba**. Configuración instalada —monolítico de experto en recta y
curva, detención al clásico— medida sobre los 7 peldaños:

```
  peldaño              umbral   clásico  monolít.    MEZCLA
  semáforo            12272.2   12149.6   12080.8   12169.5   +88.7
  ruta completa       11688.1   11458.9   11644.3   11662.3   +18.0
  zigzag               7552.6    7404.5    7416.3    7417.6    +1.3
  curva de carretera  12034.9   11798.9   11835.3   11835.5    +0.2
  manejado             4006.8    3928.2    4107.1    4073.3   −33.8
  aprobados:  monolítico 2/7 · mezcla 2/7
```

Mejor en 4 rutas, peor en 1, **sin ganar ninguna aprobación nueva**. Eso es
el resultado, sin adornos: la mezcla no destrabó ningún peldaño.

**Y una honestidad sobre la suavidad, para no vender humo.** Con los expertos
entrenados la sacudida era 1.54× el clásico contra 2.70× del monolítico, y
estuve a punto de anotarlo como ventaja de la arquitectura. No lo es, o al
menos no está demostrado: esos expertos estaban POCO ENTRENADOS y por eso
emitían poca corrección — suaves y débiles a la vez. Separar "especializar
suaviza" de "un experto débil se mueve poco" necesita un experto fuerte, que
no se consiguió. Queda como pregunta abierta, no como resultado.

**Lo que queda pendiente, por orden de valor:**

1. El criterio de admisibilidad por sacudida (7.5). Es lo único de esta
   sesión que cambia lo que el proyecto considera "bueno", y está sin
   implementar.
2. Si se retoma la especialización: entrenar el experto **sembrado desde el
   monolítico** y sobre varios recorridos a la vez, no desde cero y sobre
   uno. Las dos rondas fallidas hicieron lo contrario.
3. Repetir cualquier conclusión sobre varias semillas (sección 4). Todo lo de
   arriba es una semilla.

**Corrección del árbitro, por evidencia contra mi propio cambio.** Llegué a
clasificar por aceleración LONGITUDINAL (frenar desde crucero como régimen
aparte): el régimen crecía de 1557 a 3735 ticks y su hueco de 24.7 a 301.3
puntos, así que parecía mucho mejor blanco. Midiendo la contribución POR RUTA
resultó que el régimen no era homogéneo:

```
  semáforo        2372 ticks   −61.2  ESTORBA
  zigzag           138 ticks    −0.4  —
  ruta completa   1225 ticks   +24.0  AYUDA
```

Cruzado con el régimen angosto (sólo detención, 1557 ticks, −68.7 en el
semáforo), las rampas aportan **+7.5 incluso dentro de la ruta del semáforo**:
**el daño está entero en la detención**. Agrupar rampas con detención juntaba
ticks donde la red ayuda con ticks donde estorba, y se vio en la tabla:
apagar la red en el régimen ancho mejoraba el semáforo (+127) y empeoraba las
otras cuatro rutas. Con el régimen angosto, apagar en detención da mejor en 3
rutas, peor en 2, y **no pierde ninguna aprobación**. La lección:
**un régimen que mezcla ticks donde la corrección ayuda con ticks donde
estorba no es un régimen, es un promedio.** Antes de darle un experto a un
régimen, verificar que el signo del aporte sea el mismo en todas las rutas.

### 7.5 La aptitud no mira la SACUDIDA del mando, y el proyecto es de hardware

Hallazgo que salió de una sugerencia externa y que nadie había medido nunca.
La aptitud vive en el dominio del tiempo —suma posiciones y ticks— así que un
genoma puede comprar puntaje moviendo la consigna de actitud mucho más brusco
que el clásico. En el simulador eso es GRATIS. En un aparato real es calor en
los motores, estrés de ESC y vibración, y la sección 1 de esta biblia dice
que el objetivo no es que el número del simulador sea grande sino que el
aparato vuele afuera.

Medido (|Δroll| + |Δpitch| por decisión, en rad, sobre los 6 recorridos):

```
  clásico       0.03606
  monolítico    0.09991     x2.77
  mezcla        0.05536     x1.54
  mezcla vacía  0.03584     = clásico (el árbitro solo no agrega nada)
```

**El genoma instalado mueve el mando 2.77× más brusco que el piloto clásico**
y la aptitud no lo nota. Dos consecuencias:

1. Cualquier número de aptitud de este proyecto es, hasta acá, **una cota
   superior optimista de lo que va a pasar en hardware**. No invalida las
   comparaciones entre genomas (todos se midieron con la misma vara ciega),
   pero sí invalida leer "aptitud alta" como "vuela bien de verdad".
2. La mezcla resulta **casi el doble de suave que el monolítico** sin que
   nadie lo hubiera buscado — el apagado en detención y las transiciones
   sigmoides le quitan actividad de actuador. Es un argumento a favor de la
   arquitectura que no aparecía en la tabla de aprobaciones.

**Y entonces se midió lo que nadie había preguntado, y dio vuelta la sesión
entera.** Si el genoma le gana al clásico sacudiendo 2.77× más, ¿le gana
porque vuela mejor o porque sacude? Se le puso un techo de velocidad al
residuo (`DRON_SUAVE`, `persecucion::limitar_sacudida`) hasta dejarlo tan
suave como el clásico (×1.01) y se volvió a medir:

```
  peldaño              libre (x2.77)   suave (x1.01)        Δ
  semáforo                   12080.8         12185.1   +104.3
  recta de carretera         12121.2         12171.2    +50.0
  ruta completa              11644.3         11673.0    +28.7
  zigzag                      7416.3          7432.0    +15.7
  manejado                    4107.1          4007.4    −99.7
```

**El genoma no ganaba sacudiendo: PERDÍA sacudiendo.** El semáforo pasa de
12080.8 —por debajo del clásico— a 12185.1, por encima, **sin reentrenar
nada**. Son +104.3 puntos, más de lo que consiguió toda la arquitectura de
mezcla de la sección 7.4 (+88.7), con un `clamp` de tres líneas.

**Consecuencia incómoda y honesta: buena parte de 7.4 atacaba un síntoma.**
El "la red estorba en la detención" que motivó el árbitro, los expertos y el
ascenso por coordenadas era, en su mayor parte, un artefacto del temblor y no
un desajuste de régimen. Con el residuo limitado la red ya no estorba ahí, así
que no queda nada que apagar: medido, el monolítico solo da 2/7 y la mezcla
1/7. **La mezcla queda retirada** — el código del árbitro y `Piloto::Mezcla`
se dejan (están probados y no cuestan nada apagados), pero la configuración
instalada es el genoma solo con el residuo limitado.

**Por qué esto NO invalida las tablas:** el límite se aplica al residuo de la
red, y el piloto clásico no tiene residuo. `umbral_aprobado` y `clasico_de`
dan exactamente lo mismo que antes. La vara no se movió; lo que cambió es lo
que el genoma realiza.

Queda prendido por defecto en 0.05. El valor exacto tiene sensibilidad NO
monótona (0.07 pierde la aprobación de «manejado» que 0.05 conserva), así que
es la mejor medición de UNA semilla, no una constante afinada.

**Y la tarea de ENTREGA está mucho peor, medido.** La misma instrumentación
sobre `mision.rs`, peldaño 5 («ir a B», el que lleva 1200+ generaciones
trabado):

```
  sacudida del mando:  clásico 0.00892  ·  red 0.60958  ·  x68
```

**El genoma principal del proyecto —`genomes/DRON3D_64.adn`, el que sirve el
visor— mueve la consigna de actitud 68 veces más brusco que el piloto
clásico.** A 100 Hz, 0.61 rad por decisión es la actitud pedida oscilando
unos 35° entre decisiones consecutivas, sin parar. Eso no es volar: es
explotar el integrador del simulador, y nada en la cadena lo impedía —
`consignas_red` acota el VALOR de la corrección pero nunca su VELOCIDAD.

Con el límite puesto, ese peldaño cae de 218.9 a 133.4. **El 218.9 nunca fue
real.** Los dos están lejísimos del umbral de 900, así que la tarea no estaba
cerca de ninguna manera — pero ahora se sabe que parte de la distancia estaba
tapada por la ficción.

Consecuencia para leer esta biblia: **la tabla de `capacidades` de la sección
7 (despegar 3/3, ir de A a B 1/5, etc.) se midió con la aptitud ciega.** No
está falseada —todos los genomas se compararon con la misma vara— pero
describe lo que el genoma logra en un simulador que le permite temblar, no lo
que haría en un aparato. Hay que volver a medirla con el límite puesto antes
de sacarle conclusiones sobre hardware.

**A diferencia de persecución, acá el límite NO mejora el número** (218.9 →
133.4). Era de esperar: ese genoma se evolucionó SIN límite, así que su
política depende del temblor que ahora se le prohíbe. Lo que corresponde es
reentrenar con el límite puesto desde el principio, no clampear al final.

### 7.6 El visor, reescrito de cero: la interfaz era parte de la vara

El usuario lo dijo sin vueltas: *"la interfaz está tan mal hecha que incluso
minimizar el panel del dron para ver cómo entrega el paquete hace que se
cancele la acción. Paneles sobre paneles, demasiados botones dispersos, es
totalmente un fracaso"*. Tenía razón, y había un bug real debajo de la queja
estética — no era sólo desprolijidad.

**El bug, con su mecanismo exacto.** El MODO DE COLOCACIÓN (punto A / punto B
/ poste) vivía sólo como una clase CSS dentro de un panel que se podía
colapsar u ocultar. Ocultarlo no lo desarmaba: quedaba **armado e invisible**.
El siguiente toque sobre la escena caía en el `pointerup` del lienzo y
disparaba `/punto?cual=a`, que en `web.rs` hace `m.d = Dron3::nuevo(...)` más
`reiniciar_control = true`. O sea: teletransporta el dron y reinicia los lazos.
En medio de una entrega, eso la mata. Y el botón "entregar A→B" pegaba a
`/control?k=m`, que es un **interruptor**, con una etiqueta fija: el segundo
toque cancelaba sin que nada lo dijera.

Es la regla 3.3 mordiendo por el otro lado. La biblia ya exigía que toda
capacidad se pueda VER en el visor; lo que faltaba decir es la otra mitad:
**si mirar puede romper lo que estás mirando, el visor dejó de ser un
instrumento de medición.** Una interfaz que cancela la corrida que observa es
tan inútil como una aptitud que no mide lo que dice medir.

**Lo que había.** Seis superficies flotando encima de la escena (`#datos`,
`#banco`, `#ayuda`, `#datosBio`, `#tactil`, `#ocultar`), la misma acción
repartida en tres lugares (teclado, banco, botonera táctil), y el panel del
conectoma quedándose con el **42% del ancho** de una línea de investigación
que la sección 7.0 ya declaró abandonada. El único remedio era un botón que
los escondía a todos, incluido el que hacía falta.

**Las tres reglas de la versión nueva** (`src/visor.html`, reescrito entero):

1. **La escena nunca se tapa.** Barra de arriba y dock de abajo son filas de
   un grid, no paneles flotantes: el 3D vive en la fila del medio y nada se le
   superpone. El único overlay es el cajón, y es bajo demanda.
2. **Una acción, un lugar.** Cada cosa está en exactamente un botón. El
   teclado sigue andando como atajo, no como cartel.
3. **Mirar nunca toca la simulación.** Abrir el cajón, prender el conectoma,
   mover la cámara, esconder el HUD: ni un `fetch`. Todo lo que habla con el
   servidor pasa por una sola función (`mandar`), así que la propiedad se
   verifica leyendo ocho líneas en vez de todo el archivo.

Y el modo de colocación: cartel grande mientras esté armado, **A y B se
desarman solos después de colocar** (son destructivos y de una sola vez),
poste sigue armado (casi siempre querés varios), `Esc` lo suelta.

**Verificado en un navegador de verdad, no razonado** (regla 3.12). No había
extensión de Chrome conectada, así que se manejó Brave en headless por el
protocolo de DevTools — el guión quedó en el scratchpad de la sesión, es
quince líneas de `node --experimental-websocket`. Lo que dio:

```text
  botón con entrega viva      class="peligro"  "■ cancelar entrega"
  barra de misión             viva · 6 segmentos · "tramo 1/6 · despegar"
  cajón+conectoma+HUD+cámara  misión 6→6 · tramo 0→0 · postes 0→0 · piloto true→true
  tecla 4 arma / Esc suelta    armado="on" → armado=""
  colocar A y auto-desarmar    partida (0,0) → (-1.01,-0.36), armado=""
  colocar poste, sigue armado  postes 0→1, armado=true
  errores de consola           ninguno
```

La tercera línea es el arreglo: la que antes cancelaba la entrega.

**Dos cosas que el servidor ya publicaba y el visor viejo nunca mostró**:
`entrenamiento` (el objeto de `persecucion::leer_estado`, con peldaño, gen,
mejor, umbral y estancamiento) y `regimen`. El visor viejo seguía leyendo la
`pizarra` vieja. Ahora están los dos, en el cajón.

**Y una decisión de presentación que vale anotar porque es la regla 3.15 en
la pantalla**: la barra de progreso del entrenamiento mide GENERACIONES, no
aptitud. Una barra de aptitud desde cero no dice nada acá — con la red en cero
el piso ya es el piloto clásico, así que el mejor y el umbral difieren en
~0.1% y la barra saldría siempre llena. Lo que falta para aprobar va como
número (`falta X.X (0.08%)`), que es lo que de verdad se lee.

**Lo que NO se tocó en la reescritura del cliente**: ni un solo endpoint. El
HTML es todo nuevo y el servidor nada, así que `cabina.html`, las rutas y el
formato de `/estado` siguen igual.

### 7.7 El visor volaba OTRO piloto que el que se entrena y se mide

El usuario avisó: *"ojo, el dron actualmente no se siente que haga todo lo que
dices que hace"*. Tenía razón, y no era una impresión — era un bug de una
línea, el mismo tipo que la regla 3.12 lleva tres rondas persiguiendo.

`mision.rs` (entrenamiento) y `capacidades.rs` (el informe) usaban
`gym3d::consignas_hibridas_punto`: **el modelo físico pilota y la red corrige
encima**, acotada (regla 3.15). La rama de "navegar a un punto" de `web.rs`
—el visor— seguía en `gym3d::consignas_red`: **la red REEMPLAZA al piloto**.
Son los mismos bits del genoma leídos con dos significados distintos.

Medido, con `genomes/DRON3D_64.adn` instalado, objetivo (6,0,3), piloto RED:

```text
  visor con `consignas_red`          falta 4.35 · 2.56 · 2.43 · 4.14 m   (da vueltas)
  informe con `consignas_hibridas`   5/5 rutas cumplidas
  visor con `consignas_hibridas`     falta 0.90 · 0.91 · 0.89 · 0.90 m   (se queda)
```

Los tres campos que la regla 3.12 manda chequear —genoma, objetivo, piloto—
estaban BIEN los tres. El que estaba mal era un cuarto que nadie había pensado
en chequear: **qué traducción de salidas se está usando**. La regla 3.3 ya
decía que `gym3d::consignas_red` existe porque esa traducción estaba copiada
en seis archivos; lo que no decía es que tener DOS traducciones distintas
convivendo (`consignas_red` y `consignas_hibridas_punto`) reabre exactamente
el mismo agujero. Arreglado, y de paso el filtro pasabajos pasó a correr
siempre en el visor —no sólo para el clásico—, igual que en `mision.rs`.

**Agregar a la lista de la sección 5, antes de decir "mirá el visor":**

```bash
grep -n "consignas_hibridas_punto\|consignas_red" src/web.rs src/mision.rs src/capacidades.rs
```

Si el visor y el gym no nombran la MISMA función, no estás mirando el mismo
dron, por más que el genoma y el objetivo coincidan.

### 7.8 «El orbitar está cerrado» es falso: está escondido adentro de la tolerancia

Con el visor ya arreglado, el dron hace esto en las cinco rutas de
`capacidades`, medido:

```text
   ruta 1    queda a 0.95 m      ruta 4    queda a 0.92 m
   ruta 2    queda a 0.89 m      ruta 5    queda a 0.90 m
   ruta 3    queda a 0.91 m
   deriva en los últimos 3 s:  0.89 / 0.89 / 0.89 m
```

**Las cinco rutas terminan entre 0.89 y 0.95 m, y el criterio de aprobado es
"entrar al metro del punto".** No es casualidad ni ruido: es un genoma que
aprendió a estacionarse justo adentro de la línea de aprobación. Y en el visor
se ve lo que el número esconde — el dron orbita despacio alrededor del punto a
~0.9 m de radio (x: 6.47 → 5.75 → 5.21 → 5.22 → 5.65 mientras y: 0.75 → 0.86
→ 0.39 → −0.32 → −0.79), nunca entra.

Más abajo en este mismo archivo dice **"La deuda #1 —el orbitar— está
CERRADA"**. Con esta medición, no lo está: está por debajo del umbral que la
mide. El 5/5 de «ir de A a B y frenar» es cierto contra su propia vara y
engañoso como descripción de lo que hace el aparato — que es justo lo que el
usuario reportó mirando la pantalla.

Lo que la regla general dice: **un criterio de aprobado con tolerancia fija es
un imán.** Si «llegar» es «estar a menos de 1 m», la evolución encuentra 0.9 m
y se queda ahí, porque más cerca no paga. No se arregla bajando la tolerancia a
mano (eso es mover la vara, regla 3.2) — se arregla si la aptitud sigue pagando
por acercarse adentro del radio, cosa que hoy hay que verificar antes de tocar
nada.

### 7.9 El experimento final de la mosca: NEGATIVO, y el subcircuito se borró

Pedido explícito del usuario, después de meses con la línea de investigación
en pausa (7.0): visitar la documentación real de FlyWire/Codex, confirmar que
el circuito estaba bien conectado —no a ojo, con la anatomía real— y correr
la medición definitiva con el instrumento de hoy, no el que se usó la primera
vez (que ya se sabía roto, ver 3.9). Si volvía a dar negativo, borrar todo.

**Paso 1 — verificar que el cableado es anatómicamente real, no inventado.**
Se fue a la documentación oficial (`codex.flywire.ai/faq`) y a la literatura
(Seelig & Jayaraman 2015 sobre el *bump* de rumbo en E-PG; Eckstein et al.
sobre la clasificación de neurotransmisor). Tres verificaciones concretas,
contra los datos ya extraídos en `data/conectoma_*.json` (antes de borrarlos):

- El umbral de 5 sinapsis mínimas que usaban los scripts de extracción **es
  el mismo que Codex recomienda por defecto** para el dataset FAFB.
- El subcircuito HS/VS retenía neuronas tipadas `HSN`/`HSE`/`HSS`/`VS1-10`
  (las tangenciales de lóbula real) de entrada y `DNp*`/`DNc*` (descendentes
  reales) de salida — no cualquier cosa con ese rol.
- El subcircuito del Complejo Central retenía, con el nombre exacto, **46
  E-PG, 33 PFN, 37 PFL, 25 PFR, más hDelta/vDelta** — las piezas que la
  literatura real dice que comparan "hacia dónde miro" contra "hacia dónde
  quiero ir". No era una aproximación de la brújula: era la brújula.

Conclusión de este paso: el cableado estaba bien. Si el experimento daba
negativo, no iba a ser por haber conectado cualquier cosa.

**Paso 2 — arreglar el instrumento de medición.** `gymbio.rs` todavía corría
`episodio_bio` con la formulación VIEJA (`consigna(o[0], VEL_MAX)`, la red
REEMPLAZANDO al piloto) — exactamente el régimen que la regla 3.15 ya había
medido incapaz de aprender nada (892 de 7161 evolucionando desde cero, contra
12420 de 12600 corrigiendo sobre el modelo). Se reescribió para usar
`consignas_hibridas_punto` (el modelo físico pilota, lo aprendido corrige),
con el filtro pasabajos corriendo siempre y el mismo límite de sacudida que
el resto del proyecto (`persecucion::limitar_sacudida`). Y se agregó el
brazo de control que le faltaba al experimento original: `solo`/`cx-solo`,
IAFZ-64 sin ningún circuito bio integrado — antes sólo se comparaba real
contra nulo, nunca contra no tener nada.

**Paso 3 — correr las 24 combinaciones** (2 circuitos × {real, nulo, solo,
control de latencia a 1 paso} × 3 semillas, 60 generaciones cada una):

```text
                          semilla1   semilla2   semilla3   PROMEDIO
solo (sin mosca, HS/VS)    5434.6     5694.8     5616.8     5582.1
HS/VS real                 4048.5     5647.4     6469.3     5388.4
HS/VS nulo                 5970.9     5707.2     5376.5     5684.9

cx-solo (sin mosca, CX)    4343.3     5325.3     6360.3     5343.0
CX real                    6447.4     3943.1     5566.8     5319.1
CX nulo                    4821.9     4498.0     2452.0     3924.0
```

**Veredicto, sin adornar: ninguno de los dos circuitos supera de forma
confiable a no ponerle nada.**

- HS/VS real perdió, empató y ganó — una vez cada cosa, en las tres semillas
  (−1386, −47, +853). El promedio (−193.7) es más chico que el desvío entre
  semillas (1231): eso es ruido, no señal.
- CX real hizo lo mismo al revés: ganó feo, perdió feo, quedó en el medio
  (+2104, −1382, +224). El promedio contra su propia línea de base sin
  circuito quedó en **−23.9 sobre 5343 — menos del 0.5%, adentro del ruido**.
  Sí le ganó al cableado al azar (+1395), pero el cableado al azar dio
  resultados bajos y erráticos en las tres corridas por su cuenta (4822,
  4498, 2452): ganarle a un control que anda mal no es lo mismo que aportar.

La pregunta que importa no es "¿le gana al cableado al azar?" — es "¿le gana
a no ponerle nada?", y ahí la respuesta es que no, para ninguna de las dos
partes de la mosca. A diferencia de la medición de hace un año (3.9), ésta
no se descarta por instrumento roto: la fórmula era la de hoy, el filtro
corría siempre, el límite de sacudida estaba puesto, y hubo tres semillas en
vez de una. Es un resultado negativo limpio.

**Bug encontrado en el camino, sin relación con la biología**: el control de
latencia (forzar el circuito a 1 paso de integración) dio, en las 6 veces que
corrió (2 circuitos × 3 semillas), un número IDÉNTICO —dígito por dígito— a
la línea de base sin circuito. Es un bug propio (la corrección queda pegada
en cero con un solo paso), no evidencia sobre latencia. No afecta el
veredicto de arriba, que no depende de ese brazo.

**Consecuencia: el subcircuito se borró del repo**, no se dejó apagado como
la vez anterior (7.0 decía "no borrada, por si alguien la retoma" — ese
corolario queda revocado, reemplazado por este cierre). Se sacaron
`src/bio.rs`, `src/gymbio.rs`, el comando `entrenarbio`, los genomas
`DRON3D_BIO*`/`DRON3D_CX*`/`DRON3D_NULO*`, los datos extraídos
(`data/conectoma_*.json/.txt`, `data/raw/*.csv.gz`, 99 MB), los scripts de
extracción (`tools/extraer_conectoma.py`, `tools/extraer_cx.py`,
`tools/generar_nulo.py`) y `ARCHITECTURE.md`/`EXPERIMENTO-CX.md` enteros. En
el visor, el panel del conectoma (que se comía hasta el 38% del ancho) se
reemplazó por un panel de **nuestra propia red** (IAFZ-64): como el kernel es
denso —XNOR+popcount, sin neuronas direccionables— lo único observable de
verdad son los 96 bits de sensor que entran y las 4 salidas crudas que
decide, así que eso es lo que muestra (`vistaRed`/`dibujarRed` en
`visor.html`, campos `sensRed`/`salRed` de `/estado`).

`consignas_red` y `consignas_hibridas_punto` (`gym3d.rs`) perdieron el
parámetro `corr_bio` que tenían: no hay nada que corrija más, y dejarlo ahí
con `(0.0, 0.0)` en cada llamador era ruido de firma sin ningún llamador que
lo necesitara.

---

> **⛔ AFIRMACIÓN REFUTADA — se deja escrita por la regla 3.4, no vale citarla.**
> Lo de abajo («el orbitar está CERRADA») fue desmentido dos veces: primero por
> la entrada 7.8 de esta misma bitácora, y de forma definitiva en la auditoría
> (7.10), donde se midió de frente por primera vez. El número: el genoma se
> estaciona a 0.89 m del punto **moviéndose 0.55-0.61 m** en los últimos 3 s,
> contra un piloto clásico que queda a **0.01 m moviéndose 0.02 m**. La órbita
> nunca se cerró — estaba tapada por un instrumento que medía distancia al
> objetivo y la reportaba como quietud. Estado vigente: `CLAUDE.md`, «Estado hoy».

**La deuda #1 —el orbitar— está CERRADA.** Eran tres causas y ninguna era el
bono de llegada que se venía tocando (ver README, sección "El orbitar: tres
causas"): el código de sensores hacía que "llegué" no fuera distinguible de
"estoy a 14 m", la salida de altura estaba con candado en la zona saturada de
la tanh, y sobre todo la tarea no pedía frenar. Hoy el dron entra al punto,
frena y se queda; la traza que lo muestra está en el README.

**El genoma instalado hoy (`genomes/DRON3D_64.adn`) está VICIADO de origen y
hay que reentrenarlo.** Se produjo con el currículo viejo, el que saltaba de
peldaño por estancamiento: subió a "esquivar dos postes", a "posarse" y a
"entrega" **sin haber aprobado ninguno**. Sus 0/3 en entregar y 0/3 en posarse
no son mala suerte, son exactamente las fases que se saltearon. Ahora que la
regla 3.7 está en el código (sólo se sube aprobando), lo primero que hay que
hacer es volver a entrenar desde cero y ver hasta qué peldaño llega de verdad
— probablemente se trabe temprano, y esa es justamente la información que el
currículo viejo estaba escondiendo.

Para referencia de dónde se venía, el entrenamiento que lo produjo se cortó a
mitad de camino (680 de 1200 generaciones, por el límite de tiempo) y llegó a
la última misión recién en la generación 350.
`capacidades` con este genoma, medido, no proyectado:

> **Tabla histórica (1 de 3 en este archivo).** Corresponde al genoma viciado
> de aquel momento. NO es el estado de hoy — el vigente está en `CLAUDE.md`,
> «Estado hoy», medido y con fecha. Se conserva para poder comparar de dónde
> se venía.

```
   despegar               3/3        (clásico 3/3)
   ir de A a B y frenar   1/5        (clásico 5/5)
   quedarse quieto        1/3
   posarse entero         0/3        (clásico 1/3)
   recuperar de vuelco    1/3
   esquivar obstáculos    1/3        (clásico 0/3 — el genoma le gana acá)
   entregar un paquete    0/3        (clásico 1/3)
```

Es la diferencia entre "no vuela nada" (el genoma viejo, incompatible con el
código de hoy) y "despega siempre, resuelve una ruta entera, esquiva un poste
mejor que el clásico". Pero **no se puede sacar ninguna conclusión de los
ceros** hasta reentrenar con la regla 3.7 puesta: son las fases que el
currículo viejo salteó. Se probó una segunda corrida con otra semilla y dio
peor (0/5 en A a B, 0/3 esquivando), así que el que quedó instalado es el
menos malo de dos, no un resultado que se defienda solo.

**Actualización — la corrida con autoridad angular 0.80 rad y filtro
compartido llegó lejísimos, y se cortó por costo de oportunidad en el
peldaño 12.** Con el residuo subido (ver comentario de `CORRECCION_ANG` en
`gym3d.rs`) y el bug del filtro pasabajos arreglado, la misma semilla que
antes se clavaba en el peldaño 5 llegó al **peldaño 11 aprobado** en 16
generaciones — esquiva dos postes, despega, se posa desde baja altura — y
entró en el 12 ("posarse de verdad", desde más arriba y con la plataforma
corrida).

Ahí se clavó **600 generaciones** en 18.8 de 900, con la mutación en su tope
todo ese tiempo. El diagnóstico arreglado (ver más abajo) identificó la causa
exacta: el peldaño 11 ("posarse desde bajo") y el 12 son la MISMA maniobra en
dos dificultades, compitiendo por el mismo espacio de pesos — perfeccionar la
difícil desaprende la fácil, y el examen de repaso lo capta y lo penaliza
correctamente (eso SÍ funciona), pero nada empuja a la búsqueda a encontrar
una solución que sirva para las dos versiones a la vez. Es candidato a la
misma categoría que el semáforo de persecución (7.4): una escalera con dos
peldaños casi idénticos puede pedirle a un genoma binario más margen
compartido del que tiene.

A diferencia del semáforo, acá NO hay una imposibilidad geométrica medida —
el propio peldaño mejoró de 10.8 a 38.5 en la misma ventana en que el
"mejor" reportado quedó congelado en 18.8 (el examen de repaso, no el
peldaño propio, definía el mínimo). Cortado por costo de oportunidad
declarado, no por diagnóstico técnico, con `saltar` — el genoma congelado en
`/tmp/mision_v4_peldano12_congelado.adn` para quien quiera retomarlo.

**`cargo test --release` NO reconstruye `target/release/dron`.** Costó una
corrida entera: se editó `mision.rs`, se corrió `cargo test --release` (58
pruebas verdes), se relanzó el entrenamiento... y el entrenamiento usó el
binario de la noche anterior, porque `cargo test` compila el arnés de pruebas
(`target/release/deps/dron-<hash>`), no el ejecutable. El síntoma que lo
delató: el peldaño trabado reportaba **exactamente el mismo número** que antes
del arreglo — con más reloj la trayectoria tiene que dar otro número, así que
un valor idéntico bit a bit significa que el cambio no está corriendo.

Antes de lanzar un entrenamiento después de tocar código:

```bash
cargo build --release            # NO alcanza con cargo test
ls -la --time-style=+%H:%M:%S target/release/dron src/loquetocaste.rs
```

Si el binario es más viejo que el fuente, lo que va a correr no es lo que
escribiste.

**INTENTO FALLIDO, con la medición que lo descartó (regla 3.4): el reloj de
la posada NO era el problema.** La hipótesis era buena y la evidencia parecía
sólida — el peldaño 12 terminaba "sin reloj" a ~1.18 m, consistente en los
tres niveles de ruido, y recorría hasta 4× más distancia que el peldaño 11
con el mismo presupuesto de 2400 pasos. Se hizo el reloj proporcional a la
distancia (2400 → hasta 3053 pasos). Resultado:

```text
  reloj viejo (2400):        queda a 1.18 / 1.14 / 1.19 m
  reloj nuevo (hasta 3053):  queda a 1.18 / 1.18 / 1.16 m
```

**27% más de tiempo, distancia final idéntica.** Revertido.

Lo que la traza mostró, y que ninguna de las dos hipótesis anteriores decía:

```text
   t      x      y      z
   0s   0.00   0.00   3.00     arranca a 3 m
   2s  -0.73  -0.05   0.72     baja rápido, pero se DESVÍA de lado
   4s  -0.80  -0.17   0.81
   6s  -0.80  -0.26   0.81     se queda FLOTANDO a 81 cm
  12s  -0.68  -0.49   0.82     10 s sin bajar un centímetro, derivando en y
```

No se queda sin reloj: **se queda flotando a 82 cm y se niega a completar el
descenso** — la deuda #2 de esta misma sección, que ya decía "nunca llega a
tocar el suelo, falta terminar de bajar". Y flotar es la jugada
matemáticamente correcta dado el castigo: `d.caido` hace
`aptitud * 0.1 - 60`, así que con ~91 puntos acumulados, tocar mal cuesta
−51 y flotar paga 91. El punto de equilibrio está en ~15% de probabilidad de
posarse limpio; por debajo de eso, **la evolución hace bien en no arriesgar**.

Y la traza dice por qué no supera ese 15%: llega a 82 cm **derivando de lado**
(y va de −0.05 a −0.49 m) y no se puede posar mientras se desliza.

**La pista que sale de la industria** (regla 3.17): el aterrizaje de precisión
de PX4 es una máquina de estados que **alinea primero y desciende después**
(acercamiento horizontal → confirmar que el objetivo está dentro de
`PLD_HACC_RAD`, 0.2 m por defecto → recién ahí descender). Nuestro dron hace
lo contrario: baja primero y deriva después. Si alguien retoma esto, ése es
el candidato, no el reloj ni la autoridad angular.

**Correr el informe antes de creerle a nada de acá:**
```bash
cargo run --release -- capacidades
```

**RE-MEDIDO con el límite de sacudida puesto (7.5), que es lo honesto para
hardware.** La tabla de arriba se midió con la aptitud ciega al temblor; ésta
es la misma prueba con la red obligada a mover el mando a velocidad
físicamente plausible:

> **Tabla histórica (2 de 3 en este archivo).** Mide el efecto del límite de
> sacudida sobre el genoma de entonces. NO es el estado de hoy — ver
> `CLAUDE.md`, «Estado hoy».

```
   capacidad               sin límite   CON límite   clásico
   despegar                    2/3         2/3         3/3
   ir de A a B y frenar        2/5         1/5         5/5
   quedarse quieto             2/3         1/3          —
   posarse entero              0/3         0/3         1/3
   recuperar de vuelco         2/3         2/3          —
   esquivar obstáculos         0/3         0/3         0/3
   entregar un paquete         0/3         0/3         1/3
```

Dos cosas que hay que decir sin suavizar:

1. **Bajo la restricción honesta el genoma empeora en dos capacidades y no
   mejora en ninguna.** Parte de lo poco que lograba lo lograba temblando.
2. **El piloto clásico le gana en TODO lo que se mide contra él**, y en «ir de
   A a B» por 5/5 contra 1/5. La afirmación defendible del proyecto hoy no es
   "la red vuela", es "la red corrige al que vuela, y en persecución esa
   corrección a veces suma" — que es exactamente lo que dice la regla 3.15 y
   nada más que eso.

**MEDICIÓN NUEVA (sesión de alinear-antes-de-descender), que supera a toda la
tabla de arriba.** Genoma: el checkpoint del peldaño 11 de la corrida con
formulación híbrida + autoridad 0.80 rad. Vara: el clásico CON la regla de
aproximación.

> **Tabla histórica (3 de 3 en este archivo).** Es la más parecida al estado de
> hoy, pero su fila «quedarse quieto 3/3» se midió con el instrumento roto que
> la auditoría (7.10) encontró: ese 3/3 es en realidad **0/3**. No citar esta
> tabla; el vigente está en `CLAUDE.md`, «Estado hoy».

```
   capacidad               antes    AHORA    clásico
   despegar                  3/3      3/3      3/3
   ir de A a B y frenar      1/5      5/5      5/5
   quedarse quieto           1/3      3/3       —
   posarse entero            0/3      2/3      3/3
   recuperar de vuelco       1/3      2/3       —
   esquivar obstáculos       1/3      3/3      0/3
   entregar un paquete       0/3      1/3      1/3
```

Dos resultados que hay que separar del resto:

> **⛔ REFUTADO — el 3/3 era el conjunto de entrenamiento.** La prueba de
> esquivar corría con `gym3d::obstaculos(i)`, los MISMOS postes del gym. Con
> postes nuevos: genoma 1/3, clásico 2/3. Ver 7.17. Lo de abajo se deja por la
> regla 3.4, no vale citarlo.

1. **Esquivar obstáculos: 3/3 contra 0/3 del clásico.** La red hace perfecto
   algo que la vara honesta NO PUEDE HACER. Es la evidencia más fuerte que dio
   este proyecto de que la red aporta capacidad propia, no sólo iguala al
   modelo físico.
2. **Entregar un paquete: 1/3.** Primera vez que el genoma completa una
   entrega entera — la tarea original del proyecto, que llevaba meses en 0/3.

**La honestidad que va con el número**: parte de la mejora es del genoma
(formulación híbrida, autoridad angular) y parte es de haber ARREGLADO LA VARA
(el clásico pasó de 1/3 a 3/3 en posarse con la regla de aproximación). Los
dos efectos están mezclados en esta tabla y separarlos necesitaría re-medir el
genoma viejo contra la vara nueva, que no se hizo.

**Y lo que sigue trabado, que es lo de siempre**: el olvido catastrófico. El
entrenamiento llega al peldaño 12 y ahí el examen de repaso lo hunde porque
desaprendió los once de atrás — medido, el campeón sacaba −1.6 en «rumbo», un
peldaño trivial que aprueba en la generación 2. Es el agujero del `min`
documentado en 3.8: mientras el peldaño actual sea el peor término, degradar
lo viejo sale gratis. Ése es el bloqueo real, no la mecánica de aterrizar.

**Deuda abierta**, ahora con el número de `capacidades` sobre rutas nuevas al
lado de cada una (no las de entrenamiento). **La 0 va primero y bloquea a las
demás** — por la regla 3.7, no se investiga ninguna de las de abajo hasta
haberla hecho:

0. **Reentrenar de cero con el currículo que no saltea.** Todo lo que sigue se
   midió sobre un genoma que se salteó fases. Hasta no tener uno entrenado con
   la regla puesta, no se sabe cuáles de estos números son limitaciones reales
   y cuáles son consecuencia de los peldaños que nunca aprobó.

   **Quedó corriendo** al cierre de la sesión, con `nohup` para que sobreviva
   a la terminal:

   ```bash
   ./target/release/dron entrenarmision 4000 semilla=1 salida=/tmp/.../final.adn
   ```

   Cómo retomarlo, en orden:

   ```bash
   grep -E "peldaño [0-9]+ \(|TRABADO" final.log | tail -20   # ¿hasta dónde llegó?
   grep -A5 TRABADO final.log | tail -7                       # ¿por qué se trabó?
   ```

   Si llegó lejos: instalarlo (`cp final.adn genomes/DRON3D_64.adn`), correr
   `capacidades` **y recién ahí** creerle a los números, con el visor
   reiniciado. Si se trabó temprano: la telemetría ya dice en qué caso y si es
   navegación, percepción o precisión — atacar eso, no adivinar.

   **El costo del examen de repaso, calculado de la estructura real** (pasos de
   simulación por individuo y generación, no una impresión — antes acá decía
   "~4×" a ojo y estaba mal):

   ```text
     peldaño   sin repaso   con repaso   factor
           4       14,400       48,000     3.3x
           6       14,400       76,800     5.3x
           8       21,600      120,000     5.6x
          10       14,400      156,000    10.8x
          12       21,600      199,200     9.2x
          14       81,000      319,800     3.9x
   ```

   El factor no crece parejo porque depende de cuántos pasos dura el peldaño
   actual: el 10 (despegar, episodios cortos de 1600 pasos) arrastra diez
   peldaños de repaso y queda 10.8×, mientras que el 14 (entrega, 9000 pasos)
   diluye el repaso en su propio costo y queda en 3.9×. Ritmo real medido en la
   corrida en curso: **~18 generaciones por minuto** en el peldaño 6.

   **Actualización, misma sesión:** esa corrida se cortó a mano en la
   generación 2620, clavada 1860 generaciones sin mover un decimal en
   "ir a B" al 100% (ver `mision::PELDANOS`). Se agregó un escalón intermedio
   al 85% —la escalera pasó de 13 a 14 peldaños, así que **cualquier ejemplo
   de log de más arriba en este archivo con "/13" es de antes de este
   cambio**, no un error de tipeo— y se relanzó desde cero. Es la corrida que
   hay que retomar con los comandos de arriba.
1. **La entrega completa no sale (0/3).** Nunca aprobó los peldaños de los que
   depende, así que este número no dice nada todavía. El entrenamiento llegó a
   este peldaño en la generación 350 de 1200 y se cortó en la 680, con la
   aptitud clavada en 602.5 desde la 360.
2. **Posarse sin romperse (0/3, clásico 1/3).** El campeón toca a "0.00 m/s"
   en las tres pruebas — eso no es un aterrizaje perfecto, es que **nunca
   llega a tocar el suelo**: se queda flotando cerca. Falta terminar de bajar,
   no falta suavidad.
3. **Ir de A a B fuera de las rutas de entrenamiento (1/5).** Resuelve la
   primera (la más parecida a lo que entrenó) y no las otras cuatro, que están
   más lejos o en otra dirección. Generaliza poco todavía.
4. **Esquivar dos postes seguidos — y la causa probable es de PERCEPCIÓN, no
   de maniobra.** Con UN poste la ruta 1 se resolvía en los tres niveles de
   ruido (1225–1235 de aptitud). Con dos, el genoma NO choca pero tampoco llega
   (1/3). Mirando `Camara::ver_con`, el porqué salta a la vista:

   ```rust
   if peor.map_or(true, |(a, _)| amenaza > a) { peor = Some((amenaza, rumbo)); }
   ```

   **El dron ve UN SOLO obstáculo a la vez** — el más amenazante del instante,
   reducido a dos números (rumbo + proximidad). Cuando esquiva el primer poste,
   el segundo pasa a ser "el peor" y el primero **desaparece de su percepción**.
   No tiene mapa ni memoria del terreno. Pedirle que encadene dos rodeos con esa
   entrada es pedirle que recuerde algo que no puede ver.

   Peor: **no hay ningún bit de elevación del obstáculo**. La física SÍ permite
   pasarle por arriba a un poste (`Obstaculo::distancia` da infinito si el dron
   va más alto que el poste), pero el dron no tiene cómo saber si el poste
   termina abajo suyo o sigue subiendo. Hay una solución válida —subir— que es
   **invisible para el piloto**. Esa asimetría entre lo que el mundo permite y
   lo que el dron percibe es del proyecto, no del genoma.

   El reemplazo natural es un **anillo de proximidad**: N sectores alrededor del
   dron, cada uno con lo más cerca que hay en esa dirección, más un canal de
   "¿me lo puedo saltar por arriba?". Es 360° por construcción, ve todos los
   obstáculos a la vez, y es exactamente cómo funciona un ojo compuesto y cómo
   se reparten los sensores de proximidad en un dron real. **La tensión a
   resolver midiendo es el presupuesto**: hoy la visión de obstáculos usa 13 de
   96 bits y un anillo cuesta bastante más, así que hay que sacárselos a otro
   campo y medir qué se pierde. Sigue pendiente — lo de abajo es el paso
   anterior y más barato, no un reemplazo de esta idea.

   **PASO INTERMEDIO, implementado (pedido explícito del usuario: buscar cómo
   lo resuelve la industria — regla 3.17).** Antes de construir un anillo
   (que sí toca el presupuesto de bits), se atacó la mitad barata del
   problema: `Camara::ver_con` se quedaba con el obstáculo MÁS amenazante y
   tiraba el resto — con dos postes a la vista, el que se esquivaba primero
   desaparecía de la percepción apenas dejaba de ser "el peor". Es
   exactamente el patrón que la industria ya identificó como el débil: *"a
   diferencia de Skydio, los drones DJI pueden ser puramente reactivos y
   esquivar un obstáculo sólo para chocar con otro"* (dronedj.com, sobre por
   qué Skydio construye un mapa 3D en vez de reaccionar obstáculo por
   obstáculo). La técnica barata que sí cabe en el presupuesto del chip —de
   la familia *artificial potential fields* / *velocity obstacles*, estándar
   en robots reactivos de bajo costo— es **sumar** la amenaza de cada
   obstáculo visible en vez de quedarse con el máximo: `obs_proximidad` pasa
   a ser la suma de amenazas (dos postes moderados pesan más que uno solo muy
   cerca) y `obs_horizontal` el rumbo promedio ponderado por amenaza (dos
   postes simétricos dan rumbo ≈ 0 — "el hueco está al centro" — en vez de
   mentir "sólo hay uno"). Sigue siendo el mismo bucle y los mismos DOS
   números; no toca el presupuesto de 96 bits.

   **Esto invalida el genoma instalado en ese peldaño** (sección 4: cambiar
   el código de sensores invalida lo entrenado con el viejo). Reentrenamiento
   del peldaño 8 lanzado en background, sembrado desde `DRON3D_64.adn`
   (semilla=1, `salida=` a un archivo aparte del scratchpad, nunca al
   genoma instalado — regla 3.11).

   **Primera corrida: NO CONCLUYENTE, y el motivo quedó medido, no
   supuesto.** El campeón sembrado ya venía con margen de sobra en el propio
   peldaño (1383.5 de 900 — el esquive en sí no era el problema) y el
   entrenador quedó **clavado 390 de 400 generaciones sin cambiar un solo
   bit del genoma** (mismo md5 desde la gen ~10 hasta el final). El
   diagnóstico automático (`mision::telemetria_estancado`) dice por qué: el
   mínimo de la aptitud lo definía el **examen de repaso de «rumbo»**
   (escalado a 1108.2), un peldaño viejo y trivial — no el esquive.

   **Investigado a fondo (el usuario pidió explícitamente no dejarlo así):
   el techo de «rumbo» es REAL, no un bug de escala.** Se midió el piloto
   CLÁSICO —modelo físico perfecto, cero ruido de aprendizaje— en el mismo
   peldaño: `1132.4` de peor caso, contra `1108.2` del campeón. Menos del 2%
   de diferencia. Ni siquiera la vara honesta pasa de ~1135 en esta
   mini-tarea. Es el mismo fenómeno de 7.8 (el imán de la tolerancia) en
   otro lugar: el examen no está roto, está saturado — no hay mucho más
   margen que encontrar ahí, para nadie.

   **Conclusión práctica: la herramienta de medición era la equivocada, no
   el mecanismo.** `entrenarmision ... rapido` salta el examen de repaso por
   completo (usa sólo `Ruido::REAL`, sin el `min()` contra peldaños
   viejos) — con eso el «mejor» arranca directo en 1370.6, el esquive puro,
   sin nada tapándolo.

   > **⛔ CORREGIDO — el "sin cambio" de abajo era el argumento ignorado.**
   > Lo que sigue decía que el genoma nuevo daba números idénticos dígito por
   > dígito. Falso, y por la peor razón posible: `capacidades` tenía el genoma
   > CABLEADO e ignoraba `genoma=RUTA` **en silencio**, así que esa corrida
   > midió el genoma instalado, no el entrenado. Por eso coincidían dígito por
   > dígito — eran el mismo archivo. Con el argumento ya funcionando
   > (commit de la auditoría), el genoma entrenado da **0.13 / 0.06 / 0.95 m**
   > contra 0.23 / 0.17 / 0.28 del instalado: sigue 3/3, pero NO es el mismo
   > vuelo. El veredicto real está al pie de esta entrada.
   >
   > Es la cuarta aparición del mismo patrón en el proyecto y la razón de ser
   > de la regla 3.18: **dos mediciones que coinciden dígito por dígito no son
   > dos confirmaciones, son la misma medición hecha dos veces.**

   **Resultado final, con las dos corridas ya cerradas: MEJORA CHICA en el
   entrenamiento, SIN CAMBIO en las rutas de `capacidades`.** La corrida
   limpia sí se movió (1370.6 → 1413.2, +3%, en la generación 10, después
   volvió a clavarse) — un genoma genuinamente distinto del sembrado (md5
   `787b0a0e...` contra `bf4b12d0...`). Pero corriendo `capacidades` sobre
   ESE genoma nuevo, las tres rutas de esquive dan **exactamente los mismos
   números que antes** (0.23 / 0.17 / 0.28 m, 3/3) — dígito por dígito,
   igual que con el genoma viejo. La lectura más honesta: la mejora que sí
   hubo pasó en las misiones DE ENTRENAMIENTO del peldaño 8
   (`gym3d::obstaculos` índices 0-2), que no son las mismas rutas que arma
   `capacidades` de forma independiente — y es muy posible que en las rutas
   de `capacidades` los dos postes nunca estén dentro del campo de visión
   AL MISMO TIEMPO, en cuyo caso sumar en vez de tomar el máximo no cambia
   nada (con un solo obstáculo a la vista, suma y máximo dan lo mismo por
   construcción). **No se puede afirmar que el cambio ayude a esquivar dos
   postes simultáneos** con lo medido hasta acá — falta un caso de prueba
   donde los dos postes estén garantizado a la vista juntos, algo que
   todavía no se construyó. El código queda igual (sigue siendo la técnica
   correcta por la razón de 3.17), pero la validación empírica sigue
   abierta, más acotada que al principio: se sabe que no rompió nada
   (3/3 se mantiene) y que ayuda en AL MENOS un escenario de entrenamiento,
   pero no está demostrado que ayude en el caso específico que motivó el
   cambio.
5. **El camino es feo** cuando sí llega: serpentea en vez de ir derecho.
   `SOBRA_TIEMPO` premia terminar rápido, así que hay gradiente para
   mejorarlo; falta correrlo con más generaciones.
6. **Recuperación de vuelcos: 1/3, sin cambios.** Sigue siendo el intercambio
   real contra la agilidad que ya se documentaba antes de esta sesión.
7. ~~Genomas bio desactualizados.~~ **CERRADO, no reentrenado — borrado.** Se
   corrió el experimento final con el instrumento arreglado (formulación
   híbrida, filtro pasabajos, límite de sacudida, 3 semillas) y dio negativo
   para los dos circuitos: ver 7.9. `bio.rs`, `gymbio.rs`, los `DRON3D_BIO*`
   /`DRON3D_CX*`/`DRON3D_NULO*` y `ARCHITECTURE.md` se sacaron del repo con
   esa medición en la mano.
8. **El A/B end-to-end del código de sensores.** El cambio está justificado por
   una medición **directa del mecanismo** (correlación de Hamming 0.378 →
   0.990, y la tabla de distancias de `codigo.rs`), que es evidencia fuerte
   pero no es lo mismo que comparar dos entrenamientos. `DRON_CODIGO=binario`
   deja correr el código viejo para exactamente eso:

   ```bash
   for s in 1 2 3; do
     DRON_CODIGO=binario ./target/release/dron entrenarmision 250 semilla=$s salida=/tmp/bin_$s.adn
                         ./target/release/dron entrenarmision 250 semilla=$s salida=/tmp/ter_$s.adn
   done
   ```

   Lo que hay que comparar es **hasta qué peldaño llega cada uno**, no la
   aptitud final: las aptitudes de peldaños distintos no son comparables entre
   sí (lo dice la sección 4).

---

### 7.10 La auditoría de reglas, y el instrumento que no medía lo que decía

Pedido del usuario: auditar las reglas —escritas a lo largo de meses, varias
cuando el proyecto tenía otro enfoque— contra el proyecto de hoy, y ver hasta
qué punto estorban. La conclusión general es que las reglas NO estorban casi en
ningún lado: lo que estorbaba era el archivo que las contenía. Pero buscando
evidencia para la auditoría apareció un bug de medición que vale más que toda
la revisión de prosa.

**El hallazgo: `capacidades` medía distancia y la reportaba como quietud.**

La capacidad 3 («¿se queda quieto cuando llega?») acumulaba, en la ventana
final, `suma_deriva += dist` — donde `dist` es la distancia AL OBJETIVO. O sea
que el número que se imprimía como «deriva» era la distancia media al punto,
promediada. Consecuencias, las dos malas:

- **No medía quietud en absoluto.** Un dron orbitando a 0.89 m de radio a toda
  velocidad daba el mismo número que uno clavado inmóvil a 0.89 m.
- **Era una copia peor de la capacidad 2**, que ya mide distancia al punto. Dos
  de las siete capacidades del informe medían la misma cantidad.

La señal estaba a la vista desde 7.8 y nadie la leyó: esa sección reportó
«deriva en los últimos 3 s: 0.89 / 0.89 / 0.89 m» al lado de «queda a 0.89 m».
Tres pares de números idénticos dígito por dígito no son una coincidencia —
eran la misma cantidad impresa dos veces.

**Lo que dio al arreglarlo.** `excursion` = lo más que se aleja el dron de su
propia posición media en la ventana (radio de la nube). Un dron inmóvil da ~0
esté donde esté; uno que orbita da el radio de su órbita.

```text
   ruta 1   genoma: se mueve 0.55 m (a 0.89 m del obj)  NO  ·  clásico: 0.02 m (a 0.01 m)  SÍ
   ruta 2   genoma: se mueve 0.61 m (a 0.89 m del obj)  NO  ·  clásico: 0.02 m (a 0.01 m)  SÍ
   ruta 3   genoma: se mueve 0.54 m (a 0.89 m del obj)  NO  ·  clásico: 0.02 m (a 0.01 m)  SÍ

   quedarse quieto:   genoma 0/3   ·   clásico 3/3      (antes decía: genoma 3/3)
```

**El 3/3 pasó a 0/3, y el piloto clásico saca 3/3 clavándose a 1 cm del punto.**
La órbita de 7.8 no sólo no estaba cerrada: es ~27× peor que la vara honesta en
puntería y ~28× en quietud, y llevaba meses puntuando como capacidad cumplida.

**El umbral se calibró contra la vara, no a gusto** (que es la mitad nueva de
la regla del instrumento): `QUIETO = 0.50 m` es **25× más permisivo** que el
0.02 m del clásico, a propósito, para que reprobar no pudiera ser un artefacto
de haber apretado la vara. El genoma reprueba igual.

Para poder calibrar contra la vara sin duplicar la cadena de control se agregó
`capacidades::volar_con(..., usar_red: false)`: apaga las cuatro salidas y, por
la identidad de la regla 3.15, lo que vuela es exactamente el piloto clásico.
Es el mismo truco que `la_mezcla_vacia_es_exactamente_el_clasico` ya usaba en
persecución, ahora disponible para el informe de entrega.

**Por qué esto justifica una regla nueva (3.18) y no sólo un arreglo.** Es la
TERCERA vez que el proyecto saca una conclusión con un instrumento roto:

1. **El veredicto del conectoma de la mosca** (3.9): medido con el código de
   sensores binario y las salidas con tanh saturada, las dos cosas que después
   se descubrieron rotas. Hubo que repetir el experimento entero un año después.
2. **La telemetría de la mezcla** (7.4, bug 3): `telemetria_estancado` evaluaba
   al campeón como piloto monolítico mientras la aptitud lo medía dentro de la
   mezcla. Reportaba 11951.0 donde el número real era 12182.6.
3. **`deriva_media`** (acá): medía distancia con nombre de quietud.

Tres incidentes, ninguna regla que los cubriera. Las reglas 3.2, 3.5 y la
sección 4 apuntan todas HACIA los instrumentos —la vara es sagrada, medí no
argumentes, cerrá con `capacidades`— pero ninguna apuntaba A los instrumentos.
El proyecto confiaba en sus aparatos de medición sin auditarlos nunca.

**Hallazgo secundario: la prueba del presupuesto de cómputo es inestable.**
`costo_de_computo` falló una vez de ~7 corridas de la batería completa, y
corriéndola sola cinco veces seguidas pasó las cinco. Es una prueba de reloj de
pared con 17× de margen: bajo contención de CPU el margen se come solo. No es
un bug de la regla 3.14 (el presupuesto sigue siendo correcto) sino de cómo se
la hace cumplir — si vuelve a molestar, medir la mediana de varias corridas en
vez de una sola. Queda anotado, no arreglado: falla del lado seguro.

**Lo que la auditoría NO encontró**, y vale decirlo porque era lo que se fue a
buscar: ninguna regla resultó ser un lastre que hubiera que sacar. Las que se
tocaron se ampliaron o se afilaron; una sola (la vieja 3.10, la conexión con la
mosca) se retiró del contrato, y porque su objeto de estudio ya no existe en el
repo — su parte viva se fusionó en 3.17. El problema real era de forma: 2009
líneas cargadas en cada sesión, 62% de ellas historia, contradiciéndose entre
sí. De ahí la separación en `CLAUDE.md` (contrato) y este archivo (registro).

---

### 7.11 Tres preguntas del usuario sobre el grafo, y media capa que no se usa

El usuario miró el panel «nuestra red» del visor y preguntó por qué no ve
conexiones entre sensores/memoria y las capas, ni entre la capa de estrategia y
las salidas; por qué sólo hay 4 salidas teniendo 64 neuronas; y por qué no hay
más capas. Contestado contra `csrc/iafz_64.c`, no de memoria. **Dos de las tres
observaciones eran correctas y destaparon un bug de dibujo y un hallazgo de
arquitectura.**

**El flujo real del kernel**, para que quede escrito una vez:

```text
  sensores 96b  +  memoria 32b   =  contexto de 128 bits
        ↓  64 neuronas «hidden»   (XNOR+popcount contra 128b de pesos)
  l1_out 64 bits
        ↓  64 neuronas «strategy» (XNOR+popcount contra [l1_out, 0])
  l2_out 64 bits
        ├─→ out_action_weights[4][64]   →  4 salidas float
        └─→ memory_weights[32][64]      →  32 bits de memoria del próximo tick
```

**1. El grafo dibujaba UN haz de los cuatro.** Las conexiones entrada→oculta,
estrategia→salidas y estrategia→memoria existen en el kernel y no se dibujaban:
el código sólo trazaba aristas oculta→estrategia. Por eso la imagen sugería que
los sensores no entran y que la estrategia no sale a ningún lado — las dos cosas
falsas. Es un defecto del instrumento, de la misma familia que 7.10: **dibujar
de menos engaña igual que dibujar de más.** Arreglado: los cuatro haces, con el
mismo criterio honesto que ya usaba (se traza el par sólo si los dos extremos
están prendidos en esta decisión), y el haz de entrada submuestreado para que se
lea como haz y no como mancha. Verificado en vivo: 48 bits de sensor, 17 de
memoria, 35 ocultas y 33 de estrategia prendidas.

**2. La capa de estrategia NO ve los sensores ni la memoria — y tiene el lugar
vacío esperando.** Acá el usuario tenía razón y es el hallazgo bueno:

```c
if (THINK_128(l1_out, 0ULL, swarm[id].strategy[i])) ...
```

La neurona de estrategia compara sus **128 bits** de pesos contra `[l1_out, 0]`.
La palabra alta es **constante cero**, así que ese término vale
`popcount(~(0 ^ w1))` = `64 − popcount(w1)`: un número fijo por neurona. O sea
que **los 64 bits altos de cada neurona de estrategia no codifican nada más que
un sesgo escalar** — 4096 bits (512 B de genoma) haciendo el trabajo de 64
números, y encima redundante con el `threshold` que cada neurona ya tiene. La
mutación gasta ahí sin que pueda aprender nada nuevo.

**La mejora es gratis en cómputo**: ese popcount YA se está pagando. Poner datos
reales en la palabra alta —los 32 bits de memoria recurrente más 32 de sensor,
por ejemplo— le da a la estrategia una conexión de salto al contexto **sin un
solo popcount extra y sin un byte extra de flash**, porque los pesos ya existen.
Es candidato fuerte y barato. Invalida los genomas (cambia el significado de una
entrada), así que se mide reentrenando, no clampeando.

**3. Cuatro salidas no es una restricción de presupuesto.** Las 64 neuronas no
son salidas, son la representación interna que alimenta a las 4. Son cuatro
porque son las cuatro consignas que acepta un controlador de vuelo real
(regla 3.1: alabeo, cabeceo, guiñada, altura), no porque no entren más. De
hecho es lo más barato del kernel: **una salida más cuesta 256 bytes y ~32 sumas
flotantes**, contra los 8192 bytes y ~1024 sumas que ya se gasta la proyección
de memoria. Si aparece algo más que valga la pena mandar (un canal de
confianza/ganancia que module cuánto se le hace caso al residuo es el candidato
obvio), el costo no es el obstáculo.

**4. Más capas: nadie lo midió, y el presupuesto ya no es la excusa.** Una
tercera capa binaria cuesta 128 popcounts y 1088 bytes — barato al lado de la
memoria. Lo que bloqueaba era la regla 3.14 calculada contra un Cortex-M0 de
48 MHz; con el piso movido a clase ESP32 por decisión del usuario, el margen
pasa de 17× a ~57× (aritmética, no medición — ver 3.14). **Pero el orden
correcto es primero usar la media capa que ya está pagada y vacía (punto 2), y
recién después agregar una tercera.** Agregar capacidad encima de capacidad
desaprovechada es gastar dos veces.

---

### 7.12 Veredicto del reentrenamiento de esquive: cambió el vuelo, no mejoró

La corrida limpia del peldaño 9 («esquivar») **terminó** sus 400 generaciones
en 268 s. Lo que hizo, medido del log:

```text
  gen   0   mejor 1370.6     (sembrado desde genomes/DRON3D_64.adn)
  gen  10   mejor 1413.2     +42.6  (+3.1%)
  gen 399   mejor 1413.2     clavado 390 generaciones
```

Un solo movimiento, en la generación 10, y 390 generaciones planas después. El
genoma sí cambió de bits: md5 `787b0a0e…` contra `481e7901…` del sembrado.

**`capacidades` sección 6, genoma entrenado contra el instalado** (distancia
mínima al poste: más grande = más margen):

```text
  ruta                 instalado   entrenado
  1 · un poste            0.23 m      0.13 m    ← menos margen
  2 · dos postes          0.17 m      0.06 m    ← menos margen (6 cm, roza)
  3 · dos postes          0.28 m      0.95 m    ← más margen
  aprobadas                  3/3         3/3
```

**Veredicto, sin inflarlo: no mejoró.** Las tres rutas se siguen aprobando, o
sea que no rompió nada, pero el margen empeora en dos de tres y en la ruta 2
queda en 6 cm, que es rozar. La única que mejora lo hace fuerte (0.28 → 0.95).
Es un vuelo distinto con el mismo resultado, no un vuelo mejor: el +3.1% de
aptitud de entrenamiento **no se tradujo** a las rutas de `capacidades`, que se
arman aparte.

Advertencias que van con el número, para que nadie lo cite de más: es **una
semilla**, y las rutas de `capacidades` no son las misiones de entrenamiento
del peldaño (sección 4). Lo que sí queda establecido es que la afirmación vieja
—"da exactamente los mismos números"— era un artefacto del argumento ignorado,
no un resultado.

---

### 7.13 El visor nunca reseteaba la memoria recurrente entre vuelos

Reporte del usuario: el dron esquivaba el poste perfecto la primera vez y,
con el tiempo, "se ponía tonto"; reiniciar la entrega lo arreglaba un rato.

`grep -rn reset_memoria src/*.rs` muestra que TODO camino de entrenamiento y
medición (`gym.rs`, `capacidades.rs`, `etapas.rs`, `persecucion.rs`,
`gym3d.rs`, `mision.rs`) resetea los 32 bits de memoria recurrente al empezar
cada episodio. `web.rs` era el único que no — se resetea sólo al (re)cargar el
genoma, nunca al reposicionar el dron para una entrega nueva.

Medido en vivo antes del arreglo: justo después de una entrega CUMPLIDA,
`/estado` mostraba `memRed` con 18 de 32 bits en 1 (no debería importar,
estaba "parado"); al armar la siguiente entrega (dron ya reposicionado en el
suelo, `reiniciar_control` ya corrido), el primer estado leído traía un
patrón casi idéntico — la memoria de la entrega anterior seguía ahí.

Consecuencia: cada entrega repetida en el visor arrancaba desde un contexto
que la red NUNCA vio en entrenamiento (ahí siempre arranca en 0), así que el
comportamiento diverge más cuantas más entregas se acumulan sin recargar el
genoma — que es exactamente lo que describió el usuario.

Arreglado en los cuatro lugares que reposicionan el dron para un vuelo nuevo:
`armar_entrega` (el botón de entrega), `/punto?cual=a`, `'r'` (reset
completo) y la entrada a modo persecución. `VISOR`/`VISOR_PERSEC` se subieron
a constantes de módulo para que `armar_entrega` (un método de `Mundo`, sin
acceso a los `const` locales de `volar()`) pudiera llamarlos.

No se pudo aislar una prueba en vivo perfectamente limpia (las decisiones
corren a 100 Hz por defecto, así que entre armar la misión y el siguiente
`curl` ya pasaron varias) — la evidencia que sostiene el arreglo es el código:
`reset_echo_iafz_64` pone `recurrent_32[id]=0`, y ahora los cuatro sitios lo
llaman igual que ya lo hacía cada episodio de entrenamiento.

---

### 7.14 El dron no aterrizaba nunca: el descenso se apagaba antes de tocar

El reentrenamiento se clavó **330 generaciones en 20.4 de 900** en el peldaño 12
(«posarse» con la plataforma corrida). El usuario pidió auditar el peldaño antes
de seguir moliendo. Lo que apareció no era un peldaño difícil: era un bug que
hacía imposible aterrizar, y que estuvo escondido meses.

**La señal que lo delató fue la no-monotonía del ruido.** Midiendo el peldaño
con el piloto CLÁSICO (modelo perfecto, cero ruido de aprendizaje):

```text
  misión 3      limpio 106.6   ·   real 1135.1   ·   duro 88.3
```

Aprobar con sensores SUCIOS y reprobar con sensores LIMPIOS es imposible si la
dificultad fuera real. Eso es la firma de un bug, no de un peldaño exigente.

**La traza, con sensores limpios:**

```text
   0 de 1 tramos · 12.0 s · "se quedó sin reloj"
   terminó a 0.00 m del punto
   traza final:  x=-1.50  y=2.00  z=0.00  |v|=0.03 m/s
```

Posado, quieto, clavado en el punto — y **0 de 1 tramos**. El dron hacía un
aterrizaje perfecto y no se lo acreditaban.

**La causa.** `altura_de_aproximacion` pedía consigna `0.0` y el lazo de altura
es proporcional (ganancia 1.6): la velocidad de descenso es −1.6·z, o sea que
**se apaga junto con la altura**. Es una exponencial: z tiende a cero y no lo
cruza nunca. `Dron3::tocar_suelo` sale por `if self.z > 0.0 { return; }`, así
que `apoyado` nunca se prendía y el tramo de posada nunca se completaba. El dron
quedaba flotando a milímetros del piso quemando el reloj. Con ruido, el ruido lo
empujaba abajo y le hacía cruzar el cero — por eso aprobaba sucio y reprobaba
limpio.

**La consecuencia de fondo, que es peor que el peldaño trabado: no había
gradiente para aprender a aterrizar.** Aterrizar perfecto y flotar para siempre
puntuaban IGUAL, así que la evolución no tenía ninguna razón para aprender a
tocar el suelo. Eso explica los meses de 0/3 en «posarse entero».

**El arreglo, de la industria (regla 3.17).** ArduPilot y PX4 no usan ley
proporcional en el tramo final: usan velocidad de descenso CONSTANTE
(`LAND_SPEED`, 50 cm/s) por debajo de una altura (`LAND_ALT_LOW`). Se pide la
consigna 0.25 m bajo tierra, en RAMPA desde 0.5 m de altura, lo que sostiene
~0.40 m/s de descenso hasta tocar — un tercio del límite que rompe el tren
(`VZ_POSADA` = 1.2 m/s). No regala nada: `tocar_suelo` sigue exigiendo llegar
lento, derecho y sin derrapar, y el `freno` de alineación sigue sin dejar bajar
hasta estar encima y quieto.

**La rampa no es cosmética.** La primera versión aplicaba el offset entero desde
cualquier altura y rompió la misión que ya funcionaba: bajando 3 m a plomo, el
descenso quedaba pegado al tope y el lazo no alcanzaba a frenar. Misión 1 pasó
de 1347.4 a **−46.6, SE ESTRELLÓ**. Con la rampa, arriba de 0.5 m manda el
proporcional de siempre y abajo el offset sostiene el toque.

**Medido, piloto clásico:**

```text
  peldaño 11 (posarse desde bajo)     1170.3  →  1274.8    (mejoró, sin tocarlo)
  peldaño 12 misión 3, limpio          106.6  →  1208.7    (ahora SÍ aterriza)
  peldaño 12 misión 1                 1347.4  →  1348.2    (sin regresión)
```

**Y el agujero estructural que lo dejó vivir meses.** Se midió el clásico contra
el umbral de los 14 peldaños — algo que nunca se había hecho:

```text
  peldaño  8  esquivar        umbral  900   clásico   −49.5   NO ALCANZA
  peldaño  9  esquivar        umbral  900   clásico   −49.5   NO ALCANZA
  peldaño 12  posarse         umbral  900   clásico    88.3   NO ALCANZA
  peldaño 13  ida y vuelta    umbral 1200   clásico   −49.5   NO ALCANZA
  peldaño 14  entrega         umbral 2400   clásico    −7.5   NO ALCANZA
```

Los de *esquivar* están bien así: ahí el clásico choca A PROPÓSITO (0/3) y el
genoma saca 3/3 — es el mejor resultado del proyecto y el umbral SÍ tiene que
estar por encima. Pero el **12 no tiene postes**: ni el modelo perfecto podía
aprobarlo. `persecucion.rs` tiene desde hace tiempo la prueba que caza esto
(`ningun_umbral_pide_mas_que_la_perfeccion`, regla 3.16) y **nunca se portó a
`mision.rs`**. Ahora sí: `ningun_peldano_sin_postes_pide_mas_de_lo_que_el_clasico_puede`,
que excluye los peldaños con postes por la razón de arriba.

**Segundo arreglo, del currículo.** Aun con el descenso arreglado, la misión 3
con ruido DURO sigue sin poder posarse: el clásico se queda flotando a 0.85 m
porque su velocidad estimada nunca baja del umbral que da permiso para
descender — persiguiendo una posición tan sucia se mueve de verdad 0.3-0.6 m
entre muestras, y posarse derrapando a ~1 m/s es un choque real
(`VH_POSADA` = 1.0). El freno está bien puesto; lo que estaba mal era exigirlo.
El peldaño 11 (posarse ya encima de la plataforma) SÍ aprueba con ruido duro
(1274.8): lo que no se puede es navegar Y posarse con precisión con los sensores
rotos a la vez — dos capacidades en un escalón, otra vez. `ruidos()` ya no pide
DURO en ese peldaño, con la misma lógica que el comentario de `PELDANO_DURO` ya
usaba. Resultado: peldaño 12 pasa de **88.3 a 1163.5** contra un umbral de 900.

---

### 7.15 La red estrellaba la posada que el modelo ya hacía perfecta

Con el descenso arreglado (7.14) el reentrenamiento llegó al peldaño 12 aprobando
los once anteriores limpio — y se volvió a trabar ahí, en 24.9 de 900, 260
generaciones. Pero el diagnóstico cambió de forma decisiva:

```text
  ANTES (7.14):  ⚠ DESAPRENDIÓ los peldaños [1,2,3,4,5,6,7,8,9,10,11]
  AHORA:         ⚠ DESAPRENDIÓ los peldaños [11]
```

El olvido catastrófico se desplomó de once peldaños a uno. Lo que quedó es la
hipótesis que la bitácora anotó hace meses y nunca se había podido aislar: los
peldaños 11 y 12 son **la misma maniobra en dos dificultades**, compartiendo
pesos y contextos de sensor casi idénticos.

**La medición que dio vuelta el diagnóstico.** El mismo campeón:

```text
  peldaño 11 (posada fácil)     1076.8   las 9 cumplen
  peldaño 12 (posada corrida)    −47.9   SE ESTRELLA
  peldaño 12 con la red en CERO 1163.5   las 9 cumplen   ← el clásico (regla 3.15)
```

**La red no fallaba en aprender a posarse: estaba arruinando una posada que el
modelo ya hacía perfecta.** Emitiendo ceros sacaba 1163.5; emitiendo lo que
aprendió, −47.9.

**La causa es el tamaño de la palanca, no el aprendizaje.** `CORRECCION_ANG` son
0.80 rad (46°) y `CORRECCION_ALT` son 2.5 m. En vuelo eso es margen para
corregir; a medio metro del piso, equivocarse no es puntuar peor, es volcar — y
2.5 m de autoridad en altura pisan por completo la rampa de aproximación, que
trabaja en décimas. El paisaje de aptitud alrededor del campeón deja de ser una
pendiente y pasa a ser un acantilado, así que la búsqueda no encuentra el camino
de vuelta al silencio.

**Dos arreglos que se evaluaron y se descartaron, con el motivo:**

- **Fusionar los peldaños 11 y 12**: casi un no-op. El examen de repaso ya mete
  las misiones del 11 en la aptitud como término `min` — es literalmente lo que
  ataba el número en 24.9. Fusionar no cambia el paisaje de búsqueda.
- **Sembrar el 12 desde el campeón del 11**: ya pasa. La población se arrastra
  entre peldaños, así que el campeón del 11 ES el punto de partida del 12.

**El arreglo: `autoridad_de_posada`.** La autoridad de la red se desvanece con la
altura mientras baja a posarse — plena arriba de 1.5 m, piso de 0.10 al tocar.
Mismo remedio que `limitar_sacudida` (7.5): acotar a la red donde estorba.

**No apaga la red, y es a propósito.** Al tocar le quedan ~0.08 rad (4.6°) y
0.25 m para refinar. Con piso cero el peldaño dejaría de medir a la red —se
aprobaría quitándola, no enseñándole— y eso sería maquillar. No mueve la vara
honesta: el clásico no tiene residuo, así que las tablas siguen comparables.

**Resultado, con el MISMO campeón y sin reentrenar:**

```text
  peldaño 12    −47.9  →   967.3    (las 9 cumplen, y 967.3 > 900: APRUEBA)
  peldaño 11   1076.8  →  1244.7    (mejoró de arrastre)
```

**Y en `capacidades`, también sin reentrenar, con el genoma instalado viejo:**

```text
  posarse entero        2/3  →  3/3     (iguala al clásico)
  entregar un paquete   1/3  →  2/3     (le GANA: clásico 1/3)
```

La tarea original del proyecto —entregar un paquete— pasa de 1/3 a 2/3 y por
primera vez supera a la vara honesta. Salió de acotar a la red, no de entrenarla.

---

### 7.16 El ojo: percepción desde píxeles, validada — y la cámara de la máquina no sirve

Propuesta del usuario: usar la webcam de la HP ENVY como fuente de prueba real
del kernel, calculando flujo óptico e inyectándolo "a los 96 valores de sensores
que el kernel espera". Ataca el tapón correcto (la percepción está al 0%: hoy
`RasgosVisuales` se calcula analíticamente del estado exacto del simulador).
Pero el plan tenía dos supuestos que no se sostienen, y conviene dejarlos
escritos porque son fáciles de repetir.

**1. Una cámara cubre 14 de 96 bits, no los 96.** El reparto real de
`sensores_guia`:

```text
  error al punto de guía          30 bits   ← GPS / estimador
  velocidad relativa al objetivo  15 bits   ← GPS / estimador
  cómo me estoy moviendo          18 bits   ← IMU
  actitud                         12 bits   ← IMU
  altura sobre el suelo            7 bits   ← barómetro / lidar
  VISIÓN                          14 bits   ← la cámara
```

El 85% restante sale del controlador de vuelo. Ningún puente de visión puede
alimentar "los 96 sensores".

**2. El flujo óptico ya no se consume.** `flujo_x`/`flujo_y` se sacaron del
código de sensores hace tiempo (ver "El reparto de los 96 bits" en `gym3d.rs`)
porque duplicaban peor lo que velocidad y altura daban con más resolución.
Calcularlo y mandarlo tal cual no conecta con nada. Sigue siendo el cálculo
INTERMEDIO correcto —la divergencia se deriva de él— pero no es una entrada.

Lo que la cámara sí puede dar son tres números: `obs_horizontal` (6 bits,
±1.4 rad), `obs_proximidad` (5 bits, 0..1) y `divergencia` (3 bits, ±4.0).

**Por qué la divergencia es la comparación limpia.** En el simulador
`divergencia = vz / altura`, que es la tasa de expansión del campo visual: el
inverso del tiempo hasta el impacto. Una cámara mide exactamente esa misma
cantidad física como la divergencia del campo de flujo. No se aproxima nada: es
el mismo número por dos caminos.

**La herramienta: `tools/ojo.py`.** Captura, flujo denso (Farnebäck) y los tres
campos, con los rangos del termómetro al lado para ver si la cámara los excita o
los deja muertos. Acepta `--video` además de cámara.

**Auditoría del instrumento antes de creerle (regla 3.18), y encontró DOS
errores de escala propios.** Se generó una secuencia sintética de acercamiento a
una pared con textura, con `v` y `d` conocidos, o sea con divergencia de verdad
de campo calculable a mano:

```text
  v = 0.5 m/s, d = 2.0 m → 1.0 m en 2 s
  divergencia esperada: 0.25 → 0.50 1/s,  media = 0.5·ln2 = 0.347
  divergencia MEDIDA:   0.194 → 0.572,    media = 0.339   (2.3% de error)
```

Los dos errores que la validación cazó, ninguno visible leyendo el código:

1. **Factor 2 en la divergencia.** La divergencia 2D (`∂u/∂x + ∂v/∂y`) es el
   DOBLE de la tasa de expansión, pero `sim3d` define `vz/altura` sin ese 2.
   Sin dividir, todo salía 2× y saturaba un campo de 3 bits que llega a ±4.0.
2. **`obs_proximidad` inflado 9×.** Se sumaban las 9 columnas en vez de
   promediarlas, y el campo saturaba al 100% del tiempo — los 5 bits habrían
   quedado siempre en el tope, o sea entrada muerta. Arreglado: 14.3% del rango.

**La percepción desde píxeles funciona y está validada contra verdad conocida.**
Es el primer pedazo de la cadena de hardware que existe y se puede defender con
un número.

> **⛔ CONCLUSIÓN CORREGIDA — se midió con la lente TAPADA.**
> Lo de abajo («la cámara no sirve») se sacó sin saber que el usuario tenía la
> webcam cubierta con cinta. Eso explica exactamente el negro absoluto de cam0,
> así que la medición no vale para concluir nada sobre el hardware. Enumerando
> los formatos después: **`/dev/video0` expone sólo `YUYV 4:2:2`, o sea que es
> una cámara de color UVC común** — no necesita librealsense ni ningún driver
> especial, y el puente de visión sólo necesita eso. `/dev/video2` expone
> `Z16` (profundidad de 16 bits), `INZI` e `Y10`: ése sí es el nodo de
> profundidad y ése sí necesitaría librealsense 1.x.
>
> Lo que sigue valiendo de abajo: la F200 es de 2014, librealsense 2.x le quitó
> el soporte, y la profundidad requiere la rama legacy `v1.12.1`. Lo que NO
> vale: dar por muerta la cámara de color. Falta re-medir sin la cinta.
>
> **Lección, que es la regla 3.18 otra vez:** antes de declarar muerto un
> sensor, verificar que no haya algo tapándolo. Un desvío de exactamente 0.00
> era la pista —un sensor real tiene ruido— y en vez de buscar la causa física
> se saltó a la conclusión de driver.

**Pero la cámara de la máquina NO sirve, y es un hallazgo de hardware.** No es
una webcam: es una **Intel RealSense F200** (USB 8086:0a66), la primera
generación de 2014, con cuatro nodos `/dev/video0-3`. Medido:

```text
  cam0: 640x480, media 0.0,  desvío 0.00   (negro absoluto)
  cam2: 640x480, media 90.0, desvío 0.00   (gris uniforme)
```

Desvío **exactamente 0.00** en 40 cuadros seguidos, en los dos. Un sensor real
siempre tiene ruido; varianza cero significa que no está capturando. Los
controles v4l2 sí responden (brillo 55, exposición 156, ganancia 64) y forzarlos
no cambia nada, así que el driver habla con el hardware pero el stream viene en
blanco. Ningún formato (MJPG, YUYV, Z16, Y8, INVZ) en ninguno de los cuatro
nodos da imagen.

La causa es conocida: la F200 necesita la secuencia de inicialización de
`librealsense 1.x`, que está archivada, sin mantenimiento, y no compila contra
un kernel moderno. `librealsense 2.x` **quitó el soporte para F200/SR300**.
Recuperarla es un proyecto propio con pocas chances.

**La ironía que vale anotar:** si funcionara sería MEJOR que una webcam, no peor
— es una cámara de profundidad, así que `obs_proximidad` saldría de profundidad
métrica real en vez de inferirse del flujo. Si algún día aparece una cámara de
profundidad que sí ande, vale retomarlo.

**Salida práctica:** cualquier webcam USB de 5-10 USD destraba esto en el acto, y
además es más representativa de lo que un dron va a llevar encima. Mientras
tanto `--video` permite validar y trabajar con material grabado.

---

### 7.17 El «3/3 esquivando» era el conjunto de entrenamiento

El usuario, mirando el visor: *"el dron vale verga para esquivar"*. La batería de
pruebas decía lo contrario —3/3 contra 0/3 del clásico, citado durante meses como
el mejor resultado del proyecto y como la única evidencia de que la red aporta
capacidad propia—. Ganó el usuario.

**La causa.** `capacidades` sección 6 construía los escenarios con
`crate::gym3d::obstaculos(i)`: **los mismos tres conjuntos de postes con los que
el genoma entrena**. La prueba no medía si esquiva, medía si recuerda.

Lo irónico es que la biblia ya advertía exactamente esto, en la misma función, a
cincuenta líneas de distancia — la prueba de entrega dice explícito *"rutas que
el genoma NO vio entrenando — entregar donde ya entrenó no mide nada"*. La de
postes hacía justo lo contrario y nadie lo miró.

**Medido, mismo genoma, mismas rutas, lo único que cambia es dónde están los
postes:**

```text
  6a. postes que YA VIO     genoma 3/3  ·  clásico 0/3
  6b. postes NUEVOS         genoma 1/3  ·  clásico 2/3

  ruta 1 (2 postes nuevos)  genoma: pasó a −0.01 m  ESTRELLADO  ·  clásico: SÍ
  ruta 2 (2 postes nuevos)  genoma: pasó a  0.30 m  cumplida    ·  clásico: SÍ
  ruta 3 (3 postes nuevos)  genoma: pasó a −0.01 m  ESTRELLADO  ·  clásico: NO
```

Los postes nuevos no son trampas: están en la trayectoria, son esquivables
rodeando, y el clásico —que supuestamente "no puede esquivar"— saca 2/3 en
ellos. El genoma se estrella en dos de tres (roce −0.01: los toca).

**El veredicto honesto, que hay que decir sin adornos: con obstáculos que no
memorizó, la red es PEOR que el modelo físico.** La afirmación más fuerte del
proyecto —"la red hace perfecto algo que la vara honesta no puede hacer"— era un
artefacto de evaluar sobre el conjunto de entrenamiento.

**Lo que esto NO invalida:** que el genoma le gane al clásico en las rutas de
entrega (2/3 contra 1/3) y que iguale en despegar, ir a B y posarse. Esos sí se
miden sobre rutas que el genoma no vio.

**Y la regla que sale, que es 3.18 apuntando a un lugar nuevo:** un informe de
capacidades que reutiliza los escenarios del gym no mide capacidad, mide
memoria — y lo va a hacer en silencio, informando números altísimos, que es el
peor modo de falla posible. Cada prueba de `capacidades` tiene que construir sus
escenarios APARTE del gym, y si comparte alguno hay que decirlo en la propia
salida. Ahora se corren los dos bloques (6a memoria, 6b generalización) y el
resumen marca cuál vale.

**Cómo se encontró, que también importa:** no lo cazó ninguna prueba automática
sino una persona mirando el dron volar y diciendo que no le creía al número. La
regla 3.3 («todo lo que se afirma tiene que ser verificable en el visor») existe
para esto, y funcionó — pero al revés de como estaba pensada: el visor no
confirmó la medición, la desmintió.

---

### 7.18 Gradiente en la selección, y el canal de elevación que faltaba

Dos cambios que atacan las dos cosas que el usuario marcó: que el entrenamiento
no avanzaba y que el dron no esquiva.

#### 1. Separar con qué se ORDENA de con qué se APRUEBA

El peldaño 13 estuvo **380 generaciones clavado en 39.2 de 1200**. La causa no
era el peldaño: era el `min`.

La aptitud es `min` sobre todos los términos, y un `min` **no tiene gradiente en
ningún término salvo el peor**. El mínimo lo definía el examen de repaso, que a
su vez es un `min` sobre doce peldaños olvidados: una mutación que recuperaba
once de los doce **no movía el número ni un decimal**, así que la selección no la
veía y se perdía. Recuperarse exigía arreglar todo a la vez, que es justo lo que
la mutación al azar no hace. Es el agujero del `min` que la regla 3.8 ya
documentaba sin cura.

La salida NO es aflojar la vara (regla 3.2, intacta) sino separar dos cosas que
estaban pegadas por accidente:

```text
  APROBAR   min >= umbral                    ← el peor caso entero, sin cambios
  ORDENAR   min + 0.15 × (media − min)       ← monótona en TODOS los términos
```

El peso es chico a propósito: el mínimo sigue mandando entre genomas con peor
caso distinto, y la media sólo decide entre los que empatan ahí — que es
exactamente el caso que antes se resolvía por sorteo.

Detalles que importan para que no se rompa después:

- `mision::terminos` expone los términos crudos cuyo mínimo ES la aptitud, y
  `aptitud`/`aptitud_y_orden` los agregan de dos formas **sin duplicar el
  cálculo** (regla 3.3).
- `terminos_de_repaso` comparte la fórmula de escalado con `examen_de_repaso`:
  si divergieran, el número que ordena y el que aprueba hablarían de exámenes
  distintos.
- El `mejor` que se informa y que aprueba es el mejor PEOR-CASO **de toda la
  población**, no el honesto del primero por orden: ordenar por la suave no
  puede inflar lo que se declara aprobado.
- Para persecución y punto las dos aptitudes son la misma: el cambio se hizo
  donde el problema está medido, no en todas partes por simetría.
- Prueba `ordenar_con_la_suave_no_afloja_lo_que_aprueba`: la honesta es el
  mínimo exacto y la de orden nunca queda por debajo.

#### 2. El canal de elevación, donde antes iba la divergencia

Con postes nuevos el genoma saca 1/3 (7.17). La biblia ya decía dónde estaba
parte del problema y llevaba meses sin hacerse:

> *no hay ningún bit de elevación del obstáculo. La física SÍ permite pasarle
> por arriba a un poste (`Obstaculo::distancia` da infinito si el dron va más
> alto), pero el dron no tiene cómo saber si el poste termina abajo suyo o sigue
> subiendo. Hay una solución válida —subir— que es invisible para el piloto. Esa
> asimetría entre lo que el mundo permite y lo que el dron percibe es del
> proyecto, no del genoma.*

El canje es a presupuesto CERO y está justificado por los dos lados:

- **Lo que sale no informaba nada nuevo.** `divergencia` es exactamente
  `vz / altura` (`sim3d::ver_base`), y las dos magnitudes ya viajan por separado
  con mucha mejor resolución (velocidad 18 bits, altura 7). Es palabra por
  palabra el motivo por el que ya se habían sacado `flujo_x` y `flujo_y`:
  quedaba como el último resto de esa familia.
- **Lo que entra hacía falta.** `obs_altura` = cuánto sobresale la punta del
  estorbo por encima del dron, ponderada por amenaza igual que el rumbo.
  Positivo = todavía tapa; negativo = se le puede pasar por arriba. ±8 m en
  3 bits.

El campo `divergencia` se conserva en `RasgosVisuales` aunque ya no entre a los
96 bits, porque `tools/ojo.py` lo usa como la comparación limpia contra una
cámara real (7.16) — ahí sí se mide desde píxeles y es la única forma de
contrastar percepción real contra simulada.

`los_sensores_llenan_exactamente_los_96_bits` sigue pasando: el presupuesto no
se movió.

**Esto invalida TODOS los genomas** (sección 4: cambiar el código de sensores
los invalida más que cualquier otra cosa). El genoma instalado quedó ilegible
para el código nuevo y `capacidades` sobre él no significa nada hasta reentrenar.
Se relanzó desde cero.

---

### 7.19 El ojo real alimentando al dron en vivo, y grabación reproducible

Primera vez en el proyecto que **el mundo físico toca el cerebro del dron**.
Hasta acá `RasgosVisuales` salía siempre de trigonometría sobre la posición
exacta de los postes — que en un aparato de verdad no se la regala nadie — y la
biblia lo lista como uno de los tapones para hardware.

**Qué hay ahora:**

- `tools/ojo.py --visor URL`: manda a `/ojo?h=..&p=..` lo que ve la cámara, y
  el visor reemplaza `obs_horizontal`/`obs_proximidad` de la visión simulada por
  las de píxeles reales. Verificado en vivo con el dron volando:
  `ojo manda: True · vivo: True · rumbo: −0.150 · prox: 0.003`, cambiando con lo
  que pasa delante de la lente.
- `tools/ojo.py --grabar PREFIJO`: guarda `PREFIJO.mkv` (video) y `PREFIJO.csv`
  (los tres campos por fotograma, con tiempo). Con `--video` se repite el
  experimento exacto.

**Se pisan sólo DOS de los tres campos, y eso es a propósito.** `obs_altura`
—el canal de elevación que entró en 7.18— NO se puede sacar de una cámara
monocular: necesita profundidad o saber el tamaño del obstáculo. Inventarlo
sería exactamente el instrumento mentiroso que la regla 3.18 persigue, así que
ese campo sigue viniendo del simulador y queda anotado como lo que falta.

**Dos seguros contra el modo de falla obvio.** Si el ojo se calla, la percepción
real se apaga sola a los 0.5 s (100 pasos) y vuelve la simulada; y al terminar,
el script manda `on=0`. Sin eso, un rumbo congelado dejaría al dron esquivando
un fantasma — peor que no tener percepción.

**La grabación no reproducía lo medido, y hubo que arreglarla.** Primera
versión, mp4 a 15 fps declarados:

```text
  captura en vivo   divergencia 14.0% del rango
  reproducción       divergencia  3.6% del rango     ← 4x de diferencia
```

Dos causas, las dos reales:

1. **mp4 comprime con pérdida** y borra justo el grano fino que el flujo óptico
   necesita para estimar movimiento. Cambiado a **FFV1 (sin pérdida)**.
2. **La cámara no entrega a cadencia fija.** Reproducir asumiendo los fps
   declarados escala mal la divergencia, que es una tasa *por segundo*, no por
   fotograma. Ahora el reproductor toma los tiempos reales del `.csv` hermano.

Con las dos cosas, captura y reproducción coinciden:

```text
  en vivo        divergencia 2.5%   obs_horizontal 32.0%   obs_proximidad 6.2%
  reproducción   divergencia 2.2%   obs_horizontal 27.8%   obs_proximidad 5.6%
```

**Una grabación que no reproduce lo que se midió no sirve para nada**, que es
para lo único que existe: repetir un experimento sin depender de mover la mano
igual dos veces.

**Lo que esto NO resuelve**: la cámara cubre 14 de 96 bits, el otro 85% sigue
saliendo del simulador (posición, velocidad, actitud, altura), y el medio campo
de visión real (~0.55 rad) es 2.5× más angosto que el que el simulador asume
(1.4 rad). Son datos de hardware, no defectos del puente.

---

### 7.21 El puente a hardware real: MSP, y el piloto de seguridad

Dos de los cuatro ítems de hardware que la biblia listaba en **0%**, hechos.
No dependían del entrenamiento ni de comprar nada: simplemente no se habían
empezado.

**`src/puente.rs`.** Convierte las cuatro consignas del cerebro en algo que un
controlador de vuelo comercial obedece.

**Por qué MSP y no MAVLink.** El controlador elegido es una placa de 15-20 USD
con Betaflight/iNav (7.1), y ésas hablan MSP. MAVLink es de ArduPilot/PX4, que
corren en placas más caras. Y dentro de MSP la forma correcta de que una
computadora pilote es `MSP_SET_RAW_RC`: se inyectan los canales de radio como
si vinieran del control remoto.

Eso último es deliberado y es lo que hace la industria:

- El controlador sigue haciendo estabilización, límites y failsafe. No se le
  pide que confíe en nosotros para nada crítico.
- **Si el puente se calla, Betaflight dispara su failsafe sin que nosotros
  tengamos que hacer nada bien.** Es seguridad que no depende de que nuestro
  código no tenga bugs, que es la única clase que sirve.
- El piloto humano recupera el mando con un interruptor físico.

**El piloto de seguridad**, aparte del mapeo a propósito — son dos preguntas
distintas ("cómo se dice esto en canales" y "¿corresponde decirlo?") y
mezclarlas termina con condiciones de seguridad escondidas adentro de una
conversión de unidades:

- **El software NUNCA se arma solo.** No hay ningún camino, ni siquiera "si
  todo está bien". El costo de un falso positivo es un aparato girando hélices
  sobre una mesa.
- **Si el cerebro se calla 0.2 s, desarma.** No repite la última orden: un dron
  volando con una consigna congelada se va derecho hasta que choca. Y no
  intenta aterrizar solo — el failsafe del controlador está mucho mejor probado
  que cualquier cosa que se escriba acá.
- **El tope de inclinación del puente es 0.35 rad (20°)**, no los 1.40 del
  simulador. El primer vuelo real no es el momento de averiguar si el mapeo de
  signos está bien a 80°. Se sube a mano, con el aparato atado y midiendo.
- **El acelerador en reposo va al MÍNIMO, no al centro.** Hay prueba dedicada:
  un acelerador centrado por defecto es un aparato que sube apenas se arma.

**Verificado en seco** (`dron puente --seco`), tramas contra el formato:

```text
  24 4D 3C 10 C8 DC 05 DC 05 E8 03 DC 05 E8 03 ... D8
  $  M  <  16 200 1500  1500  1000  1500  1000
```

`$M<` + largo + comando 200 + canales little-endian + checksum XOR. Es un
paquete MSP válido; Betaflight lo acepta.

**Lo que este módulo NO hace, y hay que decirlo:** no cierra el lazo de altura.
La consigna de altura del proyecto está en METROS y un controlador en modo
ángulo espera ACELERADOR. Quién cierra ese lazo —el barómetro del propio
controlador en modo ALTHOLD, o un lazo propio— es una decisión que depende de
la placa concreta y no se adivina acá.

**Y no está probado contra hardware**, porque no hay placa. Está probado contra
el formato y contra sus propias reglas de seguridad (5 pruebas). Eso es lo que
se puede afirmar hoy.

---

### 7.22 El simulador y la cámara metían cantidades distintas en los mismos bits

Lo destapó una pregunta del usuario: *"en la web hay un /cabina, ¿esa cámara no
sirve para entrenar al dron? no tengo dinero para una webcam"*.

La respuesta corta a la pregunta es no: la cámara del `/cabina` es Three.js
renderizando el mundo simulado desde el punto de vista del dron — gráficos de un
mundo falso, no una cámara. Pero buscando cómo capturarla apareció algo peor.

**El bug.** El anillo de proximidad recibía DOS cantidades físicas distintas
según quién lo llenara:

```text
  simulador      cercania = 1 − dist/ALCANCE        →  DISTANCIA
  tools/ojo.py   divergencia del flujo por sector   →  LOOMING (v/d)
```

No son lo mismo y la diferencia es brutal: un poste a 3 m da **siempre el mismo
valor** en distancia, y da **CERO** en looming si el dron no se está acercando.
O sea que un genoma entrenado contra el simulador leería mal a la cámara real —
todo el puente de percepción de 7.19, que se verificó "funcionando" mirando que
los números llegaran, estaba alimentando al cerebro con datos cuyo significado
no era el que había aprendido.

Es la regla 3.18 otra vez, en su forma más incómoda: el instrumento funcionaba,
los números fluían, y lo que transportaban era otra cosa.

**El arreglo: que el SIMULADOR hable el idioma de la cámara, no al revés.** Tres
razones, en orden de peso:

1. **Es lo que una cámara monocular puede dar sin ayuda.** Estimar distancia con
   una sola lente necesita saber el tamaño del objeto. El looming sale del
   flujo, gratis. Entrenar contra distancia es entrenar contra un sensor que el
   aparato no va a tener.
2. **Es invariante de escala.** Un poste grande lejos y uno chico cerca loomean
   igual, y para no chocar eso es exactamente lo que importa.
3. **Es lo que usa el bicho.** Los detectores de looming son el mecanismo de
   evasión de colisiones de los insectos — de donde salió media inspiración de
   este proyecto.

**El costo, dicho de frente:** quieto en el aire el looming es cero, así que el
dron NO VE un poste si no se mueve hacia él. No es un defecto del modelo: le
pasa igual a cualquier dron con una sola cámara, y es por eso que los aparatos
baratos le suman un sensor de distancia. Se acepta a propósito — mejor entrenar
contra la limitación real que contra una percepción que el hardware no tendrá.

**Tres pruebas que fijan el contrato**, para que no vuelva a divergir en
silencio:

- `el_anillo_mide_acercamiento_no_distancia`: mismo poste a la misma distancia,
  quieto da ~0 y acercándose se prende.
- `mas_rapido_o_mas_cerca_loomea_mas`.
- `cada_poste_prende_su_propio_sector`: dos postes laterales con el centro libre
  no pueden leerse como "centro tapado" — que era el modo de falla del resumen
  viejo.

`RANGO_LOOMING` (4.0) tiene que seguir siendo el mismo número que
`RANGO_DIVERGENCIA` de `tools/ojo.py`. Si divergen, vuelve el bug.

**Y la respuesta práctica al problema de plata:** no hace falta comprar nada. La
cámara de la laptop funciona desde que se le sacó la cinta (7.16). Una webcam
USB sería mejor —campo más ancho, montable mirando al frente— pero no bloquea
nada.

---

### 7.23 El tambaleo era un genoma leyendo basura, y nada lo impedía

Reporte del usuario mirando el visor: *"el dron vuela y entrega paquetes sí,
pero su vuelo es muy tambaleante de adelante para atrás o de lado a lado, ¿es
normal? ¿en la vida real no sería un problema?"*.

**Las tres respuestas, en orden.**

**¿Es normal?** No. Medido: `sacudida del mando: clásico 0.00892 · red 0.02632
· x2.95`. La red mueve la consigna de actitud casi tres veces más brusco que el
piloto clásico.

**¿Sería un problema real?** Sí: calor en los motores, estrés de ESC y
vibración. Y el proyecto ya lo había medido — cuando se acotó esa brusquedad
(`limitar_sacudida`, 7.5) el genoma **ganó** 104 puntos sin reentrenar. Temblar
no lo ayudaba, lo perjudicaba.

**Pero la causa concreta era peor que el temblor genérico.** El genoma instalado
se entrenó el 17-Sep 23:28. El código de sensores cambió TRES veces desde
entonces:

```text
  18-Sep 23:32   obs_altura reemplaza a divergencia
  19-Sep 05:48   anillo de proximidad reemplaza a rumbo+amenaza
  19-Sep         el anillo pasa a medir looming
```

O sea que estaba **leyendo basura** en los bits de visión. Y por la regla 3.15
(el modelo físico vuela, la red corrige encima, acotada) eso no se cae ni da
error: el clásico hace el trabajo y la red le suma ruido acotado a 100 Hz. Se ve
exactamente como lo describió el usuario — vuela, entrega, y tiembla.

**Lo grave: la sección 4 de la biblia ya avisaba que cambiar el código de
sensores invalida los genomas "más que cualquier otra cosa" — y no había NADA
que lo hiciera cumplir.** Cargar un genoma de otra época funcionaba sin una sola
queja. Una regla escrita que el código no verifica es una regla que se va a
violar, y acá se violó tres veces en dos días sin que nadie se enterara hasta
que una persona miró volar el dron.

**El arreglo:** `iafz::VERSION_SENSORES` (hoy 5, con historial de los cinco
cambios). Al guardar un genoma se escribe `<genoma>.sensores` con la versión;
`cargar_compatible` la verifica y devuelve error explicando qué pasa.

El visor no sólo avisa: **arranca en piloto CLÁSICO** cuando el genoma no es
compatible. Si sólo avisara por consola, el usuario seguiría viendo un dron
tembloroso y lo leería como "así vuela el proyecto" — que es precisamente el
malentendido que originó todo esto.

Verificado:

```text
  ⚠ GENOMA INCOMPATIBLE
    genomes/DRON3D_64.adn no tiene marca de versión de sensores...
  El visor vuela con el PILOTO CLÁSICO hasta que haya uno al día.
  piloto: CLASICO
```

**La lección general, que es la regla 3.18 apuntando a un lugar nuevo:** hasta
acá "auditar el instrumento" se había aplicado a lo que MIDE. Esto es lo que
EJECUTA: un genoma es un archivo de 11392 bytes crudos sin metadatos, así que
nada distingue uno entrenado ayer de uno de hace un año, y el modo de falla no
es un error sino un vuelo sutilmente peor. Cualquier artefacto binario que
cruce una frontera de versiones necesita llevar encima con qué se hizo.

---

### 7.24 «P» pisaba en silencio el arranque seguro en clásico

El arreglo de 7.23 (arrancar en clásico si el genoma es incompatible) tenía un
agujero: sólo actuaba al ARRANCAR el visor. La tecla `P` seguía siendo
`m.piloto_ia = !m.piloto_ia` a secas, así que un solo toque —sin querer, o
probando— volvía a poner la red ciega volando, sin ningún aviso en pantalla
(el de 7.23 iba a `eprintln!`, sólo visible en `journalctl`).

Pasó de verdad: el usuario reportó *"antes al menos si esquivaba un poste de
frente, ahora ni siquiera eso"* y leyó eso como un retroceso del anillo de
proximidad. No lo era — era el mismo genoma viejo de siempre, con `P` puesto,
leyendo un cuarto idioma de sensores distinto al que aprendió.

**Arreglo, dos partes:**

1. `Mundo.compatible: bool`, seteado por las TRES rutas de carga (arranque y
   las dos recargas automáticas — antes sólo la de arranque usaba
   `cargar_compatible`, las otras dos usaban `cargar` a secas y no
   actualizaban nada). `P` ahora es:
   ```rust
   'p' => if m.compatible { m.piloto_ia = !m.piloto_ia; }
          else { m.aviso = "genoma incompatible: reentrená..."; }
   ```
2. Banner FIJO en el visor (`#avisoIncompatible`), no el aviso de 4 segundos
   que ya existía. Mientras `compatible` sea `false`, queda ahí — porque un
   aviso que se apaga solo es exactamente lo que ya falló una vez.

Verificado en vivo: con el genoma viejo instalado, apretar `P` deja el piloto en
`CLASICO` y publica el aviso en `/estado`.

**La lección, van dos veces con esta misma forma:** una salvaguarda que sólo
corre en el arranque no es una salvaguarda, es un valor por defecto que
cualquier acción del usuario puede pisar sin darse cuenta. Si algo tiene que
seguir siendo cierto durante toda la sesión, tiene que revisarse en cada punto
donde el estado puede cambiar, no una vez al principio.

---

### 7.25 Freno de pared, y el olvido catastrófico confirmado en el peldaño final

**El freno.** El usuario armó una pared de postes en el visor y el dron voló
derecho contra ella — ni el guiado clásico ni el híbrido tenían un lugar que
supiera "esto está tapado en todos lados, hay que parar", el anillo de
proximidad era sólo una entrada más a la red. `frenar_ante_pared` (`gym3d.rs`)
escala la velocidad de crucero con el MÍNIMO del anillo (no el promedio):
un poste suelto con hueco no frena nada, sólo entra cuando los cinco sectores
están tapados a la vez. Primera calibración, LIBRE=0.35/PARED=0.75, estaba
desconectada del rango real del instrumento — una pared real a 5 m/s da
0.20-0.32 de looming, nunca 0.75 (regla 3.18). Recalibrado contra la medición
(LIBRE=0.03/PARED=0.18). 3 pruebas nuevas, 74/74 verdes.

**El olvido catastrófico, con evidencia en el peldaño MÁS ALTO, no sólo en
uno intermedio.** Una corrida honesta (`looming`, semilla 1, sensores con el
anillo/looming de 7.22, sin `saltar`) llegó de verdad al **peldaño 13/14 (ida
y vuelta)** sosteniendo los 12 anteriores sin olvidar nada — mejor que
cualquier corrida documentada hasta ahora. Se trabó ahí (590 generaciones
pidiendo 1200) y se cortó por costo de oportunidad. Una exploración con
`saltar` (decisión de una persona, logueada, regla 3.7) sembrada desde ese
checkpoint para sondear el peldaño final sin esperar la aprobación de 13
reprodujo el patrón exacto que la sección 7 ya señalaba como el bloqueo real,
esta vez en el peldaño 14/14 «entrega»:

```
gen 954  TRABADO en «peldaño 14/14: entrega» hace 700 generaciones
  └ ⚠ DESAPRENDIÓ los peldaños [5, 6, 7, 8, 9, 10, 11, 12, 13]
```

Chocó un poste, quedó a 3.61 m. El peldaño 14 (`entrega`, `mision.rs`) encadena
SEIS maniobras en un solo episodio de 45 s con memoria recurrente compartida
de punta a punta, más el examen de repaso de los 13 peldaños anteriores en la
misma evaluación — es, literalmente, "cuántas capacidades le estás pidiendo a
la vez" (regla 3.7) llevado al extremo, y coincide con el diagnóstico que
`etapas.rs`/`mision.rs` ya tenían: **el `min` del examen de repaso sólo
presiona el término que hoy es el peor**, así que romper 9 peldaños de atrás
no mueve el número hasta que ese peldaño roto quede por debajo del actual.

**Arreglo estructural, no otra corrida.** En vez de moler más generaciones
contra el mismo cuello (prohibido por 3.7 sin antes atacar la causa), se
activó la mejora que la bitácora 7.11 ya tenía diagnosticada y sin tocar:
la capa de estrategia del kernel comparaba sus 128 bits contra
`[l1_out, 0ULL]` — la palabra alta constante, así que los 64 bits altos de
cada neurona de estrategia (512 B de genoma) no codificaban más que un sesgo
redundante con `threshold`. Cambiado a `[l1_out, ctx_hi]` en
`csrc/iafz_64.c::forward_iafz_64` — el MISMO `ctx_hi` (memoria recurrente 32b
+ 32b de sensor) que ya calcula la primera capa, sin un popcount ni un byte
de más. Le da a la estrategia una conexión de salto directo al contexto en
vez de operar a ciegas sobre el resumen de la primera capa. Invalida todo
genoma viejo (regla 3.9): se mide reentrenando, no clampeando. 74/74 tests
verdes tras el cambio.

**Por qué esto y no mezcla de expertos todavía**, aunque 3.7/sección 7 lo
señale como candidato: 7.4 ya midió que especializar por régimen en
persecución NO pagó cuando el experto arranca de cero sobre un solo
recorrido — "el monolítico YA es el mejor experto de recta disponible,
porque se entrenó sobre el currículo entero". Repetir eso en entrega sin
sembrar desde un campeón ya entrenado y sin muestrear varios recorridos
sería repetir el mismo error. La media capa de estrategia es más barata,
no tiene ese riesgo conocido, y la sección 7 ya la marcaba como "lo que va
antes" — se prueba primero.

**Corriendo ahora**: reentrenamiento de cero (sin sembrar, sin saltar, 4000
generaciones, semilla=1, `salida=` a scratchpad — regla 3.11) con el kernel
nuevo. Sin resultado todavía — una corrida sola no decide nada (sección 4).
Falta: verlo llegar a 14/14 o trabarse, y si se traba, mirar si el patrón de
olvido cambió antes de concluir si la media capa ayudó o no.

---

### 7.26 Que generalice, y que siga aprendiendo después del gym

Tres cosas de una sesión, en orden de qué tan de raíz atacan el problema.

**1. Los postes dejan de ser tres mapas memorizados.** `gym3d::obstaculos`
devolvía TRES sets fijos de coordenadas, y el currículo entero se entrenaba
contra esos mismos postes miles de veces. Es la causa directa del resultado de
7.17 —3/3 esquivando los postes del entrenamiento, 1/3 con postes nuevos,
peor que el clásico— y no es un defecto del genoma: con el mismo escenario
repetido, la evolución no tiene ningún motivo para aprender la regla general.

Ahora `mision::postes_en_ruta` los genera **sobre el segmento
partida→objetivo**, con desvío lateral, radio y alto al azar, y
`mision::fijar_variante` los cambia **una vez por generación**: dentro de la
generación todos los individuos ven los mismos (si no, el campeón se elegiría
por suerte de sorteo) y entre generaciones cambian (memorizar deja de pagar).
Es la misma aleatorización de dominio que el proyecto ya hacía con el ruido de
sensores y que nunca se había aplicado a los obstáculos. El umbral de
aprobación no se movió: `LLEGADA × puntos + COMPLETA` no depende de dónde
estén los postes, así que la vara sigue pidiendo lo mismo.

**2. Adaptación en vuelo** (`vivo.rs`). El dron deja de ser un genoma
congelado: prueba variantes de sí mismo mientras vuela y se queda con las que
miden mejor. La estructura es búsqueda local **anclada**, no mutación libre:

```text
  ANCLA (lo que ya sabe) ──→ ASPIRANTE = ancla + empujón de 3 bits
        ↑                         │ vuela una ventana y se mide
        └── sólo lo reemplaza ────┘ ¿ganó por margen? sí → adopta; no → se tira
```

Seis salvaguardas, cada una con su prueba: el ancla sólo cae ante algo medido
mejor (verificado **byte a byte del genoma**), se re-mide el ancla en cada
ciclo (comparar contra un mérito viejo sería adoptar el viento de hace un
minuto), margen de promoción, empujón fino (`iafz::empujar`, 3 bits sueltos —
`mutate` voltea ~32 de 64 y eso no es refinar), nada de experimentos cerca del
suelo, y la vara honesta como piso.

**El mérito se calcula sólo con lo que hay A BORDO** —distancia estimada,
looming del anillo, sacudida del propio mando, golpes de la IMU—, nunca con la
verdad del simulador. Si mirara la verdad, no serviría para hardware, que es
el punto del proyecto.

**3. El repertorio** (`repertorio.rs`), que es lo que ataca el olvido de raíz.
Idea del usuario, y tiene nombre en la literatura: **MAP-Elites + Intelligent
Trial and Error** (Cully, Clune, Tarapore & Mouret, *Robots that can adapt like
animals*, Nature 521, 2015) — un robot guarda un mapa de comportamientos por
nicho y, cuando la situación cambia, **recupera el que sirve ahí** en vez de
re-aprender.

Nueve nichos: estorbo (libre / con algo / tapado) × tarea (crucero /
aproximación / sostén). Cada celda guarda su propio genoma. **Aprender a
rodear un poste sólo puede escribir en «crucero tapado»: no hay forma de que
pise lo que el dron sabe de posarse, porque eso vive en otra celda.** El
olvido catastrófico (regla 3.8, y medido otra vez en 7.25) deja de ser un
riesgo del mecanismo y pasa a ser imposible por construcción. Hay prueba:
`aprender_en_una_celda_no_toca_las_otras`.

Y corrige explícitamente el error que hundió la mezcla de expertos de 7.4:
**todas las celdas nacen siendo el campeón entero**, no pesos al azar. Aquella
sesión dejó anotado que había que "sembrar desde el monolítico" y nunca se
hizo. El peor caso del repertorio es, celda por celda, el genoma que ya se
tenía. Presupuesto: 9 × 11392 B = 100 KB, contra 4 MB de flash de un ESP32
(hay prueba que lo vigila). El repertorio se guarda a disco con cabecera de
versión, así que lo aprendido sobrevive a apagar el dron — y un repertorio de
otra versión de sensores **no se carga**, que es el fallo silencioso de 7.23
multiplicado por nueve.

**Dos errores propios que encontró la medición, no el razonamiento:**

- **Un `PISO` absoluto para apagar la red era un umbral elegido a ojo** —el
  error exacto que persigue la regla 3.18—. Se vio apenas se encendió en el
  visor: el dron sostenía posición perfectamente y el piso lo apagaba igual,
  porque en la fase de sostén el mérito es estructuralmente negativo (siempre
  queda error) mientras que en crucero es positivo. Un mismo número no puede
  servir para las dos cosas. Lo reemplaza algo que el proyecto ya tenía y sale
  gratis: una tercera fase que mide **el piloto clásico en ese mismo nicho**
  (red en cero = clásico exacto, regla 3.15). Así "la red estorba acá" es una
  comparación contra la vara honesta, no un número inventado — y en vez de
  apagar todo el sistema se apaga la red **sólo en el nicho donde estorba**,
  que es lo único de la mezcla de expertos de 7.4 que sí había pagado.
- **Un veredicto por ventana cambiaba de piloto por ruido.** El visor mostraba
  el mismo nicho alternando entre «la red AYUDA» y «la red ESTORBA» cada pocos
  segundos. Se arregló con votos (`VOTOS_PARA_CAMBIAR = 3`) en vez de
  promedios pesados, a propósito: un promedio necesita elegir una escala
  («¿cuánto es mucho?») y elegir escalas a ojo es el mismo error de recién.
  Contar ventanas no necesita escala, sólo el signo.
- Y una tercera, que cazó una prueba: al cambiar de tramo el objetivo salta de
  golpe y eso entraba al mérito **como si el dron hubiera volado 70 m en una
  centésima**. Acotado a lo físicamente posible (0.4 m por decisión a 100 Hz
  con techo de 31.8 m/s).

**Verificado en vivo en el visor** (regla 3.12), no supuesto: tecla `V`,
`/estado` publica nicho, fase y repertorio. Secuencia real observada — el dron
midió que en «sostén libre» su red estorbaba, probó variantes, adoptó una que
midió mejor y el nicho pasó a «la red AYUDA», sostenido. Distancia sostenida
al objetivo en esa corrida: media 0.54 m (mín 0.20).

**Lo que NO está medido todavía, y hay que decirlo:** que todo esto mejore
`capacidades` o destrabe el currículo. El repertorio se probó en vuelo libre
en el visor, no en la escalera de peldaños, y la aleatorización de obstáculos
no tiene todavía una corrida de entrenamiento completa que la respalde. 95
pruebas en verde dicen que el mecanismo hace lo que dice hacer; no dicen que
el dron vuele mejor. Eso se mide reentrenando y corriendo `capacidades`.

---

### 7.27 Todas las direcciones, y el veredicto del `ctx_hi` que caducó

**1. El currículo entrenaba tres rumbos, no el espacio.** Las rutas eran tres
vectores fijos: `+x` nivelado, `−x+y` subiendo, `+x−y` bajando. O sea que el
dron nunca practicó ir a `−y`, ni a `−x` puro, ni a la mayoría de las
diagonales. Es el mismo defecto que tenían los postes (7.17) pero en el eje de
la dirección: **un dron que sólo practicó tres rumbos no está entrenado para
volar, está entrenado para esos tres rumbos.**

`mision::rutas()` las genera ahora repartidas en azimut (120° entre sí, con el
trío entero rotando por generación con paso áureo para que no se repita en
ciclos cortos) y con desnivel propio: una sube, otra baja, otra queda
nivelada, y cuál es cuál también rota. Los peldaños cortos —frenar y rumbo—
heredan la dirección de la ruta en vez de tener sus propios vectores a mano
(`corta_hacia`, `girar_media_vuelta`). Prueba
`el_curriculo_cubre_todas_las_direcciones`: barre 64 variantes y exige los
**ocho rumbos del compás y las tres bandas verticales**; y
`ninguna_ruta_manda_al_dron_bajo_tierra`, porque cubrir direcciones no puede
significar pedir destinos imposibles.

Aclaración que importa para hardware: esto **no inventa movimientos**. Un quad
real se traslada inclinándose y puede combinar eso con subir o bajar, y la
física del simulador ya lo modela (`angulo_de_crucero`, arrastre cuadrático,
motores con retardo). Lo que se agregó son DESTINOS en todas las direcciones;
cómo llegar lo sigue decidiendo la física.

**2. El veredicto del `ctx_hi` caducó, y la regla 3.9 lo predijo.** En 7.25 se
revirtió la capa de estrategia con contexto porque "aprendía más rápido y
olvidaba más". Esa medición se hizo **con los postes fijos**, y en ese mundo
aprender rápido y memorizar rápido son indistinguibles. Al aleatorizar los
obstáculos las condiciones cambiaron, así que por la regla 3.9 la conclusión
quedó sin respaldo.

Re-medido, misma semilla (1), misma aleatorización, única diferencia el
kernel:

```text
                          postes FIJOS (7.25)      postes ALEATORIOS
  kernel [l1_out, 0ULL]   peldaño 13 en gen 576    peldaño 13, estancado en gen 50
  kernel [l1_out, ctx_hi] peldaño 13 en gen  51    peldaño 14/14 en gen 34
                          DESAPRENDIÓ los 12       sin desaprender nada (gen 40)
```

**Indicio fuerte a favor de lo contrario de lo que se concluyó en 7.25** — y la
lectura que lo explica es que con escenarios fijos la capacidad extra se
gastaba en memorizar (y por eso pisaba lo viejo), mientras que con escenarios
que cambian esa misma capacidad tiene que usarse en generalizar. Es UNA
semilla y las corridas van por la generación 40-50: **no es un veredicto**, se
anota como indicio y se vuelve a medir con el currículo completo.

Lo que esto deja como lección de método, que es lo más caro de las dos:
**cuando un experimento sale mal, la pregunta no es sólo "¿sirve el cambio?"
sino "¿bajo qué condiciones lo medí?".** El `ctx_hi` no era malo; estaba
midiéndose en un mundo donde memorizar pagaba.

---

### 7.28 El olvido catastrófico era, en buena parte, sobreajuste

**El bloqueo número uno del proyecto se movió, y no lo movió el cerebro: lo
movió dejar de entrenar contra escenarios fijos.**

Medido, semilla 1 en los tres casos, currículo de 14 peldaños:

```text
  escenarios                     llega a peldaño 13   llega a 14/14   olvida
  postes y rutas FIJOS               gen 576             nunca         los 12
  postes ALEATORIOS, rutas fijas     gen  ~30            gen 34        ninguno
  postes y rutas ALEATORIOS          gen  23-31          gen 34        ninguno
```

La primera fila es la corrida honesta de 7.25: 576 generaciones para llegar al
peldaño 13, trabada ahí, y al forzar el 14 con `saltar` **desaprendió los doce
de atrás** y se quedó así 190 generaciones. Las otras dos son de hoy.

**La lectura que lo explica:** con tres mapas de postes y tres rumbos
repetidos diez mil veces, lo que paga es memorizarlos — y una memoria de
coordenadas es conocimiento frágil, que la búsqueda pisa apenas el peldaño
siguiente necesita esos mismos pesos para otra cosa. Cuando el escenario
cambia en cada generación, memorizar deja de pagar y lo único que sobrevive a
la selección es la regla general; y una regla general no se pisa tan fácil,
porque le sirve a todos los peldaños a la vez.

Dicho de otro modo: **el examen de repaso (regla 3.8) estaba peleando contra
un problema que en buena medida creaba el propio currículo.** No es que el
agujero del `min` no exista —existe y está medido—, es que el olvido que se le
atribuía era en gran parte el desmoronamiento de una memoria sobreajustada.

**Y el corolario que corrige a 7.27:** el brazo con `ctx_hi` y el brazo de
control llegaron al peldaño 14 en **la misma generación, la 34**. O sea que el
"indicio fuerte a favor del `ctx_hi`" de 7.27 era, en realidad, el efecto de
la aleatorización de obstáculos que ese brazo también tenía. Con el A/B bien
hecho —misma semilla, mismo currículo aleatorizado, única diferencia el
kernel— **el kernel no explica la mejora**. Queda como estaba: revertido, sin
evidencia a favor ni en contra.

Es UNA semilla y los dos brazos todavía tienen que APROBAR el peldaño 14, no
sólo llegar. Pero el bloqueo que tenía frenado al proyecto desde varias
sesiones se movió, y se movió por el lado que nadie estaba mirando.

**La lección de método, que es lo caro:** durante sesiones enteras se buscó la
causa del olvido **adentro del cerebro** —más capacidad, pesos por régimen,
examen de repaso más ancho— y la causa principal estaba **afuera, en el
currículo**. La regla 3.7 manda preguntar "¿cuántas capacidades le estás
pidiendo a la vez?" antes de moler generaciones; faltaba la hermana de esa
pregunta: **"¿cuántas veces le estás pidiendo exactamente lo mismo?"**

**Apéndice de 7.28 — lo aprendido se vuelca solo.** Guardar el repertorio
únicamente al apagar la adaptación con la tecla dejaba todo colgando de que
alguien se acordara de apagarla bien, y un dron real se queda sin batería sin
avisar. Ahora se vuelca cada `ADOPCIONES_POR_GUARDADO = 3` adopciones:
escribir en cada una castigaría la flash del chip, y esperar más convierte un
corte de energía en horas de vuelo perdidas. Verificado en vivo: a la tercera
adopción apareció `genomes/DRON3D_REPERTORIO.bin` (102630 B) y al reiniciar el
visor el aviso dijo «repertorio de vuelos anteriores».

**Y una trampa de la sección 4 en la que caí yo mismo, para que conste:**
`cargo test --release` **no** reconstruye `target/release/dron`. Verifiqué el
guardado automático contra un visor que corría el binario viejo, no vi el
archivo aparecer, y estuve a punto de salir a buscar un bug que no existía. El
síntoma es exactamente el que la biblia describe: algo que debería haber
cambiado reporta el mismo resultado que antes del arreglo.

---

### 7.29 El peldaño partido: los 15 aprobados en la generación 100

Con la escalera partida (7.28 destrabó el olvido con obstáculos y rutas
aleatorias; esta entrada parte además el peldaño final en dos — ver el
comentario de `PELDANO_ENTREGA_SUAVE` en `mision.rs` para la medición
completa), se relanzó de cero: sin `saltar`, sin sembrar, semilla 1,
`salida=` a scratchpad.

```text
  gen  34  → peldaño 14/15 (entrega, limpio+real)   APROBADO
  gen 100  → peldaño 15/15 (entrega, +ruido DURO)   APROBADO
  DESAPRENDIÓ: cero apariciones en todo el log
```

Los 15 peldaños, incluida la entrega completa con el peor ruido, en 100
generaciones — contra las 576 que costaba sólo LLEGAR al peldaño 13 con el
currículo viejo, y contra el olvido sostenido (190+ generaciones) que
mostraban las corridas con escenarios fijos. Es la mejor corrida que tuvo el
proyecto.

**Lo que corrige a 7.25/7.27, con el A/B bien hecho esta vez.** La sospecha
de 7.27 ("el `ctx_hi` parece ayudar con obstáculos aleatorios") se midió mal:
comparaba una corrida con `ctx_hi` contra una corrida vieja con escenarios
FIJOS, dos variables a la vez. Repetido con el control correcto —misma
semilla, mismo currículo aleatorizado y partido, único cambio el kernel—
los dos brazos (con y sin `ctx_hi`) llegaron al peldaño final en la MISMA
generación (34). El kernel no explicaba nada; explicaba todo la
aleatorización. `ctx_hi` queda revertido (ya lo estaba) y esta vez con la
medición que realmente lo descarta.

**Disciplina que costó parar dos veces:** las dos corridas de fondo (kernel
viejo, kernel `ctx_hi`) se cortaron apenas dieron la señal que hacía falta
—clásico no aprueba el peldaño de entrega con ningún ruido, el campeón es
inconsistente entre niveles de ruido— en vez de dejarlas moler contra un
peldaño que ya se sabía mal planteado. Seguir esas corridas después de tener
el diagnóstico no habría sido rigor, habría sido obstinación (regla 3.7).

**Falta, antes de instalar nada:**

1. Que la corrida termine sus 4000 generaciones — aprobar una vez no es
   sostenerlo; el peldaño final sigue viendo escenarios nuevos cada
   generación y el número fluctúa después de aprobar (visto: -13 a -124 en
   las 70 generaciones siguientes a la aprobación, sin volver a acusar
   `DESAPRENDIÓ` — hay que confirmar que eso es ruido de medición y no una
   recaída lenta).
2. Repetir con otra semilla. Sección 4: una sola no decide nada.
3. `capacidades` contra el genoma que salga, y comparar la tabla nueva con la
   vieja (7.17) explícitamente — no basta con que el gym apruebe.

**La lección de la sesión, dicha una vez y no seis:** el proyecto pasó
varias sesiones buscando el arreglo del olvido catastrófico ADENTRO del
cerebro (más capacidad, pesos por régimen, examen de repaso más ancho). El
arreglo que de verdad pagó estaba AFUERA: en qué tan variado es lo que se le
pide aprender. La regla 3.7 ya preguntaba "¿cuántas capacidades a la vez?";
le faltaba la hermana — "¿cuántas veces exactamente lo mismo?".

---

### 7.30 Aceptar por UNA muestra: el mismo error en el gym y en el repertorio

Sesión de dos hallazgos que resultaron ser **el mismo error en dos lugares
distintos del proyecto**, y que sólo se vio porque se miraron los dos juntos.

#### 1. La corrida "mejor del proyecto" nunca había aprobado nada

La entrada 7.29 cerró con "los 15 peldaños aprobados en la generación 100,
sin un solo `DESAPRENDIÓ`", y dejó anotado como pendiente que faltaba ver si
se sostenía. Se sostuvo hasta la generación 110. El log completo de esa misma
corrida, leído hasta donde llegó:

```text
  gen 100  mejor  2817.4   peldaño 14/15: entrega
  gen 100  → peldaño 15 (APROBADO)
  gen 110  mejor   -96.6   peldaño 15/15: entrega
  gen 140  mejor    -0.2   (estancado)
  gen 379  TRABADO hace 200 generaciones (necesita 2400, va -115.0)
        └ ⚠ DESAPRENDIÓ los peldaños [9, 10, 11, 12, 13, 14]
```

De 2817.4 a −96.6 en diez generaciones, y nunca más por encima de 8.3 en las
280 siguientes. **La lectura fácil —"recayó"— es la equivocada.** Con los
escenarios re-sorteados cada generación (7.28), el 2817.4 de la generación
100 no midió una capacidad: midió un sorteo. El umbral se cruzó porque a esa
generación le tocaron misiones benignas. La aprobación nunca fue real, así
que no hubo recaída — hubo una foto.

Esto **refuta la parte optimista de 7.29** (no la aleatorización, que sigue
siendo el mejor cambio de esas sesiones: la escalera se sube de verdad y
rápido hasta el peldaño 14). Lo que se cae es el "15/15 en la generación
100" como logro: era un artefacto del criterio de aprobación.

**El arreglo (`gym3d.rs`): aprobar pide dos sorteos independientes.** Cuando
el campeón cruza el umbral, se lo vuelve a evaluar ahí mismo con otro juego
de escenarios (`fijar_variante` con otra semilla) y sólo se aprueba si
también pasa. Cuesta una evaluación de un genoma contra las 64 de la
población, y si no pasa lo dice: *"cruzó el umbral con X pero con escenarios
nuevos dio Y — fue suerte del sorteo, NO aprueba"*.

**Consecuencia obligatoria (regla 3.2):** esto SUBE la vara de aprobación.
Las generaciones-a-aprobar de 7.28 y 7.29 dejan de ser comparables con las
de acá en adelante. Quedan marcadas.

#### 2. Un bug de instrumento al lado: la foto guardaba a otro genoma

Encontrado mirando el mismo bucle. `ranking` se ordena por la aptitud
**suave**, pero el número que se informa y que aprueba es el mejor
**peor-caso** de la población — dos cantidades distintas, así que
`ranking[0]` puede ser un individuo diferente del que produjo ese número.
Todo lo que el entrenador archivaba o diagnosticaba usaba `ranking[0]`: la
foto del peldaño aprobado, el guardado cada 20 generaciones, el guardado
final, la telemetría de estancamiento y el dibujo del campeón. O sea que la
foto de un peldaño aprobado podía **no ser el genoma que lo aprobó**, y la
telemetría podía estar diagnosticando a otro. Regla 3.18, otra vez: el
instrumento archivando algo distinto de lo que acaba de medir. Unificado:
hay un solo `campeon`, el del mejor peor-caso, y lo usan los cinco.

#### 3. El repertorio, conectado al entrenamiento (regla 3.19)

`repertorio.rs` existía y sólo lo usaba el vuelo en vivo (tecla `V`). Ahora
`refinar_nicho` corre el MISMO lazo anclado de `vivo.rs` —empujón fino,
comparar contra el ancla, promover sólo si mide mejor— pero contra misiones
sintéticas del gym: miles de empujones por minuto en vez de uno cada varios
segundos. El nicho elegido es «crucero tapado», con una misión de **esquive
aislado** (`mision::mision_esquive_aislado`): una pierna, un poste, y el
poste a menudo CENTRADO —el caso que el usuario vio fallar en el visor y que
la aleatorización ancha casi nunca produce—.

Comando: `dron refinar [gens] [genoma=] [rep=] [salida=] [semilla=]`.

#### 4. Y ahí apareció el mismo error del punto 1, en chico

Primera tanda, un empujón evaluado contra UNA misión por generación, 1500
generaciones, cuatro semillas. Lote held-out de 24 postes nuevos, nunca
vistos en el entrenamiento:

```text
  semilla   elite anterior      tras refinar (1 misión/gen)
  1          18/24  ·  6 roces    22/24  ·  2 roces   (+171.8)
  2          17/24  ·  7 roces     6/24  · 17 roces   (−760.7)  ← se destruyó
  3          19/24  ·  5 roces    20/24  ·  4 roces   (+33.4)
  4          18/24  ·  6 roces    19/24  ·  5 roces   (+43.1)
```

La semilla 2 terminó muy por debajo de donde arrancó. La causa es la del
punto 1: con una sola misión por comparación, un aspirante gana por suerte
del sorteo —el poste le tocó de costado— y se lo asciende igual; repetido
1500 veces, la cadena de ascensos afortunados aleja al ancla de lo que sabe
hacer. Que el guardado final lo haya rechazado (mide contra held-out, no
contra el lote de entrenamiento) es la prueba de que esa parte del
instrumento sí estaba bien puesta.

**Arreglo idéntico al del gym: `LOTE = 4` misiones frescas por empujón.**
Segunda tanda, con LOTE=4 y **menos generaciones** (800, no 1500):

```text
  semilla   clásico        elite anterior     tras refinar (LOTE=4)
  1         5/24 · 19 r    18/24 ·  6 r        24/24 ·  0 roces   (+337.4)
  2         2/24 · 22 r    17/24 ·  7 r        24/24 ·  0 roces   (+401.4)
  3         2/24 · 22 r    19/24 ·  5 r        24/24 ·  0 roces   (+262.9)
  4         3/24 · 21 r    18/24 ·  6 r        19/24 ·  2 roces   (+61.1)
```

Cuatro semillas de cuatro mejoran, tres de ellas a **cero roces en 24 postes
nuevos**, con la mitad del cómputo de la tanda que se había destruido. El
clásico, sobre el mismo lote, choca en 19-22 de 24 — que era lo esperado (no
esquiva por diseño) y está acá sólo para que el número tenga vara.

Detalle de instrumento que valía la pena hacer bien: el veredicto NO se
escribe en `Celda::merito`. Ese campo guarda la escala del vuelo (ventanas de
cuatro segundos con sensores de a bordo) y esto puntúa con aptitud de misión
del gym. Mezclarlas haría que el lazo en vivo comparara peras con manzanas.
`reemplazar_elite` deja el mérito en `None` y la primera ventana de vuelo lo
vuelve a medir en su propia escala.

#### La lección, dicha una vez

**Aceptar un cambio por una sola muestra no es medir, es sortear.** El
proyecto ya tenía la regla escrita para las semillas de una corrida
("una sola no decide nada", sección 4) y no la había aplicado a los dos
lugares donde el propio código acepta o rechaza: el umbral de un peldaño y
la promoción de un aspirante. Los dos usaban una muestra. Los dos costaron
una conclusión falsa —una hacia arriba (15/15 que no existía) y otra hacia
abajo (una semilla destruida)—, y los dos se arreglan igual: pedir más de un
sorteo antes de creerle.

#### 5. Verificado en el visor, y un genoma indocumentado que estaba instalado

Regla 3.3: una capacidad que no se puede mirar no está terminada. Lo que
aprende una celda, sin embargo, **sólo se podía mirar con la adaptación en
vuelo encendida** — y ahí el visor pasa el primer tramo «midiendo la vara
(red en cero)», o sea mostrando el piloto clásico, que es exactamente el
malentendido que costó la regla 3.12. Se agregó `dron extraercelda` para
sacar la celda a un `.adn` suelto e instalarla como genoma principal.

Prueba en el visor, misma escena para todos: partida (0,0,3), objetivo
(6,0,3), **un poste centrado en x=3.5, y=0** (`/poste?x=3.5&y=0&r=0.6&h=6`).
La celda refinada lo rodea y llega:

```text
  t=1.4  x= 1.62  y=-1.48     ← se abre
  t=2.4  x= 4.29  y=-2.39     ← pasa el poste por fuera
  t=4.0  x= 6.14  y=-1.12     ← vuelve
  t=7.5  x= 6.28  y=-0.07     ← sostiene el objetivo, caído=False
```

**Y en el camino se cazó otra cosa.** El `genomes/DRON3D_64.adn` del árbol de
trabajo NO era el v5 que CLAUDE.md documenta: había un genoma sin documentar
(md5 `6f7d55a2…`, respaldo del v5 guardado al lado a las 15:38) que se
estrellaba contra ese poste **las cinco veces seguidas** que se probó.
Medido después en el lote común de 24 postes nuevos:

```text
                                          llega limpio   roza
  piloto clásico                             3/24         21
  genoma indocumentado (6f7d55a2, instalado) 4/24         20   ← ≈ el clásico
  v5 documentado (4c52dce5, HEAD)           16/24          8
  celda «crucero tapado» refinada           24/24          0
```

El indocumentado estaba al nivel del clásico en esquivar, o sea sin aportar
nada. Se restauró el v5 —que es lo que el contrato dice que está instalado— y
esta vez con la medición que lo justifica, no por prolijidad. El repertorio
instalado (`genomes/DRON3D_REPERTORIO.bin`) pasa a ser el refinado de la
semilla 2, con respaldo del anterior al lado.

**Lo que NO se hizo, a propósito:** la celda refinada no se instala como
genoma principal. Es una especialista de «crucero tapado»; que esquive
24/24 no dice nada sobre si todavía sabe posarse o entregar. Para eso está
el repertorio — cada celda en su nicho — y para eso falta el ítem de abajo.

#### 6. Deuda que queda abierta

1. **El refinamiento por nicho sólo se corrió en UNA celda de nueve.** Las
   otras ocho siguen siendo el genoma de arranque. Correrlas pide una misión
   sintética por nicho (hoy sólo existe la de esquive aislado).
2. **El repertorio refinado no pasó por `capacidades`.** Mide el genoma
   suelto, no el repertorio; hoy no hay forma de correr la batería con el
   repertorio puesto.
3. **Las dos corridas de 4000 generaciones con la aprobación arreglada
   quedaron corriendo** (semillas 1 y 7, `salida=` a scratchpad). El criterio
   nuevo ya cazó un caso en la generación 11 de la semilla 1: *"cruzó el
   umbral con 1049.1 pero con escenarios nuevos dio −45.8"*. Con el criterio
   viejo, ese peldaño quedaba aprobado.

---

### 7.31 El blindaje, y lo que apareció abajo cuando se sacó la trampa

Continuación directa de 7.30, y arranca con la crítica que la disparó, dicha
sin suavizar: **descubrir que el 15/15 era un espejismo no es un logro, es la
prueba de que el aparato de medir mintió durante una corrida entera y nada lo
delató.** No lo cazó la batería de pruebas. Lo cazó leer el log de una
corrida que se daba por terminada. Eso significa que el proceso de validación
venía rengo, no que tengamos buen ojo.

#### 1. La tercera encarnación del mismo error, encontrada auditando

7.30 arregló dos lugares que aceptaban con una sola muestra. Auditando el
resto apareció el tercero, y es el peor de todos: **`vivo.rs` ascendía un
aspirante por ganarle al ancla en UNA ventana de cuatro segundos**, y ése es
el lazo que corre EN VUELO, escribiendo en el genoma del dron real.

Lo irónico es que el propio módulo ya sabía que una ventana no alcanza:
`repertorio::VOTOS_PARA_CAMBIAR = 3` existe justamente porque con una
ventana el veredicto «la red ayuda / estorba» oscilaba por ruido de
medición. Ese cuidado estaba puesto en el veredicto —que no escribe nada— y
faltaba en el ascenso, que sí escribe.

Arreglado: `MUESTRAS_PARA_PROMOVER = 2` ventanas seguidas ganadas, una sola
derrota tira al aspirante. Dos y no tres porque cada ventana cuesta segundos
de vuelo real. **Lo que esto NO cubre, dicho explícito:** el ancla se mide
una vez por ciclo, así que las dos ventanas del aspirante se comparan contra
la MISMA medición del ancla. Achica el ruido de un lado, no del otro.

#### 2. Blindaje, para que no dependa de que alguien se acuerde

- **Regla 3.20** en el contrato, con la tabla de los tres casos y el costo de
  cada uno. Lo que exige: cada vez que se escribe un `if` que acepta,
  rechaza, aprueba o asciende algo medido, contar cuántas muestras tiene
  atrás. Una sola no es una medición, es un sorteo.
- **`ningun_ascenso_se_decide_con_una_sola_muestra`**: prueba que falla si
  alguien baja `LOTE` o `MUESTRAS_PARA_PROMOVER` a 1, con el número de lo que
  costó en el mensaje del `assert`.
- **`una_sola_ventana_ganada_no_asciende`**: la prueba de comportamiento que
  faltaba. La que ya existía (`una_variante_mejor_si_reemplaza_al_ancla`)
  pasaba igual con el bug puesto.
- **`dron sorteos [peldaño]`**: el instrumento que no existía. Evalúa el mismo
  genoma N veces cambiando sólo el sorteo de escenarios e imprime la
  dispersión, cuántas veces habría aprobado, y **cuál término ata la nota**
  (las misiones propias o el examen de repaso). Sin esto, "cruzó el umbral" y
  "sabe hacer la maniobra" se ven idénticos desde afuera.

#### 3. Y un instrumento más que estaba roto: las pruebas de `vivo` corrían pisándose

La prueba nueva pasaba sola y fallaba en la tanda completa. Causa: las ocho
pruebas de `vivo` escriben en los MISMOS slots del kernel (estado global del
proceso) y `cargo test` las corre en hilos. O sea que venían pisándose los
genomas entre sí y el resultado dependía del orden del planificador. Con el
bug de una-muestra puesto eso no se notaba; sacarlo lo destapó. Serializadas
con un candado. **Un instrumento cuyo resultado depende de la suerte del
planificador no mide nada** (regla 3.18).

#### 4. Lo que apareció debajo: el olvido sigue ahí, y ahora se ve

Las dos corridas nuevas (semillas 1 y 7, criterio honesto) **NO sostienen el
tipo**. A la generación 150 están las dos trabadas en el peldaño 10 de 15, y
el criterio nuevo viene rechazando aprobaciones falsas sin parar: **38 y 33
rechazos** de "suerte del sorteo" respectivamente, con números como
*"cruzó el umbral con 1123.7 pero con escenarios nuevos dio −47.2"*.

`dron sorteos` sobre el campeón vigente de la semilla 1, peldaño 10:

```text
  peor       -51.5      misiones propias (peor de los 9)   1159.4
  mediana    -48.2      examen de repaso (peor)             -47.2  ← ata la nota
  mejor      -45.8
  aprueba en 0 de 12 sorteos
```

**El peldaño que está peleando le sale bien —1159 sobre un umbral de 900, en
los nueve casos— y la nota la hunde un peldaño anterior que olvidó.** Eso es
el olvido catastrófico de la regla 3.8, sin disfraz. Con el criterio viejo,
un sorteo afortunado de los escenarios propios podía tapar esto y dar el
peldaño por aprobado; ahora no puede.

**Conclusión que hay que decir como es:** el arreglo del criterio no mejoró
al dron. Le sacó la careta. El proyecto no está "a una corrida de estar
listo": está frente al mismo bloqueo de siempre —el olvido— sólo que ahora
sin un mecanismo que lo esconda. La aleatorización de dominio (7.28) achicó
el olvido, no lo cerró.

#### 5. Deuda nueva, nombrada

1. **La aptitud misma es UN sorteo por generación.** `fijar_variante(semilla ^
   gen)` sortea los escenarios una vez y los nueve términos salen de ese
   sorteo. O sea que la selección —no ya la aprobación— persigue un blanco
   que se mueve cada generación. Es el mismo error de 3.20 un nivel más
   abajo, y es la causa candidata de que el campeón oscile entre 1123 y −47.
   **No se tocó a propósito: hay dos corridas en curso y cambiar el camino
   caliente con un A/B corriendo ya costó una medición entera** (sección 4).
2. **El repaso que ata la nota no dice CUÁL peldaño olvidó.** `sorteos`
   imprime el peor, no cuál es. Un renglón más de instrumento.
3. Sigue abierto todo lo de 7.30 §6.

---

### 7.32 La confirmación medía siempre el MISMO escenario, y tres peldaños nunca se aleatorizaron

Sesión que empieza corrigiendo a la sesión anterior —o sea, a la mía—. Dos de
los números de 7.31 estaban mal, y el error es del mismo tipo que el que esa
entrada denunciaba: **un instrumento roto**.

#### 1. `(v as f32)` con `v` grande: todas las variantes daban el mismo mapa

`rutas()` calculaba la rotación del trío de rutas como
`(v as f32) * 0.618… * τ`. Para `v` chico funciona. Para `v` grande el
producto vale ~1e10, y a esa magnitud **el paso de un `f32` es ~1024**: dos
variantes consecutivas dan el MISMO ángulo, y sumarle `i·τ/3` para separar las
tres rutas tampoco cambia nada — **las tres se apilan en un solo azimut**.

```text
  v = 0xA51E0000  →  ángulo 1.8159638
  v = 0xA51E0001  →  ángulo 1.8159638     idéntico
```

Dónde mordió, y es feo porque es código mío de ayer: la confirmación de
aprobación (7.30) usaba `semilla ^ gen ^ 0x5EED_C0DE`, un `u32` grande. O sea
que **la "confirmación con escenarios nuevos" era siempre el mismo escenario
degenerado**, y encima uno difícil. Los "46 rechazos de suerte del sorteo" de
7.31, con sus −47/−48 casi constantes, son en buena parte eso: no 46 sorteos
independientes que el genoma falló, sino **un escenario fijo fallado 46
veces**. La conclusión de 7.31 §4 —"las corridas no sostienen"— queda sin
respaldo hasta medirla de nuevo (regla 3.9).

Lo que NO cambia: el mecanismo de pedir una segunda muestra sigue siendo
correcto y necesario (3.20). Lo que estaba roto era de dónde salía la segunda
muestra.

**Cómo se cazó:** `dron sorteos` reportó **dispersión 0.0 exacta** entre doce
sorteos supuestamente distintos. Es literalmente la señal que la regla 3.18
manda perseguir —dos mediciones que dan el mismo número dígito por dígito son
la misma medición— y esta vez la cazó el instrumento, no la suerte.

Arreglado en los dos lados: `rutas()` saca la parte fraccionaria en `f64`
antes de pasar a `f32` (exacto para todo `u32`), y la confirmación usa una
variante del mismo rango chico que el entrenamiento.

#### 2. Y abajo, lo que de verdad traba: tres peldaños nunca se aleatorizaron

Con la variante arreglada, `sorteos` seguía diciendo **dispersión 0.0 en las
misiones propias** de los peldaños 10 y 11. No era el bug de arriba: esos
peldaños estaban **escritos a mano**.

```rust
9  => vec![despegue(0.0, 0.0, 3.0, 11), despegue(2.0, -1.0, 4.0, 29), …],
10 => vec![posada(0.0, 0.0, 1.5, 0.0, 0.0, 11), …],
11 => vec![posada(0.0, 0.0, 3.0, 0.0, 0.0, 11), …],
```

La aleatorización de dominio de 7.28 —el mejor cambio que tuvo el proyecto,
el que hizo que la escalera se subiera de verdad— se aplicó a las rutas y a
los postes **y se olvidó en despegar y en las dos posadas**. Son exactamente
los tres peldaños donde las dos corridas con el criterio honesto se quedaron
trabadas.

O sea que la causa del bloqueo no era ni el kernel, ni la capacidad de la
red, ni el examen de repaso: era que **tres de quince peldaños seguían siendo
tres mapas memorizables**, y memorizar coordenadas es conocimiento frágil que
la búsqueda pisa apenas otro peldaño necesita esos pesos (7.28). Aleatorizados
(`despegue_variado`, `posada_variada`), con los mismos rangos que cubrían las
misiones fijas, abiertos un poco. Verificado: la dispersión de las misiones
propias pasó de **0.0 a 78.2**.

#### 3. La deuda de "la aptitud es UN sorteo por generación", hecha

Queda implementada y **medible**, no supuesta: `mision::SORTEOS` + el
argumento `sorteos=N` de `entrenarmision`. Con N>1 cada individuo ve N juegos
de escenarios independientes por generación y la aptitud es el mínimo sobre
todos. El corrimiento es `thread_local` a propósito: la población se evalúa en
hilos y un atómico global le movería el escenario a los otros hilos en medio
de su medición — un bug que no se vería en ningún número.

Por qué no se puso en 2 y listo: cuesta lineal, y la única forma honesta de
decidirlo es un A/B (sección 4). **Corriendo ahora**, misma semilla, único
cambio el número de sorteos:

```text
  brazo A   semilla 1, sorteos=1     (el histórico)
  brazo B   semilla 1, sorteos=2     (cuesta x2)
```

#### 4. La corrida anterior se cerró a propósito, no se perdió

Las dos corridas de 7.31 se mataron para poder tocar el camino caliente. No
estaban "corruptas": midieron bien lo que se les preguntó —con el criterio de
aprobación honesto, la escalera se traba en el peldaño 10— y sus logs quedaron
guardados (`final.ANTES_multisorteo.log`). Lo que las volvió poco útiles es
que la respuesta que dieron estaba contaminada por el bug de §1. Cerrarlas fue
costo de oportunidad declarado, que es la única razón válida para cortar una
corrida (sección 4).

Eso quedó escrito como excepción a la regla de "no tocar el camino caliente
con un A/B corriendo", y de ahí salió la **regla 3.22**: si una regla traba el
trabajo, se abre la excepción y se escribe, no se frena.

#### 5. Lo que hay que re-medir por esto

- Todo 7.31 §4 (el diagnóstico de "no sostienen"): se midió con la
  confirmación rota.
- Los conteos de "suerte del sorteo" de 7.31: no eran sorteos independientes.
- Lo de 7.30 **NO** se cae: el `LOTE` del repertorio y su lote held-out usan
  `mision_esquive_aislado`, que no pasa por `rutas()`. Las tablas de 24
  postes nuevos siguen valiendo.

---

### 7.33 El currículo enchufado al repertorio: una celda por peldaño

**Lo que faltaba, dicho sin vueltas.** `repertorio.rs` existía, probado y
verificado en vuelo, desde 7.26. Resolvía por construcción el bloqueo número
uno del proyecto —el olvido catastrófico— y estaba enchufado a **dos** cosas:
la adaptación en vuelo (tecla `V`) y el refinamiento de una celda suelta
(`dron refinar`). **El gimnasio no lo usaba.** El currículo de 15 peldaños
seguía entrenando UN genoma que tenía que sostener las quince capacidades en
los mismos pesos, que es exactamente donde se pelean. Un hueco sin razón
escrita es un olvido, no una decisión (regla 3.19).

#### 1. Con quién se pelea, medido

El instrumento que faltaba era decir **cuál** peldaño se olvida, no sólo
cuánto (`mision::repaso_por_peldano`). Con eso puesto, los dos genomas —el
brazo que estaba entrenando y el v5 instalado— dijeron lo mismo:

```text
  brazo A, peldaño 10:                  v5 instalado, peldaño 10:
    peldaño  9    -47.8  ← lo perdió      peldaño  9   -45.9  ← lo perdió
    peldaño  8    -43.4  ← lo perdió      peldaño  8   -42.0  ← lo perdió
    peldaño  2    877.5                   peldaño  1  1038.6
    peldaño  1    974.0                   peldaño  2  1069.6
```

Los peldaños 8 y 9 son los dos de **esquivar**. Todo lo demás se recuerda
entre 877 y 1069. **No es olvido difuso: es esquivar peleando contra despegar
y posarse por los mismos pesos.**

#### 2. El censo: dónde gasta el tiempo cada peldaño

Antes de decidir nada había que contestar una pregunta con un número y no con
una intuición (regla 3.5): *cuando una misión atraviesa varias celdas, ¿cuáles
usa?* Eso es `dron celdas`, medido con la vara honesta (fracción de decisiones
por celda):

```text
  peldaño        crucero            aproximación         sostén
                 libre algo tapado  libre algo tapada    libre …
   1 frenar         ·    ·     ·     59%    ·     ·       41%
   3 ir a B        20%   ·     ·     48%    ·     ·       32%
   6 ir a B        50%   ·     ·     29%    ·     ·       21%
   8 esquivar      56%   5%   10%    15%    ·     2%      12%
   9 esquivar      54%   5%   11%    16%    ·     1%      12%
  10 despegar       7%   ·     ·     38%    ·     ·       55%
  11 posarse        ·    ·     ·     36%    ·     ·       64%
  12 posarse       12%   ·     ·     31%    ·     ·       57%
  15 entrega       20%   4%   13%    30%    ·     1%      33%
```

Y el solapamiento entre peldaños, que es la cota superior del daño que el
repertorio **no** puede evitar: esquivar (8-9) contra posarse (12) da
**0.38-0.40**, mientras que peldaños vecinos dan 0.9+.

#### 3. La celda que cada peldaño ESTRENA, no la que más visita

El máximo a secas no sirve y el censo lo muestra: «esquivar» pasa el **56%**
del tiempo en «crucero libre» y sólo el **10%** en «crucero tapado». Tomar el
máximo lo mandaría a la misma celda donde ya viven los seis peldaños de «ir a
B», y el repertorio no separaría nada. Lo que hace distinto a «esquivar» no es
dónde pasa más tiempo: es **dónde pasa tiempo que los peldaños anteriores no
pisaban**. Ese 10% es la maniobra que el peldaño viene a enseñar.

Regla: cada peldaño se queda con la celda que más crece respecto de TODO lo
anterior; si no crece nada (< 0.03), hereda la del peldaño previo — no estrena
nicho, es la misma capacidad más difícil. Medido, la asignación sale sola:

```text
  peldaños  1-2   frenar, rumbo                  → aproximación libre
  peldaños  3-7   ir a B, sensores duros         → crucero libre
  peldaños  8-9   esquivar                       → crucero tapado
  peldaños 10-15  despegar, posarse, entrega     → sostén libre
```

El umbral 0.03 no se eligió a gusto (regla 3.18): los peldaños que estrenan
celda lo hacen por goleada (+0.10 esquivar, +0.13 despegar, +0.20 ir a B) y
los que no estrenan nada quedan en +0.01. Entre 0.01 y 0.10 no hay nada.

#### 4. Cómo se reparte el crédito: no se reparte, se congela

Era la pregunta que bloqueaba la conexión. La respuesta es que **no hay que
repartir nada**: durante un peldaño, ocho de las nueve celdas vuelan
congeladas e idénticas para toda la población, y la novena es el individuo que
se está evaluando. Cualquier diferencia de aptitud es atribuible a esa celda y
a nada más — ascenso por coordenadas, no asignación de crédito.

En el kernel eso NO se puede hacer con nueve slots por individuo (48 × 9 = 432
contra 128 que hay, y compartir slots congelados entre hilos sería una carrera
sobre la memoria recurrente). Se hace con **un** slot por individuo y una copia
de 11 KB cuando la celda cambia — del orden de diez veces por misión, contra
2000 pasos de física.

#### 5. El chequeo de instrumento, antes de creerle a nada (regla 3.18)

Un repertorio recién sembrado tiene las nueve celdas idénticas, así que volarlo
tiene que dar **exactamente** lo mismo que volar el genoma suelto. Si no diera,
toda comparación repertorio-contra-monolítico estaría midiendo además ese
defecto. Y había un candidato concreto: `iafz::restaurar` pone la memoria
recurrente en cero, así que cruzar de celda la borra.

Se arregló copiando **sólo cuando el genoma es otro** (comparación de bytes), y
se verificó por los dos lados: una prueba
(`el_repertorio_sembrado_vuela_igual_que_su_genoma`) y el comando
`dron celdas genoma=X apt=N`, que compara la aptitud COMPLETA del peldaño
—misiones propias, tres ruidos y examen de repaso—:

```text
  aptitud en el peldaño 12:  genoma suelto -345.2   repertorio sembrado -345.2
```

#### 6. Un sesgo que habría invalidado el A/B entero

Al arrancar la comparación, el brazo monolítico marcó **-54.2** en la
generación 0 y el del repertorio **-300.1**, con el mismo genoma sembrado. No
era el repertorio: `sembrar_desde` sembraba 6 individuos de 48 y dejaba **42 al
azar**, mientras que el modo repertorio siembra los 48. El -54.2 era uno de los
aleatorios. O sea que los dos brazos no arrancaban del mismo lugar y encima el
monolítico arrancaba mejor por una razón que no tenía nada que ver con lo que
se comparaba.

Arreglado con `sembrar_todo` (y `sembrartodo` en la línea de comandos), que
siembra la población entera en los dos brazos. **De paso, el número suelto vale
anotarlo: en el peldaño 12 un genoma AL AZAR (-54.2) puntúa mejor que el genoma
que venía de aprobar el peldaño 11 (-345.2).** Ahí la red no corrige: estorba.

#### 7. El error se repitió DENTRO del arreglo que lo había corregido

Al medir el repertorio armado contra el monolítico hacía falta un genoma que
"supiera esquivar" para comparar contra algo real. El candidato obvio era el
que la corrida de 7.32 marcó `→ peldaño 9 (APROBADO)` en la generación 16 — el
mismo criterio de confirmación que 7.32 había instalado para blindar contra
esto mismo. Antes de usarlo, `dron sorteos 9 genoma=... n=12` (regla 3.20:
pedir más de un sorteo antes de creerle):

```text
  peor        -53.3
  mediana     -46.7
  aprueba en 1 de 12 sorteos independientes
```

**No sabía esquivar. Se había ganado una rifa.** Y el mecanismo que debía
impedirlo —la confirmación de 7.32/7.30— seguía pidiendo **UN** sorteo extra,
no varios: exactamente el error que esa misma confirmación existía para
corregir, cometido de nuevo un nivel más adentro, en el código que arregló la
primera vez. Con una tasa de acierto real del 8% (1/12) y un umbral que se
cruza casi todas las generaciones por el ruido de la selección, una corrida
larga encuentra su "aprobación" por sorteo en un rato — la corrida real lo
hizo en la generación 16.

**Arreglado en el mismo cambio**: `gym3d::CONFIRMACIONES` pasa de 1 a 3 sorteos
independientes antes de dar un peldaño por aprobado, y el diagnóstico de
"TRABADO" ahora imprime en cuántos de 6 sorteos independientes aprueba el
campeón — para no confundir "es difícil" con "no lo sabe hacer" (regla 3.16,
aplicada a la varianza y no sólo al techo). La prueba
`ningun_ascenso_se_decide_con_una_sola_muestra` (regla 3.20) ahora también
falla si `CONFIRMACIONES` baja de 2, con el número de este hallazgo escrito
adentro del `assert`.

**Por qué se anota separado y no se entierra en un párrafo**: la regla 3.20 ya
existía, con tabla y todo, y este es el CUARTO lugar donde el proyecto aceptó
algo por una sola muestra — el primero de los cuatro que ocurrió DESPUÉS de
que la regla se escribiera, adentro del propio mecanismo que la regla había
motivado. Eso es lo que dice la regla misma en su corolario: *"esto NO se da
por cerrado con los arreglos [anteriores]: cada vez que aparezca un resultado
bueno y limpio, la primera pregunta es cuántas muestras tiene esto atrás"*. La
pregunta se hizo tarde una vez más. Se agregó a la sección 4 de CLAUDE.md como
la cuarta fila de la tabla, para que la próxima vez que alguien escriba un
mecanismo de confirmación sea la PRIMERA pregunta, no la que se hace al querer
usar el resultado para otra cosa.

**Consecuencia sobre lo medido hasta acá (regla 3.2/3.4): todo lo que se
apoyó en "el peldaño 9 aprobó en la gen 16" queda sin respaldo.** Eso incluye
el genoma `.peldano9`/`.peldano11` usado para armar el repertorio de §5-6 de
esta misma entrada: no es que esquive de verdad, es que ganó una rifa contra
un umbral. El A/B relanzado en §8 corre con `CONFIRMACIONES = 3` desde el
arranque, así que sus "aprobado" van a valer lo que dicen valer.

#### 8. El A/B, relanzado con el criterio arreglado

Una sola variable, mismo genoma sembrado (el que aprobó el peldaño 11 en la
corrida de 7.32 — anterior al bug de §7, que sólo afecta la confirmación de
«esquivar»), misma semilla, mismo peldaño trabado:

```text
  brazo MONO   un genoma para los quince peldaños   (el histórico)
  brazo REP    una celda por peldaño, el resto congelado
```

La predicción falsable: con «crucero tapado» congelado, entrenar «posarse»
sólo puede tocar el 12% de la misión de esquive, así que el
`DESAPRENDIÓ los peldaños [8, 9]` tiene que achicarse o desaparecer. **Con el
criterio de confirmación arreglado, "aprobar" ahora cuesta más caro que en la
corrida vieja — las generaciones-a-aprobar de §7 no son comparables con las
de este A/B** (regla 3.2: cuando la vara se mueve, toda comparación anterior
queda inválida).

#### 9. Lo que se cerró de 7.32 §3: el A/B de `sorteos`

Se cerró a propósito (sección 4: costo de oportunidad declarado, con los
logs guardados). Per generación, `sorteos=2` quedó estrictamente atrás:

```text
                        peldaño 8   peldaño 9   peldaño 12   a la gen 360
  sorteos=1 (brazo A)     gen 9      gen 16      gen 142     trabado en 12
  sorteos=2 (brazo B)     gen 8      gen 142       —         trabado en 9
```

**No decide la pregunta de calidad**, y hay que decirlo: los dos brazos
terminaron trabados contra el MISMO muro —el examen de repaso de «esquivar» en
-45— así que el A/B no puede separar "aprueba más honestamente" de "avanza más
lento". Lo que sí quedó medido es el costo: al doble de precio, el brazo de dos
sorteos va 3 peldaños atrás. Una semilla, así que es indicio y no resultado
(sección 4).

---

### 7.34 El A/B del repertorio no se replica en la segunda semilla, y la vara honesta no miraba los postes

**Qué se corrió.** El A/B que 7.33 §8 dejó lanzado, terminado: un genoma
monolítico contra un repertorio de nueve celdas —una por tipo de momento—,
misma semilla (**1**, verificada reproduciendo la generación 0: `mejor 1106.9
mediana −361.7`), misma población sembrada entera desde `DRON3D_64.adn`,
1200 generaciones cada brazo, `sorteos=1`, `CONFIRMACIONES = 3`.

Los dos brazos corrieron con el MISMO binario: el único cambio de código entre
que arrancó uno y el otro fue el panel de progreso del visor (`progreso.rs`,
`web.rs`, `visor.html`), nada del camino caliente. `gym3d.rs` no se tocó desde
las 23:06 de la noche anterior y las dos corridas arrancaron después.

```text
                     llega a 8   llega a 9   llega a 10   final      tiempo
  MONO                 gen 6       gen 7        nunca     −41.5    11820 s
  REP (repertorio)     gen 24      gen 25       gen 933   1029.4     8442 s
```

**El tiempo de reloj no es un resultado y no se cita como tal**: las dos
corridas se solaparon sólo en parte sobre cuatro núcleos, así que la diferencia
2h20 / 3h17 mide el solapamiento tanto como el brazo.

#### 1. El titular «el repertorio aprendió a esquivar» está REFUTADO

El brazo del repertorio marcó `→ peldaño 10 (APROBADO)` en la generación 933,
con las tres confirmaciones independientes que 7.33 §7 había instalado justo
para blindar esto. Antes de creerle, la pregunta de la regla 3.20: *¿cuántas
muestras tiene esto atrás?* Con `dron sorteos 9 rep=… n=12` —instrumento nuevo
de esta sesión, abajo—:

```text
  peldaño 9, umbral 900, 12 juegos de escenarios independientes

  piloto                          peor   mediana     mejor   aprueba
  clásico (la vara honesta)      −49.4     −45.9    1131.4    1 de 12
  MONO, 1200 generaciones        −51.2     −45.4     −37.2    0 de 12
  REP (el repertorio guardado)  −282.4     −47.9    1032.0    3 de 12
```

**3 de 12 no es una capacidad, es una rifa** — la misma de 7.33 §7, un nivel
más adentro y con el blindaje puesto. Y los sorteos que pasa cada piloto son
**disjuntos**:

```text
  clásico   1· 2· 3· 4✓ 5· 6· 7· 8· 9· 10· 11· 12·
  REP       1· 2· 3· 4· 5✓ 6· 7· 8✓ 9· 10· 11✓ 12·
  MONO      1· 2· 3· 4· 5· 6· 7· 8· 9· 10· 11· 12·
```

No hay orden entre los pilotos: hay un sorteo que cada uno gana por su lado, y
ocho de doce que no pasa nadie. Con eso, "el repertorio aprobó y el monolítico
no" mide el sorteo, no el mecanismo.

#### 2. Por qué tres confirmaciones no alcanzaban: la cuenta se hizo por INTENTO

El comentario de `gym3d::CONFIRMACIONES` decía «con tres, la misma tasa da
0.05%», y es cierto **por cruce del umbral**. Lo que faltaba era multiplicar
por cuántos cruces tiene una corrida. Contado del log del brazo del
repertorio:

```text
  cruces del umbral                        425
  murieron en la confirmación 1            357   → sobrevive el 16.0%
  murieron en la confirmación 2             57   → sobrevive el 16.2%
  murieron en la confirmación 3             10   → sobrevive el  9.1%
  aprobaron (3 de 3)                         1
```

Las tres tasas de supervivencia dan lo mismo (~0.16), que es lo que se espera
de sorteos independientes con p ≈ 0.16. Con esa p, tres confirmaciones dejan
pasar `0.16³ × 425 ≈ 1.7` falsos aprobados por corrida. **El A/B tuvo
exactamente 1.** El modelo y el log coinciden, así que la cuenta sirve para
elegir el número: seis dejan `0.16⁶ × 425 = 0.007`.

**Y seis sale casi gratis**, medido en el mismo log: la comprobación se corta
en la primera confirmación que falla, así que las confirmaciones 4, 5 y 6 las
paga sólo lo que ya pasó tres — **1 cruce de 425 en el brazo del repertorio y
0 de 230 en el monolítico**. Del orden de tres evaluaciones extra por corrida
de 1200 generaciones, contra el costo medido de equivocarse, que es una
corrida entera.

`CONFIRMACIONES` pasa de 3 a 6, con la cuenta escrita al lado y un segundo
`assert` en `ningun_ascenso_se_decide_con_una_sola_muestra` que falla si baja.

**La lección, que es un nivel más profundo que la regla 3.20 y por eso se
escribe aparte: cuántas muestras necesita un guardián depende de cuántas veces
se lo consulta.** Uno que se consulta una vez necesita pocas; uno que se
consulta 425 veces por corrida necesita que `p^k × consultas ≪ 1`. Las cuatro
filas de la tabla de la regla 3.20 preguntaban "¿es más de una muestra?" y
ésta es la quinta, que pregunta otra cosa: "¿más de una alcanza para cuántas
veces la vas a usar?".

#### 3. El muro real: la vara honesta se cae de un peldaño al otro

Calibrar el umbral contra la vara honesta es obligatorio y nunca se había
hecho para estos peldaños (regla 3.18). Apagar la red la da gratis (regla
3.15), y el resultado es el hallazgo más importante de la sesión:

```text
  peldaño                              clásico aprueba
   7  sensores duros, SIN postes           11 de 12
   8  esquivar UN poste                     2 de 12
   9  esquivar todos los que haya           1 de 12
```

**El acantilado está exactamente en el primer poste.** De 11/12 a 2/12 en un
escalón. El clásico no esquiva —eso es por diseño y está escrito en el
contrato— así que de `PELDANO 8` para arriba la vara honesta deja de aprobar
sus propios peldaños, y como el examen de repaso del peldaño 9 incluye al 8,
**el término de repaso de todo peldaño ≥ 9 queda atado a algo que la vara
misma no pasa**: el clásico recuerda el peldaño 8 en −42.4.

Consecuencia sobre el A/B, dicha sin suavizar: **los dos brazos molieron más
de 1100 generaciones contra un peldaño que nunca se calibró, y el log no lo
decía.** Eso es exactamente la primera de las tres preguntas que la regla 3.7
exige contestar antes de moler generaciones («¿el umbral es alcanzable?»), no
contestada para este peldaño en ninguna sesión anterior.

**Lo que esto NO autoriza es bajar el umbral** (regla 3.2). El umbral es
`LLEGADA × puntos + COMPLETA`, o sea "cumplir la misión", que es el objetivo
correcto; y el peldaño 8 es justo donde la red tiene que superar al modelo
físico, así que calibrarlo contra un clásico que no esquiva lo volvería
trivial y mataría el sentido del peldaño. El camino que queda abierto, con la
evidencia que lo apunta, está en el ítem de deuda al final de esta entrada.

#### 4. Lo que SÍ queda medido a favor del repertorio: el olvido

Éste es el resultado que sobrevive, y es grande. Mismo peldaño 9, mismo sorteo,
examen de repaso peldaño por peldaño:

```text
  peldaño de atrás     MONO      REP
        8             −41.7    −38.9   ← comparte celda con el 9, por diseño
        2             −33.5   1051.9
        1              21.1    959.5
        3              19.9   1052.2
```

El monolítico destruyó todo lo de atrás; el repertorio conserva el 1, 2 y 3
arriba de 950 y pierde **sólo** el peldaño 8 — que es el único que comparte
celda («crucero tapado») con el peldaño que está entrenando. Los logs de las
dos corridas dicen lo mismo sin que nadie los provoque:

```text
                            diagnósticos   con olvido   qué olvidó
  MONO, peldaño 9                22            15       9 veces «los ocho»
  REP,  peldaño 9                 7             7       6 veces «sólo el 8»
  REP,  peldaño 10                3             3       3 veces «los nueve»
```

#### 5. Y el mecanismo, que explica las tres filas de arriba y predice la próxima

La fila del peldaño 10 parece contradecir al repertorio y no lo hace: lo
explica. El censo de celdas de 7.33 §2 dice en qué celda gasta el tiempo cada
peldaño, y con eso las tres filas salen de una sola regla:

```text
  entrenando          los peldaños 1-7 pasan ahí      olvido observado
  «crucero tapado»          0 a 11 % del vuelo        sólo el peldaño 8
  «sostén libre»           21 a 64 % del vuelo        los nueve
```

**El repertorio no elimina el olvido: lo achica en proporción exacta a la
fracción de la misión que NO pasa por la celda que se está entrenando.** La
promesa "aprender en un nicho no puede pisar otro" es cierta por celda y
engañosa por misión, porque una misión atraviesa varias celdas. Es lo que la
tabla de solapamiento de 7.33 §2 ya anticipaba (esquivar contra posarse da
0.38-0.40, no 0.00) y ahora está medido en una corrida larga, no deducido.

Y deja una predicción falsable para la próxima corrida: el repertorio va a
proteger bien mientras entrene «crucero tapado» y va a dejar de proteger en
cuanto entrene «sostén libre». Si eso se quiere arreglar, el camino no es más
celdas: es que los peldaños de sostén no compartan celda con el tramo de
sostén de los peldaños de abajo, que es un problema de censo, no de red.

#### 6. Instrumentos nuevos (regla 3.18: el aparato de medir también se audita)

- **`dron sorteos … rep=RUTA`** mide un REPERTORIO, no un genoma suelto.
  Hacía falta: desde 7.33 el brazo del repertorio aprueba volando el banco
  entero —su celda más las ocho congeladas— y preguntarle a la celda suelta
  "¿en cuántos sorteos aprobás?" contesta sobre un piloto que nunca voló.
- **`dron sorteos … clasico`** mide la vara en la misma escala y con los
  mismos sorteos. Sin esto no se puede separar "el genoma no lo sabe hacer" de
  "este peldaño no lo aprueba nadie" — que es justo la confusión que duró
  todas las sesiones de esquive.
- **`sorteo a sorteo: 1· 2· 3✓ …`** imprime el resultado de cada sorteo y no
  sólo el resumen. Es lo que mostró que los pilotos pasan sorteos disjuntos; un
  "3 de 12" contra un "1 de 12" sin el detalle se lee como una escala.

#### 7. Lo que se encontró de paso y queda anotado

**El piso del repaso que el contrato promete no existe en la tarea activa.** La
regla 3.8 dice «el piso del repaso es `max(umbral(p) × MARGEN_REPASO,
clásico(p))`, con prueba (`el_piso_del_repaso_nunca_baja_del_clasico`)». Eso es
cierto en `persecucion.rs` y **no está en `mision.rs`**, que es la tarea
activa: `examen_de_repaso` compara fracción del umbral propio, sin piso contra
el clásico. No se cambia en esta entrada —tocar la aptitud con un A/B corriendo
es lo que la sección 4 prohíbe— pero la regla 3.8 se corrige para decir dónde
vale, porque hoy se lee como si valiera en las dos.

#### 8. Segunda semilla: lanzada, y qué puede y qué no puede contestar

Una sola semilla no decide nada (sección 4), así que el A/B se relanzó con
**semilla 2**, los dos brazos a la vez y en paralelo esta vez para que el
reloj también sea comparable. Lo que ya se ve a las ~120 generaciones
confirma el diagnóstico del §3: **los dos brazos se trabaron en el peldaño 8**
—el PRIMER poste, no el segundo— y el propio diagnóstico del entrenador lo
dice en el renglón que 7.33 agregó:

```text
  MONO  gen  57  el campeón aprueba en 1 de 6 sorteos independientes
  REP   gen 118  el campeón aprueba en 3 de 6 sorteos independientes
```

Lo que esta semilla puede contestar es la pregunta del §4 (el olvido) sobre una
segunda muestra. **Lo que NO puede contestar es cuál brazo aprende a esquivar
mejor**, porque el peldaño es una rifa para todos, la vara incluida. El
resultado se anota abajo de esta entrada cuando termine.

#### 9. Deuda que sale de acá, en orden

1. **Calibrar el peldaño 8 antes de gastarle otra generación** (regla 3.7,
   pregunta 1). La pista que la evidencia apunta, y que la regla 3.17 manda
   revisar primero adentro del repo: `Guiado::consignas` —la vara de la
   misión— no mira los obstáculos, mientras que
   `Guiado::consignas_persiguiendo` **sí** suma las amenazas de todos los
   obstáculos visibles (campos de potencial, la técnica de la industria, ya
   escrita y probada en este mismo archivo). La regla 3.2 dice que la vara es
   «lo mejor que el modelo físico sabe hacer» y que arreglar un DEFECTO de la
   vara es obligatorio aunque el genoma quede peor en la comparación. Una vara
   que ignora un poste que el sensor ve no es lo mejor que el modelo sabe
   hacer. **Si eso se toca, toda comparación anterior de esquive queda
   inválida y hay que re-medirla en el mismo cambio** (regla 3.2).
2. **Re-hacer el A/B del repertorio contra un peldaño calibrado.** El de esta
   entrada no puede contestar la pregunta de calidad, y decirlo es más útil
   que reportar el 3-contra-0 como si la contestara.
3. **Los peldaños de sostén comparten celda con el tramo de sostén de los
   peldaños de abajo** (§5). Es lo que hace que el repertorio deje de proteger
   en el peldaño 10. Problema de censo.

#### 10. La segunda semilla: el titular no se replica, y la vara estaba ciega

**Semilla 2, los dos brazos en paralelo desde el mismo genoma sembrado.** Lo
que la semilla 1 decía —«el repertorio pasa esquivar y el monolítico no»— no
se repite:

```text
                    peldaño 8 (un poste)   peldaño 9 (todos)
  semilla 1  MONO         gen 6                 NUNCA (1200 gen)
  semilla 1  REP          gen 24                gen 933
  semilla 2  MONO         gen 6                 gen 283
  semilla 2  REP          gen 14                gen 219
```

**En la semilla 2 los dos brazos pasan los dos peldaños de esquive**, y la
diferencia (283 contra 219) es chica al lado de la varianza de un peldaño que
se aprueba en 3 de 12 sorteos. El titular de la semilla 1 era la semilla,
no el mecanismo — exactamente lo que la sección 4 dice que pasa cuando se
concluye de una corrida sola.

Y el examen de repaso, que era el hallazgo que había sobrevivido, tampoco se
replica tal cual. Peldaño 9, semilla 2 (MONO en la generación 320, REP en la
550):

```text
  peldaño de atrás     MONO      REP
        8             −46.7    −47.1
        2            1006.8    921.1
        1            1095.9    917.2
        3            1246.5   1183.1
```

**Acá el monolítico no olvida nada** —recuerda todo arriba de 1000— y el
repertorio queda apenas por debajo. **Con una salvedad que hay que decir
porque cambia la lectura: la comparación no es a igual exposición.** MONO
llegó al peldaño 9 en la generación 283 y llevaba ~37 generaciones ahí; REP
llegó en la 219 y llevaba ~330. El olvido se acumula con las generaciones que
se pasan peleando un peldaño, así que MONO todavía no tuvo tiempo de olvidar.
El resultado honesto de esta tabla es **inconcluso**, no «refutado», y para
cerrarlo hace falta medirla con las dos exposiciones parejas.

Resumen de lo que queda en pie del A/B entero, con dos semillas:

- **«El repertorio aprende a esquivar y el monolítico no»: REFUTADO.**
- **«El repertorio protege lo de atrás»: sostenido en la semilla 1, no
  replicado en la 2, y la medición de la 2 está confundida por exposición
  desigual.** Una semilla a favor y una inconclusa no es un resultado.

#### 11. Por qué el peldaño 8 era imposible: la vara honesta no miraba los postes

Esto es lo que sostiene todo lo anterior y lo que había que encontrar.
`Guiado::consignas` —la vara honesta de la misión— **no mira los obstáculos.
Ni uno.** Vuela al punto, y si hay un poste en el medio lo choca. Estuvo
escrito en el contrato como si fuera una propiedad del diseño («el clásico
nunca esquiva, por diseño — no es bug de la vara»).

La regla 3.2 define la vara como «lo mejor que el modelo físico sabe hacer» y
manda arreglar un DEFECTO de la vara aunque el genoma quede peor en la
comparación. Una vara que ignora un poste **que el sensor ya le está
mostrando** no es lo mejor que el modelo sabe hacer: es un lazo al que nadie
le conectó una entrada que ya existía (el anillo de proximidad de cinco
sectores vive en `sim3d::Camara::ver_con` desde que se arregló el resumen
promediado). Y la regla 3.15 dice lo mismo desde el otro lado: desviarse de lo
que se te viene encima se escribe en unas líneas, así que se escribe.

**Cómo esquiva** (`Guiado::consignas_esquivando`): sigue el hueco
(*follow-the-gap*, lo que usa el rubro), eligiendo el sector de menor
`amenaza + desvío` sobre el anillo. **No promedia**, y ése es el punto: dos
postes a los costados promedian a «rumbo cero», o sea que un piloto que
promedia lee «el hueco está al centro» justo cuando el centro es lo único
bloqueado — es el mismo modo de falla medido que hizo nacer el anillo.

**El compromiso con la maniobra, que la traza pidió.** La primera versión se
daba vuelta a mitad del rodeo:

```text
  t=1.00  x −1.43   se desvió hacia −x, bien
  t=1.25  x −1.69   máximo del desvío
  t=1.30  x −1.68   ← se da vuelta
  t=1.75  x −1.10   vuelve al poste y le pasa a −0.00 m
```

El sector del poste y el rumbo al objetivo se mueven los dos mientras el dron
se corre, así que el lado conveniente cambia de signo y el dron deshace lo
hecho. Cualquier seguidor de huecos del rubro lleva histéresis; acá no necesita
memoria, porque **la maniobra empezada ya está escrita en la velocidad lateral
del propio dron**. Barrido del peso, 24 sorteos:

```text
  COMPROMISO   peldaño 8   peldaño 9
     0.00       11 / 24      4 / 24
     0.15       11 / 24      6 / 24
     0.30       12 / 24      6 / 24
     0.60       12 / 24      7 / 24   ← meseta, se toma acá
     1.00       12 / 24      7 / 24
```

Sube monótono y se aplana: es una meseta, no un pico afinado sobre estos 24
sorteos. Y el detalle que decide el valor: con 0.00 el clásico con ojos
**pierde** el sorteo 4, que el ciego pasaba; con 0.60 los sorteos que pasa son
un **superconjunto** de los del ciego en los dos peldaños.

**Qué dio, sobre 12 sorteos, en todos los peldaños con postes:**

```text
  peldaño                  vara CIEGA   vara CON OJOS
   8  esquivar un poste      2 / 12        5 / 12
   9  esquivar todos         1 / 12        3 / 12
  13  ida y vuelta           1 / 12        0 / 12   ← empeoró un sorteo
  14  entrega                0 / 12        0 / 12
  15  entrega con DURO       0 / 12        0 / 12
```

Tres cosas, sin suavizar ninguna:

1. **Los peldaños 8 y 9 mejoran de verdad** (se duplica y se triplica), y en
   24 sorteos los conjuntos son superconjuntos: no hay un sorteo que la ciega
   ganara y ésta pierda.
2. **Sigue siendo una rifa.** 5 de 12 no es una capacidad. El peldaño 8 no
   queda calibrado; queda menos descalibrado.
3. **Y el problema es más grande de lo que se creía: los peldaños 13, 14 y 15
   —ida y vuelta y las dos entregas— los aprueba la vara honesta en 0 o 1 de
   12 sorteos, con ojos o sin ellos.** El tercio de arriba del currículo le
   pide a cualquier piloto algo que el modelo físico no logra. El contrato lo
   decía suelto para el 15; acá está medido para los tres.

Qué se verificó antes de decir nada (regla 3.12), y en el visor (regla 3.3):
piloto CLASICO, objetivo (14, 0, 3) distinto de la partida, genoma
`4c52dce5`, y las tres mitades del código nombrando la misma traducción. El
clásico con un poste puesto a (7, 0.2) r=0.7:

```text
  x= 3.74  y=−0.34   a 2.60 m del poste
  x= 6.07  y=−0.78   a 0.66 m    ← se corrió y pasó sin tocarlo
  x= 8.65  y=−0.44   a 1.07 m
  x=14.02  y= 0.01   llegó al objetivo
```

Con la vara ciega ese poste es un choque seguro: iba derecho por y≈0 y el
poste ocupa de −0.5 a 0.9.

#### 12. Lo que esto invalida, ejecutado y no sólo anotado (regla 3.2)

- **Toda comparación de esquive anterior deja de significar**, las dos
  columnas: el híbrido navega con esta misma función, así que **los genomas
  entrenados contra la vara ciega corrigen a un piloto que ya no existe y hay
  que reentrenarlos.** No es una migración.
- La tabla de `capacidades` de la sección 7 del contrato («esquivar postes
  NUEVOS: genoma 1/3, clásico 2/3») queda sin respaldo por el mismo motivo.
- El A/B de esta entrada entero —las dos semillas— se midió contra la vara
  ciega. Lo que dice sobre repertorio-contra-monolítico sigue valiendo entre
  ellos (los dos brazos compartieron la misma vara), pero las
  generaciones-a-aprobar no son comparables con ninguna corrida futura.

#### 13. Y el instrumento que mentía abajo de todo: la batería reprobaba 4 de 6

Encontrado mientras se agregaban las pruebas del esquive, y es el hallazgo de
proceso de la sesión. `cargo test --release`, seis corridas seguidas en el
árbol de ese momento:

```text
  FAILED · FAILED · ok · FAILED · ok · FAILED
```

Siempre la misma prueba (`ordenar_con_la_suave_no_afloja_lo_que_aprueba`) y
siempre en un peldaño distinto (1, 6, …). La causa: `mision::VARIANTE` es un
`AtomicU32` de módulo —tiene que serlo, el entrenador lo lee desde todos los
hilos de rayon— y las pruebas corren en paralelo, así que dos pruebas que
vuelen misiones a la vez se pisan los escenarios, y una que llame dos veces a
`misiones()` esperando los mismos postes recibe dos juegos distintos.

**Esta sesión reportó «105 passed» dos veces sobre corridas que tuvieron
suerte.** Es el peor tipo de instrumento roto, porque la batería «pasa» si se
la corre otra vez y se mira la que salió bien. Arreglado con un candado
(`ESCENARIOS_EN_USO`) que toman las doce pruebas que vuelan misiones: seis
corridas seguidas, 107 de 107, sin una falla.

**El candado solo no alcanzó, y el arreglo completo salió de mirar qué daba
por sentado la prueba.** Con las doce pruebas de `mision` bajo candado la
reprobación bajó de 4 de 6 a **1 de 12**, y con seis pruebas de `repertorio`
también, siguió en 1 de 12. En serie (`--test-threads=1`) no falló nunca en
cuatro corridas, o sea que era paralelismo, pero ningún escritor de
`VARIANTE`, del banco del repertorio ni del flag de censo quedaba sin candado
y el kernel en C tiene todo su estado indexado por agente. **El racer que
quedaba no se encontró.**

Lo que sí se encontró es que la prueba no necesitaba correr el riesgo:
llamaba a `aptitud_y_orden` y **después** a `terminos`, y comparaba los dos
resultados — o sea que daba por sentado que dos evaluaciones separadas ven el
mismo sorteo de escenarios. La propiedad que quiere verificar (la honesta es
el mínimo exacto; la de orden nunca queda por debajo) vale sobre UNA
evaluación, que además es lo que de verdad hace el entrenador. Con
`aptitud_orden_y_terminos` devolviendo los tres números de la misma pasada:
**14 corridas seguidas, 107 de 107.**

Dos reglas salen de acá:

1. **La batería de pruebas también es un instrumento, y «pasó» no significa
   nada si no pasa todas las veces.** Un `cargo test` que hay que correr dos
   veces no es una verificación, es un sorteo — la misma familia que la regla
   3.20, en el arnés en vez de en el entrenador.
2. **Una prueba que mide dos veces para comparar los resultados está
   midiendo, además, que nada cambió entre las dos.** Si la propiedad se
   puede verificar con una sola medición, se verifica con una sola: no es
   esconder la carrera, es dejar de depender de ella.

### 7.35 Cómo terminó el A/B de la semilla 2: lo cortó un reboot, y no importa

Revisado el 2026-09-25. **No hay ningún entrenamiento corriendo.** La máquina
se reinició el 2026-09-23 08:54 y `/tmp` es tmpfs: los checkpoints y logs de
la semilla 2 (MONO y REP) vivían en el scratchpad y se perdieron. A diferencia
de la semilla 1, **no se habían copiado a `genomes/EXP_*`**. Lo que se sabe de
esa corrida es lo que quedó anotado en 7.34 §10 (los dos brazos pasan los
peldaños 8 y 9; 283 contra 219 generaciones; olvido inconcluso por exposición
desigual), y eso no cambia.

**Perderlos no cuesta nada que valga**: los dos brazos se entrenaron contra la
vara CIEGA, y 7.34 §11 ya dejó escrito que esos genomas corrigen a un piloto
que ya no existe (regla 3.2). No había nada instalable ahí.

Estado verificado del visor tras recompilar y reiniciar `dron-visor`: piloto
RED, objetivo (6,0,3) ≠ partida (0,0,3), genoma `4c52dce5…` (v5, el de
siempre), web/mision/capacidades nombran `consignas_hibridas_punto`.
`/entrenamientos` vacío.

**Lección de proceso, que ya había costado una vez (memoria del 09-21) y se
repitió:** una corrida larga en el scratchpad tiene que copiar su log y su
campeón a `genomes/EXP_*` periódicamente, no al final. Un reboot no avisa.

**El procedimiento que sigue, en orden (regla 3.7: cada uno cierra antes del
siguiente):**

1. **Calibrar el generador de escenarios de los peldaños 8, 13, 14 y 15.** La
   vara con ojos saca 5/12, 0/12, 0/12, 0/12. No se baja el umbral (3.2): se
   descartan en el generador los sorteos donde la física no da tiempo de
   reacción (poste demasiado cerca de la partida o del eje con margen menor
   al radio de giro). Criterio de listo: `dron sorteos N clasico n=12` ≥ 10/12
   en cada uno, o una razón medida de por qué no.
2. **Reentrenar desde el genoma v5 contra la vara con ojos**, semillas 1 y 2 en
   paralelo, `salida=` explícito (3.11), copiando a `genomes/EXP_*` cada 20
   generaciones.
3. **`dron sorteos` sobre el campeón** (≥ 10/12 por peldaño) y
   **`capacidades`** contra el clásico con ojos, re-midiendo las dos filas de
   esquivar que están caducadas.
4. Recién ahí instalar en `DRON3D_64.adn` y verificar los cuatro campos de 3.12.
5. Después: el orbitar (autoridad de la red cerca del objetivo, ítem 2 del
   contrato) y la capa de tarea en runtime (ítem 9).

### 7.36 La vara volaba ciega hacia atrás y cambiaba de lado a mitad del rodeo

Paso 1 del procedimiento de 7.35: calibrar los peldaños de postes **contestando
primero la pregunta 1 de la regla 3.7 con la vara**, no moliendo generaciones.
Herramientas nuevas para eso: `dron mision N M ... variante=V` (pincha el
sorteo V de `dron sorteos`, que usa 500001+i), el informe imprime los postes, y
`DRON_TRAZA_FINA=1` da la traza entera a 0.05 s (los choques del peldaño 8
pasaban a 1.3 s y la traza de 2 s no mostraba ni una fila).

**Tres defectos de la vara, todos medidos en traza (regla 3.5):**

1. **El lado del rodeo salía del sector, y los sectores miden 32°.** Sorteo 3,
   poste a 0.16 rad de la línea: eligió bien el lado, la nariz giró hacia el
   comando, el poste cruzó de sector y el «hueco» pasó a ser el otro lado — se
   dio vuelta y lo chocó a 1.3 s. El compromiso de 7.34 no lo cubría porque
   mide velocidad lateral en el CUERPO. Arreglo: el lado lo fija el rumbo
   continuo `obs_horizontal` (la red lo ve en sus bits) contra el rumbo al
   objetivo; es estable por geometría. Arregló ese sorteo; en el agregado no
   movió el número (12-13/24), porque debajo había otro defecto.
2. **Aceleraba hacia donde la cámara no mira.** Clasificados los 12 fallos de
   24 sorteos del peldaño 8 con sensores LIMPIOS: **los 12** tenían el
   objetivo a 110-178° de la nariz (el dron arranca mirando a +x, la cámara
   cubre ±80°) y el poste a menos de 16° de la línea. Volaba ciego. Arreglo de
   industria (sensado sólo frontal ⇒ girar y después acelerar):
   `Guiado::tope_por_mirada`, rapidez ≤ `V_CIEGA` = 0.5 m/s en el borde del
   cono, sin tope en la mitad central. Barrido: 0.5 → p8 20/24, p9 15/24;
   1.5 → 12 y 5; sin tope → 13 y 6. Peldaños 1-7 no pierden (7 sube 22→24).
3. **El margen del objetivo era el margen de partida de la vuelta.** En «ida y
   vuelta» la vuelta arranca en hover desde B con el poste a 1.4 m de la
   superficie — el looming de un dron quieto es cero. Márgenes simétricos:

   ```text
     márgenes    p8   p9   p13     (clásico, 24 sorteos)
     2.5 / 1.5   20   15    0
     2.5 / 2.5   17   13    3
     3.0 / 3.0   22   16    6
   ```

**La vara, peldaño por peldaño, 24 sorteos, después de los tres:**

```text
  peldaño   1-7    8    9-12   13    14   15
  antes     ≤24   12     6      0     0    0     (vara con ojos de 7.34)
  ahora      24   22    16      6     0    0
```

(9-12 dan idéntico porque el mínimo lo pone el examen de repaso del 9.)

**Lo que sigue sin calibrar:** peldaño 9 (16/24 todavía es rifa en un tercio
de los sorteos), 13 (6/24), 14-15 (0/24). **El 14 pasa en LIMPIO en las tres
misiones** (4248, 4544, 4439) y cae con ruido real/duro — o sea que ya no es
el poste: es robustez a ruido de la cadena entera (posadas incluidas). Es un
problema distinto y es el próximo.

Por la regla 3.2, **todo número de genoma anterior queda sin comparación** —
ya lo estaba desde 7.34 §11. Reentrenamiento lanzado contra esta vara: dos
semillas, monolítico, sembrado de v5, 1200 generaciones, con salida, log y
binario en `genomes/EXP_20260925_*` (no en `/tmp`, por 7.35).

### 7.37 El lazo de giro no tenía I: una carga corrida lo dejaba inclinado para siempre

Disparador: un video de control de actitud de quadrotor (embodied.dev) que
mostraba tres cosas de manual: la I atrapa una masa colgada fuera del centro,
la I «sigue empujando después de que el error se fue» (windup), y la D mira el
giro medido, nunca el error. Las tres se chequearon contra `Estabilizador3`
**midiendo**, no leyendo (regla 3.5).

**1. La I: faltaba, y la tarea 3 del producto la necesita.** Los lazos de
velocidad angular de roll/pitch tenían `ki = 0`. Con un torque parásito
constante —la caja del imán colgada fuera del centro, sección 1— en hover:

```text
  torque     0.02    0.05    0.08    0.12 N·m
  sin I      3.3°    8.3°   13.3°   19.9°   inclinación PERMANENTE
  con I      0.00°   0.00°   0.00°   0.00°
```

Prueba: `una_carga_corrida_no_deja_inclinacion_permanente`.

**2. La I sola ROMPE la vara — el windup del video, medido.** Clásico, 12
sorteos, peldaños 1/8: ki 0 → 12/11; 0.3·kp → 12/7; 1·kp → 12/0; 2·kp → 0/0.
Se carga en las maniobras y después empuja de más. El remedio de Betaflight,
**iterm relax**: no integrar mientras la consigna cambia rápido. **Primero se
hizo mal**: relax por la MAGNITUD de la consigna — la vara volvió a 12/10,
pero la inclinación volvió a 8.2°, porque una carga corrida pide un giro
sostenido y eso apagaba la I justo cuando hacía falta. Bien hecho: relax por
la parte RÁPIDA (consigna menos su pasa-bajos de 0.1 s). Con ki = 2·kp y
relax 0.2 rad/s, 24 sorteos:

```text
  peldaño    1    7    8    9   13
  sin I     24   24   22   16    6
  con I     24   24   21   14    7      (±2 es el ruido de 24 sorteos)
```

Anti-windup condicional además (no integra con la salida saturada hacia el
mismo lado).

**3. La D sobre la medida: probada y DESCARTADA.** Clásico 0/12 en los
peldaños 1 y 8, con y sin I, con y sin tope de salida. En esta cascada la D
sobre el error trae adentro `kd·d(consigna)/dt`, y como la consigna de giro es
`GAN_ANG × error de ángulo`, ese término hace de prealimentación del lazo
externo; las ganancias se afinaron contando con él. Pasarla a la medida exige
re-afinar la cascada entera. No se reabre sin ese re-afinado. (Por la regla
3.1 este archivo se reemplaza en hardware por Betaflight, que sí la tiene: la
diferencia vuelve a importar el día del perfil DBPOWER.)

**Cambió el camino caliente con un entrenamiento vivo → se cerró a propósito**
(excepción de la sección 4). Lo que dejó la corrida contra la vara de 7.36
con el estabilizador viejo, archivada en `genomes/EXP_20260925_ojos736/`:

```text
              peldaño 9   peldaño 10   peldaño 11 (posarse)
  semilla 1    gen 13      gen 199      trabado desde gen 232 (en gen 264)
  semilla 2    gen 20      gen  70      trabado desde gen  99 (en gen 240)
```

Contra la semilla 1 de 7.34, que no pasó el 9 en 1200 generaciones, es el
primer indicio de que calibrar la vara destrabó el esquive — con 6
confirmaciones por aprobado (regla 3.20). Nuevo trabado: **posarse**, cruzando
el umbral y cayendo en la 1ª-3ª confirmación. Relanzado con el estabilizador
nuevo en `genomes/EXP_20260925b/`.

### 7.38 Auditoría de puntos ciegos: `dron estres`, y cuatro defectos que nada medía

Pregunta del usuario, después de 7.37: *«¿en cuántas más cosas nos está
sucediendo esto y ni siquiera estamos enterados?»*. La respuesta no se razonó:
se armó un instrumento y se midió.

**El instrumento: `dron estres [peldaño] [n=24]`.** Vuela la vara (clásico) en
el mismo peldaño sano y bajo cada avería que el simulador YA tenía en
`sim3d::Averia` —carga corrida, motor flojo en guiñada, menos empuje, caja
extra, viento ×3, GPS congelado— y cuenta además cuántos pasos del vuelo sano
piden algo que cuatro motores no pueden dar. **Primer hallazgo, de proceso:
esas averías existían desde `etapas.rs` y el gym de misiones no las usó
nunca** — el currículo entero se entrenaba con el aparato sano (regla 3.19:
dos piezas que obviamente iban conectadas). `mision::ESTRES` las conecta.

**Antes de arreglar nada (24 sorteos, ruido REAL):**

```text
                         p7   p11 posarse   p13 ida y vuelta
  sano                   24       24              13
  motor flojo (guiñada)  10       24               0     ← A
  15% menos empuje       24        6              13     ← B
  caja de 300 g          24        0              13     ← B
  pasos fuera del sobre  1.2%     2.7%           4.3%    ← C
```

**A. La guiñada tampoco tenía I.** Mismo defecto que 7.37 en el eje que
faltaba: 0.03 N·m de un motor flojo y el dron gira sin parar. I con el mismo
relax: p7 10 → 24, p13 0 → 12.

**B. Aterrizar dependía de conocer la masa exacta.** Bajaba al tope (2.5 m/s)
hasta muy abajo y frenaba pegado al piso; con 300 g de más —la caja del imán—
tocaba a 1.75 m/s. Subir la I de altura EMPEORA (ki 2 → 6/12, 6 → 0/12, 12 →
0/12 con 150 g): no era la I. Es lo que PX4 (`MPC_LAND_SPEED`) y ArduPilot
(`LAND_SPEED`) resuelven con dos velocidades: debajo de 2 m, no más de
0.8 m/s. `gym3d::altura_de_aproximacion`. Caja 300 g: 0 → 23/24; 15% menos
empuje: 6 → 24/24. Levantar la caja EN EL AIRE ya estaba bien: 300 g hunden
0.37 m (`medicion_caja_de_golpe`).

**C. El simulador no tenía mezclador de motores.** Empuje y torques se
aplicaban por separado, cada uno con su tope: a acelerador máximo, torque
completo — imposible en un quad X. `sim3d::mezclar_x`, con prioridades de
airmode (se recorta guiñada, después roll/pitch, y el empuje se corre antes
que perder actitud). **La primera versión estuvo mal y rompió la vara**:
normalizaba suponiendo que `TORQUE_MAX` es lo que dan los motores, y con
geometría real de un 5" (brazo 0.078 m, 8.8 N por motor) dan 1.37 N·m — el
0.42 es tope del controlador. Con eso, un giro de 43% en dos ejes saturaba, el
airmode subía el empuje y el clásico se estrellaba posando (peldaño 12,
misión 2: 1294 → −43.7; lo cazó la batería de pruebas). Con la geometría
física, `BRAZO_EJE` y `K_GUINADA`, 0.0% de pasos fuera del sobre.

**D. Sin efecto en misiones, y hay que decirlo:** viento ×3 y GPS congelado
no mueven nada. El GPS congelado **no está conectado** a los sensores de la
misión (sólo a `etapas.rs`): no es robustez, es un hueco del instrumento.

**La vara, después de todo (24 sorteos):**

```text
  peldaño   1-7   8    9-12   13   14   15
  7.36       24   22    16     6    0    0
  7.38       24   22    18    17    6    1
```

**Y el estrés después:** p7 24/24 en todas las averías; p11 24/24 salvo 40%
menos empuje (0/24) y caja 300 g (23/24); p13 sano 19, con carga corrida 13 y
motor flojo 12; p14 sano 9, con carga corrida **0**. **Abierto:** la carga
corrida todavía rompe las misiones largas aunque el hover quede derecho —
hay que trazarlo—; 40% menos empuje en posada; y conectar el GPS congelado.

Todo esto es camino caliente: la corrida `EXP_20260925b` (estabilizador de
7.37) se cerró a propósito en gen ~230 (s1 en peldaño 13, s2 en 10) y se
relanzó en `genomes/EXP_20260925c/`.

**La regla que sale de acá, para 3.18:** el instrumento que faltaba no era
otra prueba de capacidades — era volar la VARA bajo perturbaciones. La vara
no aprende, así que todo lo que la rompe es un defecto del modelo físico o
del controlador, nunca "el genoma no aprendió".

### 7.39 Sin GPS: la posición ahora es odometría que deriva

Decisión del usuario, innegociable y «el corazón del proyecto»: el dron no
usa GPS. Hasta hoy `Sensores::leer` daba posición absoluta con 0.25 m de
ruido y 45 de los 96 bits del cerebro dependían de ella. La corrida
`EXP_20260925c` (con GPS) se canceló y quedó archivada como
`genomes/EXP_20260925c_CON_GPS/` — no es comparable con nada de acá en
adelante.

**El modelo nuevo (`sim3d::Sensores`, `SIN_GPS = true`):** lo que tiene un
dron de juguete sin GPS —flujo óptico mirando al piso + IMU—. Mide velocidad;
la posición es su integral y nadie la corrige:

- escala del flujo mala por vuelo, ±`ESCALA_FLUJO` = 8%;
- sesgo del flujo por vuelo, `SESGO_FLUJO` = 0.04 m/s;
- sesgo del giróscopo en guiñada por vuelo, `SESGO_GUINADA` = 0.009 rad/s
  (~0.5°/s, un MPU-6050 calibrado): sin magnetómetro el rumbo deriva y
  **rota toda la odometría**.

Son supuestos del orden de la hoja de datos, **no mediciones propias**
(regla 3.18); se miden el día que haya placa. La evaluación (llegó, chocó,
se posó) sigue usando la verdad del simulador: el dron cree estar en un lugar
y el juez mira dónde está de verdad.

**La vara, 24 sorteos:**

```text
  peldaño              1-7   8   9-12   13   14   15
  con GPS (7.38)        24   22   18    17    6    1
  sin GPS, sólo flujo   24   23   18    13    5    1
  + deriva de rumbo     24   23   18    16    2    0
```

Las misiones cortas casi no lo sienten (5-9 m: un 8% de escala queda dentro
del radio de llegada de 1 m). **La entrega (14) es la que cae: 6 → 2**, porque
es la más larga y la que vuelve a la base. Es lo esperable y es el problema
correcto a atacar: el dron tiene que corregir su deriva con lo que ve (el
objetivo marcado está en la cámara, `ver_base`) en vez de confiar en la
integral.

**Persecución:** a la moto no se la geolocaliza, se la VE. `persecucion::
visto_desde` pasa la posición/velocidad de la moto al marco odométrico del
dron (relativa a él, como la da la cámara). Sin eso el clásico caía a 51% a
tiro en «recta de ciudad»; con eso vuelve a pasar sus pruebas.

**Batería:** 109/109 en 10 corridas seguidas, después de arreglar una carrera
vieja que apareció acá (1 de 9 corridas reprobaba dos pruebas de
`repertorio`): las 11 pruebas del módulo comparten los slots 101/102 del
kernel, que son globales. Candado `slots()`, igual que el de `mision`.

Visor reiniciado con el modelo sin GPS. Entrenamiento relanzado sin GPS:
`genomes/EXP_20260925d_singps/`, dos semillas, 1200 generaciones.


### 7.40 La sección 7 del contrato, tal como estaba antes de la purga del 2026-09-26

Movida acá entera (regla 3.4: nada se borra, se mueve). En el contrato quedó
sólo el estado vigente; esto es el registro de cómo se llegó.

#### (antes) 7. Estado hoy

> **Éste es el ÚNICO lugar que dice qué es verdad ahora.** Si una tabla de
> `BITACORA.md` o del README dice otra cosa, gana ésta — y si la que tiene razón
> es la otra, se corrige ésta, no se citan las dos. Antes de la auditoría
> convivían cuatro tablas de capacidades con números distintos y sin marcar.

**Tarea activa: la ENTREGA** (`mision.rs`), pero leída con la corrección de la
sesión 7.26/7.29: **las tareas del gym son andamio, no el producto** — ver la
sección 1. Persecución queda pausada, no cerrada, deuda en bitácora 7.2/7.4.

**SIN GPS desde el 2026-09-25 (bitácora 7.39).** Vara, 24 sorteos: p1-7 24,
p8 23, p9-12 18, p13 16, p14 2, p15 0. Entrenamiento corriendo:
`genomes/EXP_20260925d_singps/`. Lo de abajo (7.36-7.38) se midió CON GPS.

**Auditoría de estrés 2026-09-25 (bitácora 7.38):** guiñada con I, aterrizaje
en dos velocidades (PX4), mezclador X real en la física. Vara, 24 sorteos:
p1-7 24, p8 22, p9-12 18, p13 17, p14 6, p15 1. Abierto: carga corrida en
misiones largas (p14 9→0), 40% menos empuje al posar, GPS congelado sin
conectar a las misiones. Entrenamiento corriendo en `genomes/EXP_20260925c/`.

**Vara recalibrada el 2026-09-25 (bitácora 7.36):** el clásico dejó de volar
ciego hacia atrás (`tope_por_mirada`), fija el lado del rodeo con el rumbo
continuo, y los postes quedan a 3 m de ambos extremos. Clásico, 24 sorteos:
p1-7 24, p8 22, p9-12 16, p13 6, p14-15 0 (el 14 pasa en limpio: falta
robustez a ruido, no esquive). **Entrenamiento corriendo**: dos semillas
contra esta vara y con el lazo de giro con I (7.37),
`genomes/EXP_20260925b/s{1,2}.{adn,log}`, visible en el panel del visor. La
corrida anterior (sin I) pasó esquivar en gen 13/20 y se trabó en «posarse». Procedimiento restante: bitácora 7.35 pasos 3-5. **Una corrida larga copia log y campeón a `genomes/EXP_*`
mientras corre, no al final: `/tmp` se borra al bootear.**

**Genoma instalado**: `genomes/DRON3D_64.adn`, md5 `4c52dce5…`, v5 — **es el
genoma de ANTES de las sesiones 7.26-7.34, y sigue siendo el que se sirve.**
Nada de lo entrenado desde entonces se instaló, y con razón: el A/B de 7.34
terminó con los dos brazos trabados en el peldaño 8/9 y con el «aprobado» del
brazo del repertorio refutado (3 de 12 sorteos). Los genomas de esas corridas
están guardados con nombre propio en `genomes/EXP_20260921_*` para no
perderlos, y **no se instala ninguno**: no aprueban la escalera y no pasaron
`capacidades` — instalar a mitad de corrida sería repetir el error que ya
costó una sesión entera (bitácora 7.16-7.24).

**`capacidades`, medido el día de la auditoría — CADUCADO por la regla 3.9.**
Se midió sobre el genoma v5 instalado, antes de la aleatorización de
currículo y del split del peldaño de entrega (7.26-7.29). Queda la tabla
porque sigue describiendo el genoma que HOY está instalado, pero **no se cita
como el estado del proyecto** — eso es la corrida en curso, sin terminar.
Re-medir con `capacidades` en cuanto haya un genoma nuevo que valga la pena
instalar, límite de sacudida puesto (bitácora 7.5):

```text
   capacidad               genoma   clásico
   despegar                  3/3      3/3
   ir de A a B y frenar      5/5      5/5      (pero: ver el imán, abajo)
   quedarse quieto           0/3      3/3      ← era "3/3" con el instrumento roto
   posarse entero            3/3      3/3
   recuperar de vuelco       2/3       —
   esquivar (postes vistos)  3/3      0/3      ← memoria, NO capacidad
   esquivar (postes NUEVOS)  1/3      2/3      ← CADUCADO: la vara ya no es ésta
   entregar un paquete       2/3      1/3
```

Las dos líneas que hay que leer juntas, sin suavizar:

0. **Las dos filas de esquivar están CADUCADAS desde el 2026-09-21**, y no
   por envejecimiento sino porque el instrumento cambió: la columna del
   clásico se midió con una vara que no miraba los postes, y la del genoma con
   un híbrido cuya base era esa misma vara ciega (regla 3.2 / 3.9). El «1/3
   contra 2/3» de abajo no se cita más hasta re-medirlo con
   `Guiado::consignas_esquivando` puesto y un genoma reentrenado contra ella.
1. **Esquivar: el 3/3 era el conjunto de ENTRENAMIENTO.** La prueba corría con
   `gym3d::obstaculos(i)`, o sea los mismos postes con los que el genoma se
   entrenó, y durante meses se citó ese 3/3 contra 0/3 como "la red hace algo
   que la vara honesta no puede". **Con postes en posiciones nuevas el genoma
   saca 1/3 y el clásico 2/3: la red es PEOR que el modelo físico.** Se estrella
   en dos de tres configuraciones (roce −0.01, o sea que los toca). Lo cazó el
   usuario mirando el visor —"el dron vale verga para esquivar"— no la batería
   de pruebas, que decía lo contrario porque medía memoria. Ver bitácora 7.17.
2. **Quedarse quieto: 0/3 contra 3/3.** El genoma se estaciona a 0.89 m del
   punto **moviéndose 0.55-0.61 m**; el clásico queda a 0.01 m moviéndose
   0.02 m. El «5/5» de ir a A→B es cierto contra su propia vara y engañoso como
   descripción: las cinco rutas terminan entre 0.89 y 0.95 m con tolerancia de
   1 m, y el clásico entre 0.30 y 0.49. **El orbitar nunca se cerró** — estaba
   tapado por el instrumento roto de 3.18.

**Lo que bloquea, en orden. La 0 bloquea a las demás (regla 3.7):**

0. ~~Reentrenar de cero con el currículo que no saltea~~ **HECHO, y superado
   por algo mejor.** No hacía falta sólo no saltear peldaños — hacía falta que
   el currículo no fuera tres escenarios memorizables. Ver ítem 1.
1. **El olvido catastrófico ERA, en gran parte, sobreajuste — y el arreglo
   real fue afuera del cerebro, no adentro** (bitácoras 7.26, 7.28, 7.29).
   Durante varias sesiones se buscó la causa adentro de la red (más capacidad,
   pesos por régimen, examen de repaso más ancho) y la causa principal estaba
   en el currículo: postes y rutas eran tres mapas y tres rumbos FIJOS
   (`mision.rs`), así que lo que pagaba era memorizarlos, y una memoria de
   coordenadas es conocimiento frágil que la búsqueda pisa apenas otro peldaño
   necesita esos mismos pesos. Con `mision::rutas()`/`postes_en_ruta`
   generando ambos de nuevo cada generación (aleatorización de dominio, la
   misma idea que ya existía para el ruido de sensores), medido con semilla 1:

   ```text
     escenarios          llega a peldaño 13   llega al final   olvida
     FIJOS (7.25)            gen 576              nunca          los 12
     ALEATORIOS (7.28)       gen 23-31            gen 34         ninguno
   ```

   Un segundo hallazgo, encima de ése (bitácora 7.29): el peldaño final
   "entrega" pedía esquivar + seis maniobras + ruido duro los tres a la vez, y
   ni el clásico lo aprueba con ningún ruido (nunca esquiva, por diseño — no
   es bug de la vara). Lo que sí importaba: el mismo campeón sacaba 4185-4495
   con ruido DURO y se estrellaba con ruido LIMPIO en la misma misión —
   esquive por golpe de suerte del ruido, no esquive robusto. Se partió en dos
   peldaños (15 en vez de 14: "entrega" limpio+real, "entrega" con los tres
   niveles al final), mismo remedio que ya usaba `ir a B`→`sensores duros`.
   **El «15/15 en la generación 100» está REFUTADO (bitácora 7.30).** La
   corrida siguió y el log dice otra cosa:

   ```text
     gen 100  mejor  2817.4  → peldaño 15 (APROBADO)
     gen 110  mejor   -96.6
     gen 379  TRABADO hace 200 generaciones (necesita 2400, va -115.0)
           └ ⚠ DESAPRENDIÓ los peldaños [9, 10, 11, 12, 13, 14]
   ```

   No fue una recaída: **la aprobación nunca fue real.** Con escenarios
   re-sorteados cada generación, cruzar el umbral una vez mide el sorteo,
   no la capacidad. Arreglado en el mismo cambio: aprobar ahora exige que
   el campeón vuelva a pasar el umbral con un juego de escenarios
   INDEPENDIENTE, y si no pasa lo grita. **Ojo con las tablas viejas:** eso
   sube la vara de aprobación, así que las generaciones-a-aprobar de 7.28 y
   7.29 no son comparables con las de ahora (regla 3.2).

   **Lo que NO se cae:** la aleatorización de dominio sigue siendo el mejor
   cambio de esas sesiones — la escalera se sube de verdad y rápido hasta el
   peldaño 14, y el olvido desapareció de todo ese tramo. Lo que se cae es
   el peldaño final como "aprobado".

   **Y el criterio arreglado tenía, a su vez, un bug de instrumento
   (bitácora 7.32).** La confirmación usaba un `u32` grande y `rutas()`
   perdía precisión ahí: **todas las confirmaciones caían en el MISMO
   escenario degenerado**. Los "46 rechazos por suerte del sorteo" no eran
   46 sorteos independientes. Arreglado; ese diagnóstico hay que rehacerlo.

   **Lo que de verdad trababa, encontrado abajo de eso:** los peldaños 9, 10
   y 11 —despegar y las dos posadas— **nunca se aleatorizaron**. Eran tres
   mapas escritos a mano, o sea lo mismo que 7.28 ya había medido como la
   causa principal del olvido, olvidado en tres de quince peldaños. Y son
   exactamente los tres donde las corridas se trababan. Ya están
   aleatorizados: la dispersión de sus misiones propias pasó de **0.0 a
   78.2**.

   **El A/B de `sorteos=1` contra `sorteos=2` está CERRADO** (bitácora 7.33
   §9): por generación, `sorteos=2` quedó estrictamente atrás y los dos brazos
   terminaron contra el mismo muro, así que no decide la pregunta de calidad.
   Se cerró a propósito, con los logs guardados.

   **Pesos separados por régimen** sigue como candidata si algún día hace
   falta más — pero con este resultado, no hay evidencia de que haga falta
   (7.4: especializar desde cero pierde contra un genoma que vio el currículo
   entero; acá el generalista ya está ganando).

   **El A/B monolítico contra repertorio, terminado y con su titular
   REFUTADO** (bitácora 7.34). El brazo del repertorio marcó el peldaño 9 como
   aprobado en la generación 933 y el monolítico nunca lo marcó en 1200; con
   doce sorteos independientes el resultado es otro:

   ```text
     peldaño 9, umbral 900, 12 sorteos    peor   mediana    mejor   aprueba
     clásico (la vara honesta)           −49.4     −45.9   1131.4   1 de 12
     MONO, 1200 generaciones             −51.2     −45.4    −37.2   0 de 12
     REP (el repertorio guardado)       −282.4     −47.9   1032.0   3 de 12
   ```

   **3 de 12 no es una capacidad, es una rifa**, y los tres pilotos pasan
   sorteos DISJUNTOS (el clásico el #4, el repertorio el #5, #8 y #11), así
   que no hay escala entre ellos. Por qué pasó con tres confirmaciones
   puestas: la cuenta se había hecho por intento y el guardián se consulta 425
   veces por corrida — es la quinta fila de la regla 3.20, y `CONFIRMACIONES`
   pasó de 3 a 6 con la cuenta escrita al lado.

   **Lo que SÍ queda medido a favor del repertorio, y es grande: el olvido.**
   Mismo peldaño, mismo sorteo, examen de repaso peldaño por peldaño:

   ```text
     peldaño de atrás     MONO      REP
           8             −41.7    −38.9   ← comparte celda con el 9, por diseño
           2             −33.5   1051.9
           1              21.1    959.5
           3              19.9   1052.2
   ```

   Y el mecanismo, que explica el resultado y predice el próximo: cruzando eso
   con el censo de celdas de 7.33 §2, **el repertorio no elimina el olvido —
   lo achica en proporción a la fracción de la misión que NO pasa por la celda
   que se está entrenando.** Entrenando «crucero tapado» (0-11% del vuelo de
   los peldaños 1-7) sólo se pierde el peldaño 8; entrenando «sostén libre»
   (21-64%) se pierden los nueve, y eso es exactamente lo que hizo la corrida
   al llegar al peldaño 10.

   **Falta, antes de instalar nada:** el peldaño 8 calibrado (ítem 1-bis
   abajo), el A/B rehecho contra un peldaño calibrado, y `capacidades` contra
   el genoma que salga.
1-bis. **EL BLOQUEO DE VERDAD, encontrado el 2026-09-21: la vara honesta no
   miraba los postes, y por eso el tercio de arriba del currículo pedía lo
   imposible.** Es la primera de las tres preguntas que la regla 3.7 exige
   contestar **antes** de moler generaciones, y para estos peldaños no se
   había contestado nunca. Apagar la red da la vara gratis (regla 3.15), y
   `dron sorteos N clasico n=12` da esto:

   ```text
     peldaño                       vara CIEGA (antes)   vara CON OJOS (hoy)
      7  sensores duros, sin postes      11 de 12            11 de 12
      8  esquivar UN poste                2 de 12             5 de 12
      9  esquivar todos                   1 de 12             3 de 12
     13  ida y vuelta                     1 de 12             0 de 12
     14  entrega                          0 de 12             0 de 12
     15  entrega con ruido DURO           0 de 12             0 de 12
   ```

   *(Tabla SUPERADA por la bitácora 7.36: con la vara recalibrada el
   clásico da p8 22, p9 16, p13 6 de 24. Queda por el historial.)*
   **PARCIALMENTE ARREGLADO** (`Guiado::consignas_esquivando`, bitácora 7.34
   §11): la vara ahora sigue el hueco sobre el anillo de proximidad de cinco
   sectores —que ya existía y nadie le había conectado al piloto clásico— con
   compromiso con la maniobra empezada. Los peldaños 8 y 9 se duplican y se
   triplican, y sobre 24 sorteos los que pasa son un **superconjunto** de los
   que pasaba la vara ciega: la vara subió, no se movió de lugar. Con el
   anillo despejado devuelve EXACTAMENTE `consignas`, con prueba
   (`sin_nada_que_esquivar_es_exactamente_el_clasico`), así que la identidad
   de la regla 3.15 sigue en pie y los peldaños sin postes siguen comparables.
   Verificado en el visor: el clásico se corre a y=−0.78, pasa a 0.66 m de un
   poste puesto a (7, 0.2) y llega al objetivo.

   **Lo que NO está arreglado, y es lo que bloquea ahora:**

   - **5 de 12 sigue siendo una rifa.** El peldaño 8 no está calibrado; está
     menos descalibrado. En los sorteos que pierde, el clásico roza el poste a
     0.00 m y cualquier ruido lo mete adentro.
   - **Los peldaños 13, 14 y 15 están en 0 o 1 de 12 con ojos y sin ellos.**
     El tercio de arriba del currículo le pide a cualquier piloto algo que el
     modelo físico no logra. El contrato lo decía suelto para el 15; ahora
     está medido para los tres y hay que tratarlo como lo que es: **esos
     peldaños no están calibrados y moler generaciones contra ellos es lo que
     la regla 3.7 prohíbe.**
   - **Y esto NO autoriza bajar el umbral** (regla 3.2). El umbral es
     `LLEGADA × puntos + COMPLETA` —cumplir la misión—, que es el objetivo
     correcto. Lo que hay que revisar es el generador de escenarios: con
     postes sorteados hay sorteos donde la física no da tiempo (§3.3), y un
     peldaño cuyo peor caso es imposible mide el sorteo, no al piloto.

   **Lo que este cambio invalidó, y hay que ejecutarlo antes de citar
   cualquier número de esquive** (regla 3.2): el híbrido navega con la misma
   función, así que **los genomas entrenados contra la vara ciega corrigen a
   un piloto que ya no existe y hay que reentrenarlos.** Eso incluye el 1/3
   contra 2/3 de la tabla de `capacidades` de abajo y las dos semillas del A/B
   de 7.34.
2. **El orbitar, todavía sin tocar** (0/3 en quietud, medido antes de esta
   sesión). El camino diagnosticado y sin implementar: la red no reduce su
   autoridad cerca del objetivo en vuelo horizontal —sólo lo hace cerca del
   suelo, para posar (`autoridad_de_posada`)— así que un residuo con autoridad
   completa perturba un hover que el modelo físico ya resuelve casi perfecto.
   Mismo patrón que ya funcionó una vez en este archivo, aplicado al eje que
   falta. No se implementó todavía porque mezclarlo con el experimento del
   currículo hubiera arruinado la atribución (regla 4). Es el candidato
   OBVIO para la próxima sesión de mejora, una vez que el ítem 1 cierre.
3. **Posarse (2/3, clásico 3/3), sin tocar.** La pista de la industria, sin
   explorar: PX4 alinea primero y desciende después. El dron hace lo
   contrario, y llega a 82 cm derivando de lado.
4. ~~Esquivar dos postes simultáneos~~ **Reemplazado por el ítem 1**: con
   postes en posiciones nuevas cada generación, "esquivar de memoria" ya no
   es una estrategia posible — se mide junto con todo lo demás.
5. **Probado y REVERTIDO — dejarlo así, no reabrir sin nueva evidencia.**
   `csrc/iafz_64.c::forward_iafz_64`: la capa de estrategia comparaba contra
   `[l1_out, 0ULL]` (palabra alta constante). Se probó `[l1_out, ctx_hi]`
   (bitácora 7.11 lo tenía como candidato barato) y **el A/B bien hecho
   —misma semilla, mismo currículo aleatorizado, único cambio el kernel—
   dio que el kernel NO explicaba ninguna mejora**: los dos brazos llegaron
   al final en la MISMA generación (34). Un primer A/B mal controlado (con
   postes todavía fijos) había sugerido lo contrario — quedó anotado en
   bitácora 7.25 y corregido en 7.28. La media capa de estrategia sigue
   pagada y vacía si hace falta más adelante, pero la evidencia de hoy dice
   que el cuello de botella no estaba ahí.
6. **La prueba `costo_de_computo` era inestable bajo carga. ARREGLADA — pero
   el "arreglada" de la auditoría estaba a medias y hubo que volver.** El
   diagnóstico era correcto (5 tandas de 50.000 iteraciones son tan largas
   que la carga las degrada a las cinco: 21.7 µs contra 5.8 µs en reposo) y
   el remedio también (25 tandas de 5.000, así alcanza con que una caiga en
   un hueco tranquilo), **pero se aplicó a UNA sola de las dos pruebas del
   módulo.** `una_decision_de_mezcla_cuesta_casi_lo_mismo` (borrada con la mezcla, 2026-09-26) se quedó con las
   5 × 50.000 y volvió a reprobar hoy, con dos entrenamientos ocupando los
   núcleos. Ahora sí están las dos. La lección, que es de proceso: **cuando
   se arregla una clase de bug hay que buscar los hermanos en el mismo
   archivo**, o el "arreglado" del contrato queda mintiendo.
7. *(Mitigado en la vara el 2026-09-25: `Guiado::tope_por_mirada` no deja
   volar a más de 0.5 m/s hacia donde la cámara no mira — bitácora 7.36. La
   ceguera de sensor sigue igual.)* **El dron es CIEGO fuera de un cono de 160° hacia adelante — no hay atrás,
   costados traseros, ni arriba/abajo independiente.** Lo preguntó el usuario
   directo: *"si hay un obstáculo atrás, arriba, a 45°, ¿lo ve?"* No. `Camara`
   (`sim3d.rs`) es UNA cámara frontal, `campo_obstaculos = 1.4 rad` (80° a cada
   lado del eje de la nariz). Nada mira hacia atrás ni a los costados traseros;
   `obs_altura` sólo describe la elevación de un obstáculo que YA está dentro
   de ese cono, no agrega cobertura nueva. Si algo se acerca por detrás, el
   dron lo choca sin haberlo "visto" nunca — la física sigue aplicando la
   colisión aunque el cerebro no tuviera cómo anticiparla.
   **Decisión explícita del usuario: PENDIENTE A PROPÓSITO, no se toca todavía.**
   Su razón, textual: *"no tendría sentido preparar un anillo de cámaras si ni
   siquiera puede volar solito con una cámara"*. Primero hardware con la cámara
   frontal que ya existe; recién con eso funcionando se evalúa cobertura 360°
   (que además es una pregunta de HARDWARE —cuántas cámaras/sensores entran en
   el DBPOWER— no sólo de software).
8. **Adaptación en vuelo y repertorio por nicho: CONSTRUIDOS y verificados en
   el visor, sin validar todavía dentro del gym** (bitácora 7.26). `vivo.rs`
   (búsqueda local anclada, mientras vuela) + `repertorio.rs` (un genoma por
   situación — MAP-Elites, Cully et al. Nature 2015) le dan al dron una forma
   de seguir mejorando después de salir del entrenamiento, y hacen el olvido
   catastrófico imposible por construcción dentro de una sesión de vuelo
   (aprender en un nicho no puede escribir en otro). Verificado en vivo, tecla
   `V`: el dron midió solo que en un nicho su red estorbaba, probó variantes,
   adoptó una mejor. **Lo que falta:** lo mismo que el ítem 1 — nada de esto
   se probó todavía DENTRO de un entrenamiento largo, sólo en vuelo libre.
9. **La tarea todavía está compilada adentro del binario, no es algo que se
   pueda pedir.** Corrección del usuario (bitácora 7.29, sección 1): el
   objetivo de fondo no es "entregar un paquete", es que se le pueda dar
   CUALQUIER tarea — una cadena de puntos arbitraria, recoger y clasificar
   cajas — sin reentrenar. Hoy `mision.rs` tiene la cadena de puntos escrita a
   mano. Falta una capa de tarea que se reciba en runtime (simbólica, casi
   gratis en cómputo) separada de las maniobras que el currículo enseña — es
   la palanca de mayor impacto y menor riesgo que tiene el proyecto ahora
   mismo: no toca el cerebro, no invalida ningún genoma. Sin empezar.

10. **Sin GPS: HECHO el modelo, abierto el problema** (bitácora 7.39). Con
   odometría que deriva, la vara cae en la entrega de 6 a 2 de 24. Lo que
   falta es corregir la deriva con lo que la cámara VE (el objetivo marcado),
   que es exactamente la tesis del proyecto: arreglárselas con sensores
   baratos. (La avería «GPS congelado» ya salió de `dron estres`.)

**Para hardware real, actualizado** (bitácora 7.21/7.22, ya no es 0%):

- ✅ **Puente MSP** (`src/puente.rs`): consignas → canales de radio →
  `MSP_SET_RAW_RC`. Probado en seco contra el formato, no contra una placa real.
- ✅ **Piloto de seguridad**: nunca se arma solo, desarma si el cerebro se calla,
  tope de 20° de inclinación, acelerador al mínimo en reposo. 5 pruebas.
- ✅ **Percepción desde una cámara real** (`tools/ojo.py`): ya alimenta al
  simulador en vivo, con la misma cantidad física (looming) que el simulador
  usa internamente (7.22) — no una aproximación.
- ❌ **Perfil de física del DBPOWER** (`src/dbpower.rs`): existe pero no está
  conectado al lazo de vuelo, y la masa sigue siendo reconstruida, no pesada.
- ❌ **Cero pruebas contra hardware real** — no hay placa. Falta comprar la FC
  brushed 1S (~$35, ya identificada) y soldarla.

**Correr el informe antes de creerle a nada de acá:**
```bash
cargo run --release -- capacidades
```


### 7.41 El README y el INFORME viejos, movidos acá (2026-09-26)

El README tenía 550 líneas de historia con comandos que ya no existen
(`entrenar`, `volar`, `pid`, `entrenar3d`, `brecha`) y resultados de agosto
medidos con GPS; el INFORME era el resumen de la sesión de auditoría (7.10).
Se movieron enteros acá y el README quedó como puerta de entrada corta.

#### README.md (versión anterior)

# dron

Controlador de vuelo: **red binaria IAFZ-64 por encima de un PID**, entrenada
por evolución. Vive fuera del juego a propósito — se entrenan genomas acá y
después se llevan al juego (o al hardware), no al revés.

##### La decisión de diseño, que es todo el proyecto

**La red nunca toca los motores.**

```
   sensores (96 bits)  ->  IAFZ-64  ->  4 consignas de     ->  Guiado  ->  PID  ->  motores
   termómetro              100 Hz       navegación             actitud     200 Hz
```

El PID mantiene el aparato en el aire y es matemáticamente estable. La red sólo
dice *hacia dónde*. Con ese reparto:

- un genoma malo produce un vuelo torpe, no un choque;
- se puede evolucionar sin romper hélices;
- el salto a hardware real es realista, porque el lazo que garantiza la
  estabilidad no depende de nada aprendido.

Sin ese reparto —la red mandando empuje directo— cada mutación es una salida de
control aleatoria, y eso es el suelo.

##### Uso

```bash
cargo run --release -- pid            # línea de base: sólo PID, sin red
cargo run --release -- entrenar 60    # evoluciona y guarda genomes/DRON_64.adn
cargo run --release -- volar 0        # vuela el campeón y dibuja la trayectoria
cargo run --release -- visor          # visor 3D del dron, en vivo

cargo run --release -- entrenarmision 900   # LA tarea: despegar, entregar, volver
cargo run --release -- capacidades          # qué sabe hacer, en castellano
cargo run --release -- mision 9 0           # vuela la entrega y muestra la traza
cargo run --release -- mision 9 0 detalle   # los 9 casos detrás de la aptitud
cargo run --release -- mision 9 0 salidas   # qué EMITE el cerebro (saturación)
```

##### Resultados medidos (2 ago 2026)

Entrenamiento de 60 generaciones: **5 segundos**.

| | aptitud (peor escenario) |
|---|---|
| Sólo PID | 148 |
| PID + red evolucionada | **405** |

Los tres escenarios terminan con el dron **vivo, en su altura, cerca del
objetivo**: a 0.97 m, 1.37 m y 2.07 m respectivamente.

**El defecto conocido**: pasa de largo. Aprendió a ir hacia el objetivo, no a
frenar en él. Es consecuencia directa de la aptitud, que premia estar cerca
sumado en el tiempo — llegar rápido puntúa más que llegar y quedarse. Se
arregla sumando un término que castigue la velocidad cerca del objetivo.

##### Decisiones que no son obvias

- **La aptitud de un genoma es la de su PEOR escenario, no el promedio.** Con el
  promedio gana el que clava dos mundos y se estrella en el tercero. Con el
  mínimo sólo gana el que vuela en todos, que es lo que hace falta antes de
  subir esto a un aparato de verdad.
- **Caerse es descalificación** (aptitud × 0.1 − 50), no "poco puntaje". Si el
  castigo es suave, la evolución encuentra estrategias suicidas que puntúan.
- **Inclinarse NO es caerse.** Tocar el suelo sigue siendo fatal, pero pasarse
  de 60° abre una ventana de 0.6 s para enderezarse, y volver a vuelo estable
  desde ahí da premio. Antes, pasar 60° mataba en el acto: eso le enseñaba
  cobardía a la red —quedaba plana como un helicóptero de juguete— cuando un
  dron real se inclina fuerte para frenar o esquivar, y aguantar una ráfaga
  fuerte y enderezarse después es justamente lo que se le pide. El castigo por ángulo
  ahora empieza recién fuera de la envolvente cómoda (~31°): antes se castigaba
  TODA inclinación, que en este simulador es castigar trasladarse.
- **La memoria recurrente se borra entre episodios.** Si no, el agente arranca
  el vuelo con el eco del anterior y la evaluación no mide lo que cree medir.
- **El viento es determinista por semilla**, para que la selección premie
  habilidad y no suerte.
- **La red decide a 20 Hz y el PID corrige a 200 Hz.** Que el lazo lento piense
  menos no es ahorro de cómputo: le da al PID tiempo de estabilizar entre
  consignas.

##### El kernel

`csrc/` es el IAFZ-64 del juego, vendorizado con **tres arreglos**:

1. `mutate_iafz_64` no validaba índices: un id fuera de rango hacía `memcpy`
   fuera del arreglo. `earthquake_iafz_64` tenía el mismo agujero.
2. `(uint32_t)(rate * 4294967295.0f)` — ese float redondea a 2³², y convertir un
   float ≥ 2³² a `uint32_t` es comportamiento indefinido.
3. `MAX_AGENTS` de 1000 a 128: la reserva estática pasa de 9.2 MB a 1.2 MB. Un
   dron no es un enjambre, y en un microcontrolador 9.2 MB no entran.

Los arreglos 1 y 2 **también valen para el juego** — es el mismo archivo.

##### 3D con cuatro consignas y ruido de sensores

```bash
cargo run --release -- entrenar3d 200 robusto   # aleatorización de dominio
cargo run --release -- brecha                   # mide la brecha sim-a-real
```

**Cuatro salidas sin cuatro cerebros.** El kernel se extendió con vectores de
pesos de salida adicionales: las dos capas binarias —lo caro— se calculan una
vez y se comparten. Medido: **3.8× más rápido y 3.7× menos memoria** que tener
cuatro cerebros separados (10368 bytes contra 38400).

Las cuatro consignas son **velocidad deseada hacia adelante, velocidad
deseada lateral** (en el marco del cuerpo), velocidad de guiñada y
corrección de altura. La red no pide ángulo: pide velocidad, y es
`Guiado::actitud_desde_velocidad` (`pid3d.rs`) —el mismo controlador que usa
el piloto clásico rival— quien decide cuánto inclinarse para conseguirla.
Antes la red tenía que aprender ella misma cuánto alabeo/cabeceo hacía
falta para moverse; ahora ese trabajo lo hace siempre el mismo controlador,
determinista, y la red sólo navega.

**El controlador nunca ve el estado real**, sólo lecturas con ruido y retardo de
sensores baratos de verdad (MPU-6050, barómetro, GPS). La aptitud, en cambio, se
mide sobre el estado real: importa dónde está el dron, no dónde cree estar.

###### Tres incentivos rotos, encontrados midiendo

La función de aptitud tenía tres defectos que se taparon unos a otros. Cada uno
se encontró midiendo, no razonando, y los tres están documentados en el código
con la medición que los delató.

**1. No había gradiente.** Un A/B de 200 generaciones mostró al genoma clavado
en ~235 desde la generación 20, con el mapeo de ángulo (233.3) y con el de
velocidad (236.4) por igual: la diferencia era del 1.3%, o sea ruido. La causa
era aritmética: parado a 7 m del objetivo cobraba 0.125/paso por no caerse —un
sueldo de ~250 en el vuelo entero—, acercarse un metro le sumaba 0.018/paso
(+36 en total), y estrellarse en el intento le costaba ~272. La evolución hizo
la cuenta correcta y aprendió a flotar quieta en el despegue. El arreglo es
premiar el **progreso** (`Navegacion`): 50 puntos por metro acortado. Es
telescópico, así que ir y volver suma exactamente cero y no se puede farmear.

**2. Girar era gratis.** Con el gradiente arreglado el genoma se movía, pero se
plantaba a ~4 m del objetivo en las cinco rutas. La traza mostró por qué: el
yaw iba de 0 a −6.78 rad en 9 s —más de una vuelta entera— a ~0.8 rad/s
constante. Como las consignas de velocidad son en marco del **cuerpo**, con el
cuerpo rotando un "andá para adelante" barre un círculo en vez de trasladarse.
Nada en la aptitud castigaba la guiñada. Ahora `Actitud` cobra por encima de
0.5 rad/s, y la velocidad real de traslación pasó de ~0.1 a 0.6–2.3 m/s.

**3. El premio de llegar era inalcanzable.** Llegar paga ~4/paso (unos 8000 en
el vuelo), pero estaba detrás de 7 m de desierto sin gradiente que la población
nunca cruzaba. De ahí el **currículo por distancia**
(`entrenar3d N robusto curriculo`): el objetivo arranca al 15% de la distancia
—a un metro, donde ese bono está al alcance de un genoma recién nacido— y se
aleja al 35%, 60% y 100% cuando el campeón deja de mejorar. Con el objetivo
cerca el genoma saca 819.6; es la prueba de que sabe llegar y quedarse cuando
el problema está a su escala.

###### Dónde está parado hoy, sin maquillaje

> ⚠️ **Esta tabla está VIEJA y no es comparable.** Se midió con el código de
> sensores binario, el mapeo de salidas con `tanh`, `ALTURA_MAX` en 1.5 y el
> lazo de altura sin cascada. Los cuatro cambiaron. Se deja porque documenta de
> dónde se venía, no dónde se está; los números de hoy están en el informe de
> capacidades más abajo.

| sensores | genoma | guiado clásico | guiado + filtro |
|---|---|---|---|
| perfectos (irreal) | 380.6 | **7168.9** | 4596.1 |
| ruido real | 484.0 | **7064.2** | 4557.8 |
| peor que la realidad | 456.2 | **6942.3** | 4456.7 |

Los tres arreglos de aquella época llevaron al genoma de 236 a 484 con ruido
real, y la prueba de capacidades pasó de "0 de 5 rutas, parado en el origen" a
volar de verdad —llegaba a 0.74 m del objetivo en la ruta 1 y esquivaba un
poste en la 3—. Pero perdía por un factor de ~15 contra el guiado clásico y
daba **0 de 5** en "va de A a B", porque no frenaba al llegar. Eso es lo que
arregló el trabajo descrito arriba.

> **Aviso.** Este README es en buena parte HISTÓRICO: describe cómo se llegó a
> cada cosa, con los números del momento. **El estado vigente vive en un solo
> lugar, `CLAUDE.md` → «Estado hoy»**, y el detalle de cada sesión en
> `BITACORA.md`. Si algo de acá contradice a esos dos, ganan ellos.
>
> Dos afirmaciones de este archivo quedaron REFUTADAS por medición posterior y
> se dejan por la regla 3.4, no para citarlas:
>
> - **«el orbitar está cerrado»** (sección de abajo): falso. El genoma se
>   estaciona a 0.89 m del punto moviéndose 0.55 m, contra un clásico que queda
>   a 0.01 m moviéndose 0.02. Estaba tapado por un instrumento que medía
>   distancia al objetivo y la reportaba como quietud. Ver bitácora 7.10.
> - **«esquiva mejor que el guiado clásico»**: era el conjunto de
>   ENTRENAMIENTO. Con postes en posiciones nuevas el genoma saca 1/3 y el
>   clásico 2/3 — la red es PEOR que el modelo físico fuera de lo que memorizó.
>   Ver bitácora 7.17.

##### El orbitar: tres causas, ninguna donde se estaba buscando

Durante mucho tiempo el defecto principal del genoma fue que **no frenaba**:
trazaba un arco a 1.46–1.83 m del objetivo en vez de entrar, y daba 0 de 5 en
"va de A a B". Se intentó arreglar cuatro veces tocando el bono de llegada y
las cuatro dieron peor o igual (medido, tabla más abajo). El problema no estaba
ahí. Eran tres cosas distintas, y las tres aparecieron midiendo.

###### 1. El código de sensores hacía que "llegué" no existiera

El kernel IAFZ-64 no multiplica nada. Cada neurona calcula
`popcount(~(contexto ^ pesos)) >= umbral`: dispara cuando el contexto **se
parece** a un patrón guardado, medido en distancia de Hamming. Una neurona es
un detector de vecindad de Hamming y nada más.

Los sensores se codificaban en binario puro. Eso significa que la distancia de
Hamming entre dos lecturas no tiene ninguna relación con la distancia numérica
entre ellas. Medido sobre ±14 m (281 valores × 281, correlación entre |Δvalor|
y distancia de Hamming):

| código | correlación |
|---|---|
| binario, 6 bits | 0.378 |
| termómetro, 6 bits | 0.963 |
| termómetro, 12 bits | **0.990** |

Y el caso concreto — distancia de Hamming desde "estoy encima del objetivo":

| | binario 6 bits | termómetro 12 bits |
|---|---|---|
| a 0.5 m | 5 | **0** |
| a 2.0 m | 5 | **1** |
| a 8.0 m | 4 | **3** |
| a 14.0 m | **1** | **6** |

Con el código viejo, **estar a 14 metros era más parecido a estar encima del
objetivo que estar a medio metro**. No existía ningún patrón de pesos que
significara "estoy cerca", porque "cerca" no era un vecindario de Hamming. El
genoma no elegía orbitar: no tenía forma de notar la diferencia entre "a 1.5 m"
y "llegué".

El arreglo es `codigo.rs`: termómetro (unario) con compresión `|u|^0.6`, que
amontona niveles cerca de cero — con 12 bits sobre ±14 m los cortes quedan en
0.22, 1.39, 3.26, 5.70, 8.67 y 12.1 m. El primero cae justo sobre el ruido del
GPS (0.25 m), así que no se gastan niveles en distinguir ruido. Cuesta caro en
bits (12 bits pasan de 4096 estados a 13) y vale la pena igual: la entropía no
era el cuello de botella, la expresividad sí.

###### 2. La salida de altura estaba con candado

Con el código arreglado el dron llegaba bien en x/y y seguía sin completar.
`dron mision 0 0 salidas` imprime lo que EMITE el cerebro, y ahí estaba:

| salida | media | mín | máx | saturada |
|---|---|---|---|---|
| v adelante | 0.00 | −1.36 | 0.81 | 0% |
| v lateral | −0.06 | −1.68 | 1.69 | 0% |
| guiñada | −1.53 | −3.08 | −0.47 | 0% |
| **altura** | **6.61** | 5.42 | 8.38 | **81%** |

La salida de altura vivía clavada arriba: el dron flotaba a 4.39 m persiguiendo
un punto a 3.0, llegaba perfecto en x/y y la distancia que le faltaba era casi
toda vertical. Dos causas sumadas:

- **`ALTURA_MAX` era 1.5 m, más que la tolerancia de llegada (1.0 m).** Una
  salida pegada al tope alcanzaba, ella sola, para que no pudiera entrar nunca.
  Ahora 0.8 m.
- **`tanh` es un candado.** Con ganancia 0.25 y salida 6.6, la derivada vale
  0.018: mover la salida del cerebro un punto entero mueve la consigna 2 cm.
  Para la evolución eso es terreno plano, y menos todavía cuando la aptitud es
  el peor de nueve casos. Se reemplazó por un squash **algebraico**,
  `u/(1+|u|)`, que está acotado igual pero se aplana como 1/u² en vez de e^−u:
  a 6.6 tiene 1.5× más gradiente, y a 20 tiene 200×. La diferencia entre
  "cuesta salir" y "no se sale".

###### 3. Con un solo objetivo, llegar no abría ninguna puerta

Ésta es la de fondo. Orbitar a 1.5 m cobraba casi lo mismo que entrar, porque
**después de entrar no había nada más que ganar**. El bono de llegada era un
premio suelto, y por eso calibrarlo cuatro veces no cambió nada: la tarea no
pedía frenar.

En `mision.rs` la misión es una **cadena** de puntos, y el punto siguiente sólo
existe si frenaste en el anterior. Las cuentas del peldaño de ida y vuelta (dos
puntos a 7 m):

```
orbitar a 1.5 m del primero:  50 × (7 − 1.5)                      =  275
hacer los dos:                50×7 + 300 + 50×7 + 300 + 600 + 200 = 2100
```

7.6× de diferencia, y no se puede farmear: el progreso es telescópico (ir y
volver suma exactamente cero) y el resto sólo se cobra llegando.

##### Posarse no es estrellarse

Tocar el suelo era muerte instantánea, siempre. Eso hace imposible la tarea
entera: un dron que no puede tocar el suelo no entrega nada, hace acrobacia
indefinida. Ahora `sim3d::Dron3::tocar_suelo` distingue tres estados, y la
diferencia es física, no un premio:

- **posado**: llegó a menos de 1.2 m/s de bajada, sin derrapar (< 1.0 m/s
  horizontal) y derecho (< 20°). El piso lo sostiene y puede volver a despegar.
- **estrellado**: llegó de cualquier otra forma.
- **despegando**: ya estaba apoyado y el empuje superó al peso.

Los números son los de un tren de aterrizaje de dron chico. Y está apagado por
defecto (`posarse_vale`): las pruebas viejas —`escalera`, `brecha`,
la línea de base— no tienen misión de aterrizaje, así que ahí abajo sigue
habiendo sólo suelo y sus números siguen siendo comparables con los de este
README.

El lazo de altura también tuvo que cambiar. Era un PID único sobre el error de
posición, y eso no acota la velocidad: la traza del guiado clásico despegando
del suelo hacia un punto a 3 m **se pasaba hasta 5.45 m** y tardaba más de 10 s
en asentarse, quemando un cuarto del reloj de la misión antes de empezar a
moverse. Ahora es cascada posición → velocidad → empuje, con tope de 3.0 m/s de
subida y 2.5 m/s de bajada — igual que cualquier controlador comercial, que es
justo lo que `Estabilizador3` va a ser reemplazado por. Mismo despegue después
del cambio: **3.07 m en 2 s**, sin sobrepaso.

Los topes **no** están puestos para regalar el aterrizaje: 2.5 m/s de bajada es
más del doble de lo que aguanta el tren, así que el dron sigue teniendo que
frenar solo en los últimos metros. Ponerlos en 1.1 haría que la prueba de
posada se aprobara sola y no midiera nada.

###### Apéndice histórico: los cuatro intentos que no arreglaron el orbitar

> Lo que sigue es el análisis **viejo**, de cuando se creía que el orbitar era
> un problema de calibrar el bono de llegada. Se deja entero porque vale tanto
> como los que salieron bien: cuatro variantes medidas en serio, todas
> descartadas, y eso es exactamente lo que evita que alguien las repita. La
> causa real está tres secciones más arriba y no era ninguna de éstas.

La traza del campeón mostró que el problema **no es de fuerza de frenado** —el
genoma sí reduce velocidad al acercarse—, sino que encuentra un mínimo local
nuevo: orbita en un arco a 1.46-1.83 m del objetivo, girando a 0.43 rad/s
—justo debajo del límite que castiga `Actitud`— en vez de entrar. Rastreando
x/y del dron se ve el arco completo: pasa por un costado del objetivo, cruza
cerca, y sale por el otro lado sin frenar del todo. Es el mismo patrón que el
"foso" viejo, pero en el espacio en vez de en velocidad: el corte dentro de
1 m no da ningún gradiente en 1.0-2.0 m, así que orbitar ahí afuera rinde
exactamente lo mismo que estar parado a 1.99 m.

Cerrar ese refugio se probó con una rampa aditiva en el anillo 1–2 m, medida
**con 4 semillas** (150 generaciones, robusto + currículo) en vez de con una
sola corrida, porque la búsqueda evolutiva es caótica y un cambio chico
reordena toda la trayectoria:

| semilla | escalón (actual) | rampa 1–2 m |
|---|---|---|
| 2026 | **380.6** | 295.6 |
| 7717 | **314.8** | 305.4 |
| 31337 | **334.2** | 292.2 |
| 4242 | 294.5 | **320.8** |
| **media** | **331.0** | 303.5 |

El escalón gana en 3 de 4 semillas. Pero la diferencia de medias (27.5) es
**menor que la dispersión entre semillas de una misma variante** —el escalón
va de 294.5 a 380.6—, así que esto no prueba que el escalón sea mejor: prueba
que la rampa no aporta, que alcanza para no cambiar el código.

La lección que sí quedó: con esta varianza, **cualquier comparación de
funciones de aptitud necesita varias semillas**. Por eso existe
`entrenar3d N robusto curriculo semilla=M`.

##### La escalera de misiones

`entrenarmision` sube diez peldaños, y **cada uno agrega exactamente una
capacidad**:

| # | peldaño | lo nuevo |
|---|---|---|
| 1 | frenar | un punto a 2 m *adelante*: sólo hay que parar encima |
| 2 | rumbo | los mismos 2 m en cualquier dirección, atrás incluida |
| 3 | ir a B 40% | la ruta de verdad, acortada |
| 4 | ir a B 70% | más lejos |
| 5 | ir a B | distancia completa |
| 6 | sensores duros | la misma ruta, ahora con el GPS saltando 0.6 m |
| 7 | esquivar uno | la misma ruta con UN poste en el medio |
| 8 | esquivar | con los que haya: hasta dos por ruta |
| 9 | despegar | arranca posado en el suelo |
| 10 | posarse bajo | ya encima de la plataforma a 1.5 m: sólo tocar suave |
| 11 | posarse | desde 3–4 m y con la plataforma corrida |
| 12 | ida y vuelta | dos puntos: acá orbitar cuesta media misión |
| 13 | entrega | los seis tramos juntos |

La prueba final (`capacidades`, punto 7) usa **rutas de entrega que el genoma
no vio entrenando**: entregar donde ya entrenó no mide generalización, mide
memoria.

**Un peldaño sólo se sube aprobándolo. Nunca por aburrimiento.** Antes había
una "válvula de escape" que promovía al estancarse, y eso fabricó exactamente
el desastre que uno esperaría: el genoma llegó a la misión de entrega sin saber
esquivar dos postes ni posarse, y terminó los trece peldaños con deuda
escondida en cada uno — 0/3 en entregar, 0/3 en posarse. Un entrenamiento
clavado gritando *"no aprendo a esquivar dos postes"* vale más que uno que
termina la escalera fingiendo: el primero te dice dónde está el problema, el
segundo te lo esconde y encima te devuelve un genoma inservible. Saltar sigue
siendo posible con `entrenarmision ... saltar`, pero es una decisión que toma
una persona y queda gritada en el log.

Se sube **por aprobar**, no por aburrirse: el umbral es `LLEGADA` por punto más
el bono de misión cumplida, que es plata que sólo se cobra llegando — el
progreso solo no alcanza para pagarlo. Como la aptitud es el peor de los nueve
casos, pasar ese umbral significa que completó *todas* las misiones del
peldaño en *todos* los niveles de ruido. El estancamiento sigue existiendo como
válvula de escape (45 generaciones) para no quedarse clavado para siempre.

Las dos primeras versiones de esta escalera tenían 4 y 7 peldaños y las dos se
trabaron, cada una por una razón distinta que la escalera de ahora arregla:

- con 4 peldaños saltaba de "2 m sin postes" a "6 m con postes" de un tirón, y
  el currículo promovía tres peldaños seguidos con el cartel de *estancado* sin
  haber aprobado ninguno;
- con 7, el peldaño 1 mezclaba frenar con girar — dos de sus tres misiones
  daban ~1000 puntos y la tercera, la única con el objetivo detrás del dron,
  daba −7.4 **con sensores perfectos**. Como la aptitud es el mínimo, las dos
  buenas no movían el número ni una décima.

Y lo mismo pasó una tercera vez con los postes: el campeón **no chocaba**
—pasaba a 1.49 m— pero se quedaba a 3.3 m del objetivo en las dos rutas de DOS
postes, mientras que la de un poste la dejaba a 1.39 m, casi resuelta. Rodear
un obstáculo y encadenar dos rodeos son dos cosas distintas, así que son dos
escalones.

Las tres veces el diagnóstico salió de `mision N 0 detalle`, que imprime los
nueve números en vez del mínimo. Es la herramienta que faltaba: un campeón
clavado en el mismo valor durante cuarenta generaciones casi nunca está
estancado en todo, está estancado en **uno**.

###### La vara de "llegó" también estaba mal calibrada

Dos veces el campeón terminaba **adentro del radio** —a 0.47 m y a 0.86 m de un
punto con radio 1 m— y marcaba cero tramos. No es que no llegara: era la
condición de llegada.

- Cualquier paso por encima del umbral de velocidad volvía el contador de
  "quieto" a cero, y con viento y ruido sostener medio segundo sin una sola
  ráfaga es casi imposible. Ahora lo que reinicia la cuenta es **salirse del
  radio**, no un pico de velocidad.
- El umbral era 0.6 m/s, y eso choca con la resolución que tiene la red cerca
  de cero: las salidas horizontales crudas viven en ±1.7, así que una salida
  residual de 0.3 —ruido para la evolución— ya pide 0.52 m/s, y con las dos
  apenas fuera de cero el módulo da 0.74. El dron no calificaba como quieto por
  estar parado. Ahora 1.0 m/s, que sigue siendo la cuarta parte del crucero.

El efecto medido de esa sola constante: el peldaño 1 pasó de aprobarse en la
generación 47 a aprobarse en la **4**.

###### Y el ruido tuvo que entrar a la escalera, no estar en todos los escalones

La aleatorización de dominio —evaluar en el peor de tres niveles de ruido— es
lo que hace que el número del simulador signifique algo afuera, y para
**evaluar** no se negocia. Pero tenerla desde el primer peldaño tiene un costo
que se puede medir, y se midió. El campeón trabado en «ir a B», los nueve
casos:

| misión | limpio | real | duro |
|---|---|---|---|
| 1 | 1001.2 | 1067.6 | 1038.7 |
| 2 | 1252.5 | 1253.6 | **100.2** |
| 3 | 1075.7 | 1082.2 | **97.9** |

Cumplía las tres misiones con sensores limpios y reales —mil y pico de
puntos— y se caía a 100 con sensores duros. Como la aptitud es el peor de los
nueve, **el mínimo lo definía siempre el ruido y nunca la capacidad que el
peldaño quería enseñar**: navegar, esquivar y posarse quedaban invisibles
detrás de un problema de filtrado.

Así que aguantar un GPS que salta 0.6 m con medio segundo de retardo pasó a ser
lo que ya era —una capacidad más— y tiene su propio escalón, el 6. Abajo de él
se entrena con sensores limpios y reales; de ahí para arriba, con todo el rango
y para siempre. **El informe de capacidades y la tabla de brecha no cambian**:
siguen exigiendo el rango entero.

##### Una línea de investigación cerrada: el conectoma real de la mosca

Hubo, durante un tramo de este proyecto, un subcircuito sacado del conectoma
**real** de *Drosophila* (FlyWire/Princeton) atornillado por debajo del
IAFZ-64 — primero el reflejo óptico (HS/VS), después la brújula de rumbo del
Complejo Central. La pregunta era simple: el cerebro de una mosca real,
¿ayuda a navegar mejor que no ponerle nada?

Se midió a fondo, con el instrumento arreglado (la fórmula híbrida de hoy, no
la vieja que reemplazaba al piloto en vez de corregirlo) y con tres semillas
distintas para no confundir suerte de una corrida con una conclusión real. El
resultado, sin adornar: **ni el reflejo óptico ni la brújula de rumbo le
ganan de forma confiable a no ponerles nada** — la diferencia contra la línea
de base "sin circuito" quedó adentro del ruido de semilla a semilla en los
dos casos. Los números completos están en `CLAUDE.md`.

El código (`bio.rs`, `gymbio.rs`, los scripts de extracción del conectoma) se
borró del repo con esa medición en la mano — no por descuido, es la regla del
proyecto: si algo no aporta, se saca, con el número que lo dice.

##### Dónde quedó, con el número real (no proyectado)

> ⚠️ **Estos números salen de un genoma viciado y hay que rehacerlos.** Se
> entrenó con el currículo viejo, el que saltaba de peldaño por estancamiento,
> así que llegó a "esquivar dos postes", "posarse" y "entrega" **sin haber
> aprobado ninguno**. Los ceros de abajo son, en buena parte, las fases que se
> salteó. Con la regla nueva (sólo se sube aprobando) hay que reentrenar desde
> cero y ver hasta dónde llega de verdad.

El genoma instalado en `genomes/DRON3D_64.adn` es el primero entrenado con todo
lo de arriba —termómetro, salidas sin candado, misiones encadenadas, posarse,
campo de visión ancho para esquivar— puesto a la vez. El entrenamiento se cortó
a mitad de camino (680 de 1200 generaciones) y llegó al último peldaño recién
en la generación 350. `cargo run --release -- capacidades`, con rutas que el
genoma no vio entrenando:

> **Tabla histórica — no es el estado de hoy.** Corresponde a un genoma viejo.
> El estado vigente vive en UN solo lugar, `CLAUDE.md` → «Estado hoy», y se
> re-mide con `cargo run --release -- capacidades`. Se dejaron de mantener
> copias de esta tabla porque llegaron a ser cuatro, con números distintos y
> sin marcar cuál valía.

| capacidad | genoma | clásico |
|---|---|---|
| despegar | 3/3 | 3/3 |
| ir de A a B y frenar | 1/5 | 5/5 |
| quedarse quieto | 1/3 | — |
| posarse sin romperse | 0/3 | 1/3 |
| recuperar de un vuelco | 1/3 | — |
| esquivar obstáculos | **1/3** | **0/3** |
| entregar un paquete | 0/3 | 1/3 |

Es la diferencia entre "no vuela nada" —el genoma de antes de hoy, entrenado
con el código viejo y por lo tanto puro ruido bajo la codificación nueva— y
"despega siempre y resuelve una ruta entera". (El "esquiva mejor que el guiado
clásico" que decía acá está REFUTADO: se medía sobre los mismos postes del
entrenamiento — ver el aviso del principio.) No es la tarea terminada: la entrega de punta a punta todavía no
sale, y posarse tampoco (el campeón queda flotando cerca del suelo sin llegar
a tocarlo). Ambas cosas son las que menos generaciones tuvieron.

Lo que sigue no es una incógnita, es una cola de trabajo: más generaciones en
los peldaños de posarse y entrega, y confirmar si 602.5 de aptitud en el
peldaño de entrega es un mínimo local o sólo falta de tiempo (una segunda
corrida con otra semilla estaba en curso al cerrar esta sesión).

##### Lo que falta para hardware real

1. **96 bits de sensores es el techo duro**, y en termómetro pesan como 96
   niveles, no como 2⁹⁶ estados. Se codifica en marco del cuerpo para que la
   guiñada quede implícita y no gaste bits. Entran: dónde está el objetivo
   (31 bits), altura sobre el suelo (9), cómo me muevo (22), actitud (15) y
   visión (19: rumbo del estorbo, cuánto aprieta, y el loom). No entran
   píxeles y no van a entrar.
2. Masa e inercia del aparato real en `sim3d.rs` — tres números que se miden con
   una balanza y un péndulo.
3. Modelo de motor: empuje no lineal y retardo de los ESC.
4. Puente MSP o MAVLink: el controlador comercial ya trae el PID de actitud,
   así que `Estabilizador3` **se borra** y su lugar lo ocupa el hardware. Lo
   que no se borra es `Guiado`: un controlador comercial no acepta "velocidad
   deseada", así que esa conversión velocidad→actitud viaja con la red a la
   computadora acompañante, que le manda al hardware las consignas de actitud
   de siempre (alabeo, cabeceo, guiñada, altura).
5. Piloto de seguridad con el radio en la mano y modo de recuperación.


#### INFORME.md (versión anterior)

# Informe de sesión — proyecto dron

Contexto para quien lo lea sin haber estado: dron simulado en Rust, pilotado por
una red binaria evolucionada (IAFZ-64) que **corrige** a un piloto clásico
basado en modelo físico (no lo reemplaza). Objetivo final: hardware real.

---

##### 1. Auditoría de reglas

`CLAUDE.md` había crecido a 2009 líneas, 62% historia, y se contradecía sola
(cuatro tablas de resultados sin marcar cuál valía; una conclusión refutada
escrita en negrita cien líneas después de su refutación).

Separado en **`CLAUDE.md`** (contrato, ~900 líneas) y **`BITACORA.md`**
(registro por sesión). Nada se borró: lo refutado quedó marcado como refutado.

Reglas: se ampliaron 3.2, 3.7, 3.12 y 3.14; se retiró la 3.10 (su objeto de
estudio ya no existe en el repo); se agregó la **3.18 — el instrumento de
medición también se audita**, que resultó ser la más productiva de la sesión.

---

##### 2. Cinco instrumentos rotos encontrados

Todos daban números que parecían buenos. Ese es el modo de falla peligroso.

| Instrumento | Qué decía | Qué pasaba |
|---|---|---|
| `capacidades` cap. 3 | «quedarse quieto 3/3» | Medía **distancia al objetivo**, no quietud. Real: **0/3** (clásico 3/3) |
| `capacidades` cap. 6 | «esquivar 3/3 vs clásico 0/3» | Usaba **los postes del entrenamiento**. Con postes nuevos: **1/3 vs clásico 2/3** |
| `capacidades` CLI | — | Ignoraba `genoma=` **en silencio**; medía siempre el genoma instalado |
| `costo_de_computo` | fallaba | Reprobaba por carga de la máquina (21.7 µs vs 5.8 en reposo), no por el código |
| Grafo del visor | — | Dibujaba **1 de 4 haces**: parecía que los sensores no entraban a la red |

El de esquivar es el grave: **echó abajo la afirmación más fuerte del proyecto**
(«la red hace algo que la vara honesta no puede»). Fuera de lo que memorizó, la
red es *peor* que el modelo físico. Lo detectó el usuario mirando el visor, no
la batería de pruebas.

---

##### 3. Bugs de comportamiento arreglados

**El dron no aterrizaba nunca.** El lazo de altura es proporcional (ganancia
1.6), así que con consigna 0.0 la velocidad de descenso es −1.6·z: se apaga
junto con la altura. Exponencial → nunca cruza el cero → el flag `apoyado` nunca
se prendía. El dron flotaba a milímetros del piso quemando el reloj.

*La señal que lo delató*: aprobaba con sensores **sucios** y reprobaba con
**limpios** (con ruido, el ruido lo empujaba abajo). Eso es imposible si la
dificultad fuera real: firma de bug.

Arreglo estilo ArduPilot `LAND_SPEED`: consigna 0.25 m bajo tierra, en rampa
desde 0.5 m. Resultado (piloto clásico): peldaño 12 de **88.3 → 1163.5**.

**La red estrellaba la posada que el modelo ya hacía perfecta.** Mismo campeón:
peldaño fácil 1076.8 ✓, peldaño difícil **−47.9 (se estrella)**, y con la red en
**cero** (= el clásico exacto) **1163.5 ✓**. La causa era el tamaño de la
palanca: 0.80 rad (46°) y 2.5 m de autoridad. A medio metro del piso equivocarse
no es puntuar peor, es volcar. Arreglo: `autoridad_de_posada` desvanece la
autoridad con la altura. Sin reentrenar: peldaño 12 **−47.9 → 967.3**, y en
`capacidades` **posarse 2/3 → 3/3** y **entregar 1/3 → 2/3**.

**El visor no reseteaba la memoria recurrente entre vuelos.** Todo camino de
entrenamiento la resetea; el visor era el único que no. Cada entrega repetida
arrancaba desde un contexto que la red nunca vio entrenando — el usuario lo
reportó como «con el tiempo se vuelve más tonto».

---

##### 4. Los dos cambios estructurales

**Separar con qué se ORDENA de con qué se APRUEBA.** La aptitud es `min` sobre
todos los términos, y un `min` **no tiene gradiente salvo en el peor término**.
El mínimo lo definía el examen de repaso (a su vez un `min` sobre 12 peldaños
olvidados): una mutación que recuperaba **11 de 12 no movía el número**, así que
la selección no la veía. Ahora:

```
APROBAR   min >= umbral                  ← el peor caso entero, SIN CAMBIOS
ORDENAR   min + 0.15 × (media − min)     ← monótona en TODOS los términos
```

La vara no se movió; se arregló el optimizador, que estaba ciego.

**El canal de elevación.** No había ningún bit que dijera si un poste se puede
saltar por arriba — la física lo permite (`Obstaculo::distancia` da infinito si
el dron va más alto) pero era **invisible para el piloto**. Se cambió
`divergencia` (3 bits, redundante: es exactamente `vz/altura` y ambas ya viajan
por separado con más resolución) por `obs_altura` (cuánto sobresale el estorbo
por encima del dron). Presupuesto de 96 bits intacto.

---

##### 5. Percepción real desde cámara

`tools/ojo.py`: captura + flujo óptico denso (Farnebäck) → los tres campos
visuales que el kernel consume. Es el primer pedazo de la cadena de hardware que
existe.

**Validado contra verdad de campo** antes de creerle (secuencia sintética de
acercamiento con `v` y `d` conocidos):

```
divergencia esperada  0.347      medida  0.339     (2.3% de error)
```

Esa validación cazó **dos errores de escala propios**: un factor 2 (la
divergencia 2D es el doble de la tasa de expansión) y `obs_proximidad` inflado
9× por sumar columnas en vez de promediarlas.

**La cámara de la máquina es una Intel RealSense F200** (2014). Primero midió
negro absoluto — resultó ser **cinta sobre la lente**, no hardware muerto. Ya
destapada: funciona, 17 fps a 160 px, y los tres campos se excitan (10.6%, 35.1%,
28.3% de sus rangos). El nodo de **profundidad** (`/dev/video2`, Z16) sigue sin
entregar imagen: ése sí necesita librealsense 1.x (la 2.x le quitó el soporte).

---

##### 6. Estado del entrenamiento

Con todos los arreglos puestos, la corrida actual **llegó por primera vez al
peldaño 14/14 — la entrega completa** — en 17 generaciones. Las corridas
anteriores se trababan en el 12 o el 13.

Ahora trabada ahí (57.2 de 2400), pero el diagnóstico es distinto y mejor:

```
el mínimo lo define la misión 1 con ruido limpio (4155.0 de 2400)
  termina: MISIÓN CUMPLIDA — 6/6 tramos, queda a 0.29 m
  examen de repaso: peor caso «ida y vuelta» escalado a 57.2  ← el mínimo real
  DESAPRENDIÓ los peldaños [13]
```

O sea: **ya completa la entrega de punta a punta** (6/6 tramos, 4155 sobre un
umbral de 2400). Lo que la traba es que olvidó el peldaño 13, y el olvido bajó
de 12 peldaños a **1**.

---

##### 6b. Módulo de cámara — FUNCIONA

`tools/ojo.py` quedó operativo de punta a punta:

- **En vivo**: `--visor http://localhost:8080` manda lo que ve la cámara al dron,
  y la percepción de obstáculos deja de salir del simulador. Verificado con el
  dron volando (`ojoManda: True`, rumbo cambiando con lo que pasa frente a la
  lente). Se apaga sola si la cámara se calla 0.5 s.
- **Grabación**: `--grabar PREFIJO` → video sin pérdida + csv de los tres campos.
  `--video` repite el experimento exacto.
- Sólo se pisan 2 de los 3 campos: `obs_altura` (elevación) no sale de una
  cámara monocular y se dejó viniendo del simulador, marcado como deuda.

Hubo que arreglar la grabación: con mp4 la reproducción daba 3.6% contra 14.0%
de la captura (compresión con pérdida borra el grano que el flujo necesita, y la
cadencia variable escala mal una tasa por segundo). Con FFV1 + tiempos reales
del csv, captura y reproducción coinciden.

##### 7. Qué queda pendiente

1. **El orbitar** (deuda vieja, ahora medida de frente): el genoma se estaciona
   a 0.89 m del punto moviéndose 0.55 m; el clásico queda a 0.01 m moviéndose
   0.02. Criterio de llegada con tolerancia fija = imán.
2. **Generalización de esquive**: 1/3 con postes nuevos. El canal de elevación
   es el primer paso; el siguiente candidato es el **anillo de proximidad** (N
   sectores en vez de un solo rumbo promediado), que sí cuesta bits.
3. **Profundidad de la RealSense**: librealsense 1.x con backend RSUSB. Si
   anduviera sería mejor que el flujo óptico — daría `obs_proximidad` de
   distancia métrica real, y sobre todo daría `obs_altura`, que con cámara
   monocular no se puede.
4. **Hardware**: puente MAVLink/MSP, perfil de física del DBPOWER (faltan
   wheelbase y empuje), piloto de seguridad. Todo en 0%.

##### Nota importante

El cambio de sensores **invalidó todos los genomas anteriores**. El genoma
instalado (`genomes/DRON3D_64.adn`) es ilegible para el código actual; cualquier
número de `capacidades` sobre él no significa nada hasta que termine el
reentrenamiento en curso.

### 7.42 La baliza: sin GPS, el objetivo se VE

Ítem 1 del contrato. `sim3d::Baliza`: el punto está marcado; cuando la cámara
lo ve (cono frontal a menos de 20 m, o casi debajo —la cámara del flujo mira
al piso—) el dron mide rumbo y distancia (5% de error) y lo ubica en SU marco
odométrico: posición estimada + lo visto. Si no lo ve, vuela con la última
estimación (o la del mapa si nunca lo vio). El juez (`Seguidor`) sigue usando
la verdad.

**Donde importa, corrige:** 40 m ida y vuelta, ruido DURO, clásico, 12
rumbos (prueba `la_baliza_corrige_la_deriva_en_rutas_largas`):

```text
  sin baliza   9/24 tramos   termina a 2.46 m
  con baliza  24/24 tramos   termina a 0.45 m
```

**En el gym no mueve la vara, y hay que decirlo:** las misiones son de 5-9 m
y la deriva queda adentro del radio de llegada. Con ganancia 0.2 la vara bajó
un poco (ruido de medición mayor que la deriva: p13 16→12); con 0.02 y 0.005
queda igual dentro del ruido de 24 sorteos. Se toma 0.02. **Consecuencia para
el currículo: no hay ningún peldaño que ejercite la deriva.** Queda escrito
como deuda del ítem 2.

**Visor y `capacidades` ahora vuelan lo mismo que el gym** (regla 3.3):
baliza, moto vista relativa (`visto_desde`, también para la red en la
cabina) y, bug aparte encontrado acá, **el clásico del visor aterrizaba con
`alt_sp = oz`** en vez de `altura_de_aproximacion` (la bajada en dos
velocidades y la alineación del gym). Verificado en el visor: entrega
completa 6/6 tramos en 48.3 s, sin GPS.

### 7.43 Calibrando la entrega: cuatro defectos de la vara y dos del generador

Ítem 2 del contrato. Todo medido con `dron sorteos N clasico n=48` y con un
clasificador de fallas (216 vuelos por peldaño: 24 sorteos × 3 misiones × 3
ruidos). Sin GPS, con baliza.

**Punto de partida:** p8 44, p9 41, p12 35, p13 33, p14 (entrega) 15, p15 4.

1. **El generador armaba PAREDES.** Con márgenes de 3 m en rutas de 7 m, los
   dos postes caían en una franja de 1 m, a 0.76 m uno del otro sobre la
   ruta: una pared con una rendija de 0.25 m. No es «encadenar dos rodeos».
   Separación mínima de 2.5 m a lo largo de la ruta; si no entra, un poste.
   Entrega, vuelos cumplidos: limpio 49→57, real 36→59, duro 39→49 de 72.
2. **El lado del rodeo se medía contra el CENTRO del poste.** Un sector a
   0.06 rad del eje del poste contaba como «del lado correcto» y el clásico
   apuntaba derecho a él. `MARGEN_LADO` = 0.5 rad (barrido 0 / 0.25 / 0.4 /
   0.6 → p14 15 / 29 / 36 / 37 de 48; meseta). **p8 y p9 quedan en 48/48.**
3. **Obstáculos inflados** (el sector hereda la mitad de la amenaza de sus
   vecinos): 48 sorteos, p8 43→44, p9 41→43, p13 29→30, p14 7→10. Chico
   pero en los cuatro, y con razón física: se queda.
4. **`dron sorteos` exigía los tres ruidos en todos los peldaños**, también
   en los que el entrenador aprueba con limpio y real. Bug de instrumento
   (regla 3.18) arreglado; en la práctica no cambió los números, porque lo
   que ataba la nota de esos peldaños era el examen de repaso.
5. **La entrega usaba tres rutas FIJAS** —(6,0), (−5,5), (4,−7)— en el
   peldaño final: la misma memoria de coordenadas que 7.28 midió como causa
   del olvido. Ahora sale de `rutas()`, como el resto.
6. **Peldaño nuevo, «ruta larga»** (el 14; la escalera pasa a 16): ida y
   vuelta de 20-36 m sin postes, el único que ejercita la deriva sin GPS.

**Probado y descartado, con los números (no reabrir sin evidencia nueva):**

- *Sostener el rumbo y quitar el tope por mirada cerca del punto*, para que
  el clásico no orbite sobre la plataforma: p7 48→46, p9 48→44, p12 42→35.
  Sólo al posarse: p12 42→39, p13-15 38→34 (sube el 16, 18→23).
- *Fusión vertical* (barómetro + modelo del empuje, como un EKF): bajaba los
  aterrizajes duros con ruido DURO (16→12 de 72) pero con la masa nominal en
  el modelo **hundía a un dron cargado: caja de 300 g al posar, 24/24 → 0/24**
  (`dron estres 11`). Ni un estado de sesgo («empuje de hover» de PX4) ni
  más peso a la medición lo recuperaron (12/24 lo mejor). Se sacó: la caja es
  la tarea 3 del producto. Lo cazó `dron estres`, no las pruebas.

**Estado final, 48 sorteos:**

```text
  peldaño   1-11   12   13   14 larga   15 entrega   16 entrega DURO
  aprueba    48    39   36      36          36              16
```

y `dron estres`: el peldaño 7 aguanta todas las averías; al posar sólo cae
«40% menos empuje» (0/24); en la entrega, 40% menos empuje 10/24 y el resto
23-24/24.

**Lo que ata 12-15 es el examen de repaso**, y dentro de él el peldaño 12
(posarse en la plataforma corrida): el clásico orbita a 0.3 m del piso,
radio 12 cm a 0.65 m/s, con la nariz girando, y el freno de aproximación no
lo deja tocar. Los dos intentos de arriba lo atacaron y fallaron: sigue
abierto. **El 16 (entrega con ruido DURO) no está calibrado** (16/48): los
aterrizajes con ruido duro.

### 7.44 La órbita sobre la plataforma era la baliza; el 16 se va; el repaso gana piso

**La órbita del clásico sobre la plataforma corrida (peldaño 12) la causaba
la baliza.** Con baliza 39/48, sin baliza 46/48. Su error era chico (±1.5 cm,
menos que el del mapa) pero medido a 200 Hz desde encima de la plataforma
hacía temblar el punto, y eso sostenía la órbita. Congelar la estimación a
menos de 1.5 m (volar con lo último que vio): p12 45, p13-14 41, p15 40 de
48. Bajar la ganancia a 0.005 en vez de congelar: p12 42. La ruta larga
sigue en 24/24.

**Peldaños 12-15 calibrados** (≥ 83% en 48 sorteos). La vara, final:

```text
  peldaño   1-11   12   13   14 larga   15 entrega
  aprueba    48    45   41      41          40
```

**La entrega con ruido DURO (era el 16) salió de la escalera.** La vara la
aprobaba en 22/48 por los aterrizajes, y tres arreglos medidos no sirvieron:
fusión vertical con modelo (7.43: hunde al dron cargado), el mismo con
estado de sesgo (12/24 con la caja, lo mejor) y pasa-bajos sin modelo
(0.1/0.2: caja 0/24, el 16 no mejora). DURO es «peor que la realidad a
propósito» y la posada corrida ya lo excluía por la misma razón (7.29).
**Aterrizar con ruido DURO queda abierto** — es el primer candidato a medir
el día que haya placa: si el barómetro real es como REAL, no hace falta.

**El examen de repaso de las misiones tiene piso contra el clásico** (ítem 5,
regla 3.8): en cada misión vieja la red tiene que conservar el 95% de lo que
saca el clásico en ESA misión (`PISO_CLASICO_REPASO`), no sólo el umbral. El
clásico se vuela una vez por escenario (caché). La vara no cambia: el clásico
pasa su propio repaso (prueba `el_clasico_pasa_su_propio_repaso`).

### 7.45 La red se calla al llegar, y la tarea se DICE

**Ítem 4 del contrato, el orbitar.** `gym3d::autoridad_de_sosten`: cerca del
punto en vuelo horizontal (y sin estorbos a la vista) la corrección de la
red se achica, igual que `autoridad_de_posada` ya hacía cerca del suelo.
Con el genoma instalado, `capacidades`, cuánto se mueve en los últimos 3 s:

```text
                      ruta 1   ruta 2   ruta 3
  sin sostén           0.46     0.34     0.23 m
  1.5 m / piso 0.2     0.05     0.13     0.10
  3.0 m / piso 0.1     0.05     0.04     0.06     ← se toma
  clásico              0.04     0.01     0.05
```

El resto de `capacidades` igual. **Un solo genoma, entrenado con GPS: es
indicio**; lo confirma o refuta el reentrenamiento.

**Ítem 6: la tarea se recibe en runtime** (`mision::tarea_desde_texto`,
`dron tarea "..."`, `/tarea?t=` y la caja «decirle una tarea» del visor):
`despegar Z · ir X Y [Z] · posar [X Y] · agarrar [kg] · soltar`. Agarrar y
soltar cambian la masa en vuelo (el imán), con la misma cuenta en el gym y
en el visor (`carga_tras`). **Una tarea que nadie entrenó** —buscar una caja
de 0.3 kg, llevarla a otro punto, soltarla, volver—, con dos postes y sin
GPS: clásico 9/9 tramos en 26.6 s, genoma instalado 9/9 en 31.2 s, y en el
visor 9/9 en 29.2 s. Una orden mal dicha se rechaza con el motivo. Pruebas:
`tareas_dichas`.

**Reentrenamiento (ítem 3) relanzado con todo lo de hoy**:
`genomes/EXP_20260926_final/`, dos semillas, 1500 generaciones, sin GPS,
baliza, vara calibrada, escalera de 15 con ruta larga, piso contra el
clásico en el repaso y autoridad de sostén. La corrida anterior
(`EXP_20260925d_singps`, sin nada de eso) se cerró: s1 llegó al peldaño 11
(gen 360), s2 al 13 (gen 242) — números de otra escalera, no comparables.

### 7.46 El dron recuerda lo que vio: mapa, ruta y el orden de las cajas

Pedido del usuario: *«¿cuántos obstáculos puede esquivar a la vez?, ¿guarda
el mapa?, si le pongo cinco cajas, que elija la más cercana, saque rutas,
entregue, vuelva…»*.

**Cómo era:** puramente reactivo. Ve lo que tiene adelante (cono de 160°,
cinco sectores) y esquiva en el momento; no recordaba nada (la memoria de la
red son 32 bits recurrentes, no un mapa).

**`plan.rs`, la capa de arriba del piloto** (sólo en tareas dichas; el gym
sigue midiendo maniobras con la vara reactiva, regla 3.2):

- **Mapa**: `Camara::medir_postes` da rumbo, distancia (±15%) y radio (±20%)
  de cada poste visto —supuestos a medir con la placa—; el mapa los fusiona
  con pesos 1/σ² y la incertidumbre CRECE con el tiempo (0.05 m/s: la deriva
  sin GPS mueve lo viejo). Sin eso: 15 postes en el mapa para 4 reales en el
  visor, 12 para 6 con ruido real; con eso, 4/4 y 6/6.
- **Ruta**: grafo de visibilidad + Dijkstra sobre lo mapeado, recalculada a
  10 Hz; el piloto vuela al próximo vértice. **Margen por incertidumbre**
  (`K_SIGMA` = 5σ): sin él se metía por huecos que en el mapa pasaban y en
  la realidad no.
- **Orden de entregas**: `caja X Y a X Y [kg]` (varias) y `volver`; el dron
  prueba todos los órdenes hasta 7 cajas y elige el recorrido más corto —por
  RUTA sobre el mapa conocido, en el visor—. Prueba: una caja cerca detrás de
  una pared de postes deja de ser «la más cercana».

**Cuántos postes a la vez** (ida y vuelta de 16 m, clásico, 36 sorteos):

```text
  postes          2    4    6    8   12
  sólo reactivo  36   32   21   14    6
  mapa + ruta    36   34   33   27   24     (K = 6; con 4.5: 35 34 30 30 26)
```

**Cinco cajas entre seis postes, ruido real**: con mapa 33/33 tramos en
93 s, sólo reactivo 28/33. Visor: tres cajas y vuelta, 21/21 en 64.5 s,
mapa 4 de 4 (error 0.7-1.2 m contra la verdad = la deriva de la odometría
en 65 s; la ruta vive en el mismo marco y por eso sale bien).

**Bug del visor encontrado de paso:** al reposicionar el dron no se
reiniciaban la odometría ni la baliza, así que el dron creía estar donde
estaba antes (mapa corrido ~5 m). Arreglado.

**Lo que NO anda:** el genoma instalado (el viejo, entrenado con GPS) choca
en la tarea de cinco cajas con y sin mapa —el problema es el genoma—; con
ruido DURO fallan los aterrizajes (abierto desde 7.44).

### 7.47 El piso contra el clásico de 7.44 decidía con una muestra

El entrenamiento `EXP_20260926_final` se trabó 700 generaciones en el
peldaño 13. El campeón tenía medianas de 1296-1467 en todos los peldaños
viejos (umbral 1200) y reprobaba 12 de 12 sorteos: el piso comparaba MISIÓN
por MISIÓN contra el 95% del clásico, ~100 comparaciones por sorteo, y el
ruido de una sola reprobaba todo. Regla 3.20 otra vez. Ahora compara el
promedio por PELDAÑO contra el promedio del clásico. El mismo campeón pasa
de 0/24 a **17/24** (clásico 20/24). La corrida se cerró (s1 en el 13 desde
la gen 87, s2 desde la 47) y se relanzó sembrada desde ese campeón:
`genomes/EXP_20260926b/`.

### 7.48 SLAM con anclas: la referencia es donde se prendió

Pedido del usuario: nada de marcas impresas ni sensores nuevos; que el punto
donde se prendió sea la referencia, como la Tierra para una nave, y que el
mapa se arme desde ahí. Es SLAM con referencias naturales.

**Lo que quedó** (`plan::Mapa::ubicar`, `pose_por_anclas`,
`Sensores::corregir`/`corregir_rumbo`): un poste anotado cuando el dron
estaba seguro de dónde estaba —duda BRUTA, sin correcciones, menor a 0.3 m:
cerca de casa, al arrancar— queda de **ancla**, fijo. Al volver a verlo:
con dos o más anclas a la vista, posición y rumbo juntos por triangulación
(Kabsch 2D, compuertas de plausibilidad); con una, sólo posición (Kalman con
compuerta de 3σ). La baliza se mueve con las correcciones.

**Medición** (cuadrado de 25 × 20 m dos veces, ~180 m, 10 postes —4 cerca
de casa—, clásico, 24 sorteos; «error propio» = cuánto se equivoca el dron
sobre dónde está al final):

```text
                        ruido real                  ruido DURO
                    mediana  peor  cumplidas    mediana  peor  cumplidas
  sin SLAM           8.2     9.5    24/24        8.4    10.6   17/24
  con anclas         3.8     7.9    24/24        2.8    28.8   15/24
```

**Los intentos que fallaron, en el orden en que se descartaron:**

1. Corregir con cualquier poste conocido (no sólo anclas): el mapa se
   arrastraba con el dron; mediana 6-7 m y colas de 26 m.
2. Corregir el rumbo con UN poste: un error de posición se confunde con uno
   de rumbo; colas de 30 m. Con un poste sólo se corrige posición.
3. Anclas decididas con la duda YA corregida: el dron se creía seguro y
   convertía en ancla lo anotado con deriva real. Se usa la duda bruta.
4. La baliza no se movía con las correcciones: se posaba a 1.2-1.9 m de la
   base (2 misiones perdidas de 24).
5. Triangulación sin compuertas: cuatro anclas casi simétricas alrededor de
   casa encajan también giradas 90°. Compuerta de giro que escala con la
   duda de rumbo, con techo de 1 rad.

**Lo que NO está resuelto:** con ruido DURO queda una cola de 28 m, un
vuelo donde casi nunca se ven dos anclas juntas y el rumbo deriva sin
corrección (0.009 rad/s = el supuesto `SESGO_GUINADA`). **Sin brújula, ese
número manda todo, y es un supuesto**: es lo primero a medir con la placa
(dron quieto 5 minutos registrando el rumbo). Un giróscopo bien calibrado
suele derivar bastante menos.

### 7.49 Aterrizar con ruido DURO sin sensores nuevos: divergencia constante

El usuario descartó agregar un láser de distancia. Lo que queda sin
hardware nuevo es lo que hacen las abejas y los microdrones de
investigación: la cámara que mira al piso mide cómo se agranda el suelo,
`D = vz/h`, con mucho menos error que el barómetro cerca del piso (±0.04/s
contra ±0.25 m con ruido DURO). Bajando con `D` constante la velocidad cae
sola en proporción a la altura, sin conocer la altura; un piso de 0.25 m/s
asegura el toque. `gym3d::BajadaPorDivergencia`, en la bajada final de toda
posada, igual para la vara y la red (regla 3.15), y en el gym, el visor y
`capacidades` (regla 3.3). La velocidad se pide en términos de la MISMA
lectura de altura que usa el estabilizador, así el ruido del barómetro se
cancela.

Ruido DURO, vuelos cumplidos de 48 (16 sorteos × 3 misiones):

```text
                      posada corrida (12)   entrega (15)
  sin divergencia          28                  36   (11 toques duros)
  D* = −0.4                29                  41
  D* = −0.6                31                  40   (6 toques duros)
  D* = −0.9                25                  37
```

Se toma −0.5 (meseta). Con sensores limpios y reales la vara no cambia
(1-11: 48/48; 12: 45; 13-14: 41; 15: 40) y la caja de 300 g sigue en 24/24.
**No alcanza para devolver la entrega con ruido DURO a la escalera**: ~85%
por vuelo es ~60% por sorteo (tres misiones), debajo del 83%. Lo que queda
en la posada corrida con DURO es «sin reloj»: el freno de aproximación no
lo deja bajar mientras se desliza, y con velocidad de flujo tan sucia se
desliza de verdad.

`capacidades`, una sola misión: la entrega 1 del clásico pasó de 6/6 a 4/6
con la bajada nueva y la 3 del genoma de 2/6 a 6/6 — caos de escenario de
una muestra, no un veredicto (regla 3.20); el peldaño 15 con 48 sorteos da
40 con y sin la bajada. Queda a vigilar.

### 7.50 El cerebro de a bordo, medido sin tener el chip

`csrc/iafz_64_abordo.c`: sólo la inferencia de un genoma, sin librería de C
(popcount por SWAR; ni el ESP32 ni el C3 tienen instrucción), lista para
compilar en el ESP32. Da EXACTAMENTE lo mismo que el kernel del gym (prueba
`abordo::igual_al_kernel_del_gym`: 20 genomas × 300 decisiones, bit a bit).

Compilado con el clang de la máquina para RISC-V de 32 bits (el núcleo del
ESP32-C3) y corrido en un emulador (unicorn) con el genoma instalado, 50
decisiones (`tools/chip/medir.sh`):

```text
                                   instrucciones por decisión
  primera versión (recorre las 64)          65.900
  sólo las neuronas activas                  28.700   (idéntica salida)
```

Con FPU, a 160-240 MHz: 0.12-0.18 ms por decisión, 55-80× de margen a
100 Hz. Sin FPU (C3 pelado): 1.015 sumas por software, estimado 80-110 mil
instrucciones, 14-20×. Programa: 1.4 KB; genoma: 11.4 KB en flash. El
Xtensa del ESP32-S3 no se pudo compilar con el clang del sistema (el
destino existe pero no está habilitado): queda para el toolchain de
Espressif.

### 7.51 El hardware, y el puente CRTP para el LiteWing

Decisión de compra con presupuesto chico (hablada con el usuario): el DBPOWER
no sirve para empezar —~1.35 de empuje/peso con ~133 g no carga ESP32, flujo
y caja; la placa no se programa; motores con reductora— y convertirlo cuesta
lo mismo que un kit. Primer aparato: LiteWing (ESP32-S3, firmware derivado
de Crazyflie) con el módulo de flujo óptico. Las primeras pruebas no
necesitan el cerebro a bordo: deriva del giróscopo quieto 5 minutos (el
supuesto que manda el SLAM, 7.48), escala del flujo con cinta métrica,
hover y aterrizaje.

`crtp.rs` + `dron litewing`: las cuatro consignas del cerebro como consigna
de altura del commander genérico de Crazyflie (puerto 7, `<Bffff`, grados y
metros), con la misma capa de seguridad que el puente MSP (nunca se arma
solo; si el cerebro se calla 0.2 s, «parar»). Formato verificado contra la
documentación de ESP-Drone (UDP 192.168.43.42:2390, suma de control) y el
código de `cflib`. Cuatro pruebas, una por red real contra un dron falso.
**No verificable sin el aparato, y dicho:** los signos de alabeo/cabeceo
(`--signos`, con las hélices sacadas), la versión del protocolo, y la
telemetría de vuelta — escribir la tabla de variables de log a ciegas sería
un instrumento sin verificar (regla 3.18).

### 7.52 EXP_20260926c trabada en el 13: olvido, y el brazo con repertorio

Las dos semillas pasaron del 1 al 12 en 30 generaciones y se trabaron ~430
en el 13. Diagnóstico con `sorteos` sobre el campeón: las misiones propias
del 13 pasan a veces (mediana 640), pero el **examen de repaso** ata la nota:
olvidó esquivar (peldaños 8 y 9 a −79) y en los de abajo quedó debajo del
95% del clásico (repaso mediana 887 contra 900). Es el olvido de la regla
3.8, con el piso por peldaño de 7.47 haciendo su trabajo.

**Probado y descartado:** bajar la autoridad de la red cerca de los postes
(`autoridad_de_esquive`, mismo patrón que la posada y el sostén). Sobre el
campeón, peldaño 8: piso 1.0 → 6/24, 0.2 → 5/24, 0.0 → 5/24. No cambia
nada: el peldaño 8 no falla por choques sino por el repaso.

**Lo que se hizo:** la semilla 2 (igual de trabada) se cerró
(`s2_cerrada.log`) y se relanzó el brazo con **repertorio** (un genoma por
celda: aprender en una no pisa las otras) desde el mismo campeón, contra el
monolítico s1 que sigue corriendo. En 7.34 el repertorio protegió lo
aprendido en una semilla de dos: esto es la segunda medición.

### 7.53 Persecución retomada, sin GPS

`dron perseguir`, sin GPS (la moto se ve relativa al dron, `visto_desde`):
el clásico sigue la moto el 100% del recorrido en los seis recorridos (hasta
29 m/s); el genoma instalado de persecución, en cinco, y 84% en la ruta
completa (error medio 2.6 m contra 1.4 del clásico: ahí la red estorba).

Se cerró el brazo monolítico de `EXP_20260926c` (trabado ~620 generaciones
en el 13, sin olvidar lo de atrás pero sin pasar la confirmación) por costo
de oportunidad declarado, y se retomó el entrenamiento de persecución, el
una de las capacidades del producto —«te sigue»; **corrección del
usuario: es UN caso de uso, no el objetivo del dron**—:
`genomes/EXP_20260926c/persecucion.{adn,log}`,
7 peldaños (el último, el recorrido manejado en la cabina). Sigue el brazo
con repertorio de la entrega.

### 7.54 El residuo nace en cero: las dos corridas estaban DEBAJO de la vara

Diagnóstico de `EXP_20260926c`, con números (sin GPS):

```text
  persecución, peldaño 1 «recta de ciudad»
    clásico (REAL)                         12375.4   techo 12501.5   umbral 12419.5
    campeón, 170 generaciones clavado      12226.0   ← 150 DEBAJO del clásico
  entrega, peldaño 12 «posarse» (dron sorteos 12 n=12)
    campeón del repertorio (310 gens trabado)   aprueba  0 de 12
    clásico                                     aprueba 11 de 12
```

Por la identidad de 3.15 (red en cero = clásico), las dos redes ESTORBABAN:
se estaba mejor sin ellas. La población arrancaba al azar (persecución) o de
un genoma viejo con GPS (entrega), y la evolución nunca encontró el «no
corregir nada» que ya valía más. Es lo que el aprendizaje de política
residual resuelve inicializando el residuo en cero (Silver et al. 2018,
Johannink et al. 2019).

Arreglo: `dron residuocero ENTRADA SALIDA` (`iafz::salidas_en_cero`, con
prueba) pone las cuatro proyecciones de salida en cero; `dron perseguir` con
ese genoma da IDÉNTICO al clásico en los 6 recorridos. `entrenarpersecucion`
acepta ahora `sembrar=` y `sembrartodo`. Se cerraron las dos corridas de
`EXP_20260926c` (logs en `*_cerrada.log`) y se relanzaron en
`genomes/EXP_20260926d/` sembradas desde residuo cero:

```text
                     antes (EXP_c)                      con residuo cero (EXP_d)
  entrega            peldaño 12 trabado 310 gens         12 aprobado en gen 11, 13 en gen 35
                     (monolíticos: 13 trabado ~600)
  persecución        peldaño 1 trabado 170 gens          1-3 aprobados en gen 0-2;
                                                          4 «curva» trabado: 12007.5 contra
                                                          clásico 11873.0, umbral 12092.5
```

Una semilla por brazo: indicio, no resultado. Y lo que NO dice: en la
entrega, el umbral de cada peldaño lo pasa el clásico (calibración 7.43), así
que aprobar sembrando desde cero prueba que la red ya no estorba, **no** que
agregue algo. Lo que agrega se mide contra el clásico con `sorteos`, que es
el paso 3 de la sección 7. En persecución sí: el umbral es superar al
clásico, y los peldaños 1-3 se aprobaron cerrando el 35% del hueco a la
perfección. `dron perseguir` (ruido REAL): error medio 0.7 contra 0.9 m en
curva, 0.6 contra 0.7 en carretera, igual en zigzag y ruta, 0.3 contra 0.2
en la recta de ciudad.

### 7.55 Cierre de EXP_20260926d: persecución instalada, entrega trabada en el 15

**Persecución** (cerrada en gen ~760 de 800 por costo de oportunidad: 550
generaciones clavada en 12024.5 en el peldaño 4, umbral 12092.5, clásico
11873.0). `dron perseguir`, ruido REAL, error medio genoma / clásico:

```text
  recta ciudad 0.1/0.2 · carretera 0.4/0.7 · semáforo 0.5/0.7 · curva 0.7/0.9
  zigzag 1.5/1.5 · ruta completa 1.2/1.4        (los dos 6/6 al 100% a tiro)
  genoma instalado antes: ruta completa 84% a tiro, error 2.6 m
```

Primer genoma de persecución que NO estorba en ningún recorrido. Se instaló
como `genomes/DRON3D_PERSECUCION.adn` (md5 `44a27191…`; el anterior quedó en
`.antes_20260926`). La cabina lo carga con `consignas_hibridas`, la misma
traducción del gym. Una semilla.

**Entrega** (1200 generaciones, 4.5 h): 1-14 aprobados en 91 generaciones,
el 15 trabado 1030. `dron sorteos`, n=12:

```text
  peldaño 15   repertorio 1/12   clásico 9/12
  peldaño 14   repertorio 3/12
  peor recordado del repertorio en los dos: peldaño 12 «posarse» (−88 / −177)
```

La red vuelve a estorbar: en otros sorteos olvida posarse. La celda que se
entrena en 14-15 («crucero libre») es la misma que vuela la aproximación
de las misiones de posarse, y el examen de repaso de la corrida la ve con un
solo sorteo por generación. `sorteos=2` ya se midió peor (7.33), no se repite.

La máquina tuvo todo el día un núcleo menos: `systemd-udevd` en un loop de
carga del driver NVIDIA (GeForce 940M, no soportada por el 595), ~2 veces por
segundo, 2 días 20 h de CPU. Hay que desinstalarlo (necesita sudo).

### 7.56 Módulos por FUNCIÓN: la altura, aparte (idea del usuario)

El usuario, 2026-09-26: *"el cerebro humano tiene partes dedicadas a ciertas
funciones, no solo una sola masa haciendo todo"*. El repertorio ya partía la
red, pero por SITUACIÓN (crucero/aproximación/sostén), y cada celda manejaba
las cuatro salidas. El censo con la vara (`dron celdas clasico`) muestra el
cruce: posarse (12) vuela 57% en «sostén libre», y esa celda la entrenaba ida
y vuelta (13), que la usa 19%.

Cómo falla el repertorio de 7.55 en posarse (`dron mision 12 0 detalle
rep=… variante=50000N`, `rep=` agregado a `mision` para esto): en cada
fallo **SE ESTRELLÓ a 0.06–0.49 m del punto**. Llega bien y baja mal: la
falla es de la función vertical.

Implementado (`dron entrenarmision … repertorio vertical`):
`repertorio::VERTICAL` es un genoma aparte que emite SÓLO la salida de
altura, en todo el vuelo; las celdas emiten sólo las otras tres. Los peldaños
de despegar/posarse (10-12) entrenan el módulo y ninguna celda; los demás
entrenan su celda y no pueden tocar la altura. El módulo se piensa con
`abordo::pensar` (memoria propia, bit a bit el cerebro del chip). Se guarda al
lado del repertorio (`*.vertical.adn`), y `sorteos`/`mision rep=` lo cargan
solos. Prueba `el_modulo_vertical_igual_a_las_celdas_vuela_identico`: con el
módulo igual a las celdas el vuelo da idéntico bit a bit, y con uno distinto
cambia. Batería 123/123 tres veces seguidas.

Corriendo: `genomes/EXP_20260926e/v1` y `v2` (semillas 1 y 2, sembradas de
residuo cero). Control: `EXP_20260926d/rep` (semilla 1, sin módulo): 1-14 en
91 generaciones, 15 trabado 1030, posarse 1/12 al final.

**Deuda (regla 3.3):** el visor todavía no vuela el módulo vertical. No se
instala nada de esto hasta conectarlo.

**Medición intermedia (v1, generación ~75, repertorio + módulo vertical tal
como están en disco), `dron sorteos n=12`:**

```text
                   sin módulo (EXP_d, final)   con módulo (v1)   clásico
  12 posarse              1/12                    8/12            11/12
  15 entrega              1/12                    5/12             9/12
```

Una semilla y a mitad de corrida: indicio. Todavía debajo de la vara.

### 7.57 Cierre de las corridas con módulo vertical, módulo POSAR, y el visor vuela el repertorio

**v1/v2 (vertical), cerradas en gen ~560 de 1200**, las dos trabadas en el
15. `dron sorteos n=12`:

```text
                     control sin módulo   v1 gen 75   v1 gen 560   v2 gen 560   clásico
  12 posarse              1/12              8/12         5/12         6/12        11/12
  15 entrega              1/12              5/12         4/12         4/12         9/12
```

Dos semillas: separar la altura mejora 4-6× y lo repiten las dos. Pero
posarse SE SIGUE DEGRADANDO con el vertical ya congelado (8 → 5 en v1), y
los fallos siguen siendo aterrizajes (3 «se estrelló» y 5 «sin reloj» en 4
variantes × 3 misiones con ruido duro; 1 caída con ruido REAL). Lo que
queda: 13-15 siguen entrenando las celdas horizontales que el dron usa
MIENTRAS baja.

**Módulo POSAR** (`repertorio::POSAR`, `entrenarmision … vertical posar`):
dueño de las cuatro salidas en los tramos de aterrizaje (`Punto::posar`, lo
dice la tarea). Lo entrenan sólo los peldaños 11-12; despegar (10) entrena el
vertical; 13-15 sus celdas. Prueba: igual a las celdas vuela idéntico, y uno
distinto cambia el aterrizaje. Corriendo: `genomes/EXP_20260927/p1`, `p2`.

**`repertorio::Volador`**: quién piensa en cada decisión (celda, vertical,
posar), en UN lugar, usado por `mision::volar_con` y por el visor. El
refactor da idéntico bit a bit al binario anterior (misma tabla de `mision
12 detalle` por md5).

**El visor vuela el repertorio de entrega** si existe
`genomes/DRON3D_ENTREGA.repertorio.bin` (+ `.vertical.adn`, `.posar.adn`),
con recarga en vivo; `/estado` publica `cerebro` (qué módulo decide ahora).
Verificado instalando v1 a mano: `cerebro: repertorio de entrega · celda
«sostén libre» + módulo vertical`, tecla M → ENTREGA CUMPLIDA en 19.6 s (un
vuelo). Se desinstaló después: no pasa `sorteos` (regla de la sección 7).

**p1/p2 (vertical + posar), medición temprana** (gen ~40-50, `sorteos 12
n=12`): posarse **6/12 y 7/12**. La misión de posarse es un único tramo de
aterrizaje, así que desde que se aprobó el 12 (gen 10 y 10) ese número ya no
puede cambiar: nada posterior toca el módulo posar. O sea que el olvido
quedó cerrado por construcción, y lo que falta es otra cosa: el módulo
aprobó con 6 confirmaciones y generaliza a 6-7 de 12, debajo del clásico
(11/12), del que arrancó. La evolución lo alejó del clásico hacia algo que
pasa el umbral fijo del peldaño en los sorteos que vio y no en otros.

### 7.58 El guardián de aprobación, otra vez: 6 confirmaciones alcanzaban para p = 0.16, no para p = 0.55

El módulo posar de p1/p2 aprobó posarse en la generación 10, con 1 y 5
cruces rechazados antes, y generaliza a 6-7 de 12 sorteos. Con p ≈ 0.55,
pasar 6 confirmaciones seguidas es 0.55⁶ = 2.8% por cruce: nada raro. La
cuenta de 7.34 (`0.16⁶ × 425`) suponía el p de un genoma que casi nunca
pasa. `CONFIRMACIONES` = 12: 0.55¹² = 0.08% por cruce; algo tan bueno como el
clásico en la entrega (p = 0.75) pasa el 3% de los cruces, con cientos por
corrida. Batería 123/123 tres veces. p1/p2 cerradas (`p*_cerrada.log`) y
relanzadas en `genomes/EXP_20260927b/` con el binario nuevo.

**Resultado de `EXP_20260927b` (12 confirmaciones, vertical + posar, 1200
generaciones), `sorteos n=12`:**

```text
            posarse   ruta larga   entrega   dónde quedó
  p1         6/12        —          2/12     15 trabado
  p2         9/12       8/12        7/12     14 trabado (400 gens)
  clásico   11/12        —          9/12
```

Con 12 confirmaciones posarse se aprobó en la generación 309 (p1) y 164
(p2), no en la 10. p2 es el mejor repertorio medido (entrega 1 → 7 de 12),
pero ninguno llega al clásico y la dispersión entre semillas es enorme (2 contra 7).
Se lanzan las semillas 3 y 4 (`genomes/EXP_20260927c/`) para saber cuál de
las dos es la regla.

### 7.59 El simulador tenía un aparato perfecto: efecto suelo y aparato variable

Pregunta del usuario: ¿la red no sirve o le falta algo? La literatura de
control residual en drones gana EXACTAMENTE donde el modelo nominal del
controlador está mal: efecto suelo al aterrizar (Neural Lander, Shi et al.,
ICRA 2019), viento (Neural-Fly, O'Connell et al., Science Robotics 2022),
masa y parámetros mal especificados (Residual Policy Learning, Silver et al.
2018). Nuestro clásico está hecho con la física EXACTA del simulador y el
simulador no tenía efecto suelo: la red competía contra un modelo perfecto,
que es el caso en que un residuo no tiene nada que corregir salvo el ruido.

Agregado, y el simulador queda más fiel (sección 1: el punto final es
hardware):

- `sim3d::efecto_suelo` (Cheeseman-Bennett): +11% de empuje a 5 cm, nada
  arriba de ~20 cm. Vale en todas partes, visor incluido.
- `mision::aparato_de`: cada misión es otro aparato — masa −0.08..+0.18 kg,
  0-12% menos empuje, viento ×0.6..×1.8, centro de masa corrido hasta 0.012
  N·m. Sale de semilla + sorteo; `fisicanominal` lo apaga. El clásico sigue
  suponiendo el nominal (como cualquier controlador de vuelo real).

**La vara con la física nueva** (`dron sorteos N clasico n=24`,
`EXP_20260927d/vara_*.txt`):

```text
  peldaño            1-11   12 posar   13 ida y v.   14 larga   15 entrega
  nominal (+suelo)    24     23          21           21          19
  aparato variable    24     13          12           12          11
```

El efecto suelo solo casi no cambia nada (23 contra 23 medido antes con otra
n); el aparato variable parte al clásico justo al aterrizar: se estrella o
se queda sin reloj con el aparato pesado o flojo, **incluso con sensores
limpios** (sorteo 500005, misión 3). La compensación de gravedad del lazo de
altura usa la masa NOMINAL y el integrador llega tarde. Ése es el hueco que
un residuo llena en la literatura.

**Cuidado con la vara (3.2), escrito para no olvidarlo:** un controlador
comercial tiene estimador de empuje de hover (PX4 `mc_hover_thrust_estimator`).
El intento de 7.43 lo descartó porque hundía al dron cargado. Si la red le
gana al clásico acá, la afirmación es "le gana a un clásico sin estimador de
hover"; antes de afirmar más, hay que darle al clásico uno que ande y re-medir.

Todos los números de misión anteriores a esta entrada son con aparato nominal
y sin efecto suelo: no se comparan con los de acá.

Corriendo: `EXP_20260927d/f1`, `f2` (vertical + posar, 12 confirmaciones,
física nueva). Se cerraron las semillas 3-4 de 7.58 (física vieja).

**f2 aprobó posarse con la física nueva (gen 182, 12 confirmaciones). Medido
contra la vara en los MISMOS 24 sorteos** (`sorteos 12 n=24`):

```text
  red (f2)   15/24     clásico   13/24
  sorteos que gana sólo la red: 7   ·   que gana sólo el clásico: 5
```

Pareado, 7 contra 5: **empate dentro del ruido** (prueba del signo, p ≈ 0.77).
La red ya no estorba en el aterrizaje con aparato variable (con física vieja
estorbaba: 6/12 contra 11/12), pero todavía no se puede decir que lo mejore.
Se había dicho al usuario "probablemente aterrice mejor" por la cuenta de las
12 confirmaciones (0.54¹²); la medición la desmiente — ese cálculo suponía p
igual al del clásico en TODOS los sorteos, y la red gana en unos y pierde en
otros. Corregido en el mismo informe.

**f1 también aprobó posarse (gen 289). Mismos 24 sorteos:** red 17/24,
clásico 13/24; gana sólo la red en 6, sólo el clásico en 2 (p ≈ 0.29).
Juntando f1 y f2: 13 contra 7 a favor de la red (p ≈ 0.26). Tendencia de las
dos semillas hacia el mismo lado, **no probado todavía**. Se miden 96
sorteos por piloto (`sorteos12_n96_*.txt`) para decidirlo.

**96 sorteos por piloto, posarse (cierre de la tendencia, 2026-10-10).** Mismos
96 sorteos para los tres (`sorteos12_n96_*.txt`):

```text
  clásico 64/96   f1 65/96   f2 66/96
  f1 contra clásico: sólo la red 19, sólo el clásico 18  (prueba del signo, p ≈ 1.00)
  f2 contra clásico: sólo la red 23, sólo el clásico 21  (p ≈ 0.88)
```

**La tendencia de 7.59 (13 contra 7 con n=24) no se sostuvo con n=96: es un
empate.** La red aprende a no estorbar en el aterrizaje con aparato variable,
pero no lo mejora. No se afirma ventaja de la red sobre el clásico (3.2, 3.20).
Pendiente real: lo que el clásico no tiene (estimador de hover, ver arriba) y
reentrenar la entrega; el entrenamiento largo no se relanza mientras la PC no
esté lista para cargas sostenidas (decisión del usuario, 2026-10-10).

**Campeones f1/f2 contra la vara, peldaños 12-15, 24 sorteos (2026-10-10).**
Los dos entrenamientos terminaron las 1200 generaciones (~8,8 h cada uno) y
quedaron estancados en la entrega (peldaño 15). Repertorio completo:

```text
  peldaño        12 posar   13 ida y v.   14 larga   15 entrega
  clásico            13         12           12          11
  f1                 17         12           12          11
  f2                 15         12           (sin medir)  (sin medir)
```

Igual que el clásico en todo menos posarse (empate pareado, ver arriba). **No
se instala ningún genoma**: el criterio es ≥ 10/12 por peldaño contra la vara
**y** `capacidades`, y la red no supera al clásico en ninguno. f2 en 14-15 quedó
sin medir: el guardia de temperatura cortó a 80 °C (laptop de 4 núcleos).
`DRON3D_64.adn` (v5) sigue instalado, sin citar números suyos.
Siguiente de verdad (ya no es cuestión de más generaciones): dar al clásico un
estimador de hover (3.2) y re-medir, o atacar la entrega con otra señal.
