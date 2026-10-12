# La biblia del proyecto

**Leé este archivo ANTES de tocar nada.** Es el contrato del proyecto: qué se
está construyendo, qué no se negocia, qué infraestructura existe ya, y cómo se
trabaja acá. Si volvés después de un año sin contexto, esto es lo que te pone
al día. Si algo de acá se contradice con lo que creés recordar, gana este
archivo — y si descubrís que este archivo quedó viejo, **actualizalo en el
mismo cambio**, no después.

**Este archivo es el CONTRATO. El REGISTRO es `BITACORA.md`.** Acá están las
reglas y el estado vigente; allá está el detalle de cada sesión —qué se probó,
qué dio, qué se descartó y con qué número—. Cuando vayas a tocar un área,
leé su entrada en la bitácora antes de empezar: es donde están las mediciones
que ya se hicieron, para no volver a gastarlas.

La separación se hizo en la auditoría de reglas (bitácora 7.10) y tiene motivo
medido: este archivo había llegado a 2009 líneas, 62% de ellas historia, y se
contradecía solo —tres tablas de resultados sin marcar cuál valía, y una
conclusión refutada todavía escrita en negrita cien líneas después de su propia
refutación—. **Nada se borró: se movió.** La regla 3.4 lo exige.

---

## 1. Qué es esto

Un dron simulado en Rust pilotado por una red binaria evolucionada (IAFZ-64),
con la ambición de terminar volando en hardware real. Hubo, durante un tramo
del proyecto, un subcircuito del conectoma **real** de *Drosophila*
(FlyWire/Princeton) atornillado abajo como línea de investigación paralela; se
probó a fondo con el instrumento arreglado y se descartó con una medición
limpia, así que el código se borró del repo (bitácora 7.9).

El objetivo NO es que el número del simulador sea grande. Es que el aparato
vuele de verdad afuera. Todo lo demás sale de ahí.

**El punto final del proyecto es hardware.** El software tiene que simular lo
más fiel posible; una vez que las pruebas en software pasan, el paso siguiente
es hardware real, no un número más grande en el simulador. Esto es una decisión
explícita del usuario, no una interpretación: "el punto final es el hardware,
entonces el software debe simular lo más fidedigno que se pueda". Consecuencia
concreta: el aparato simulado dejó de ser números elegidos para que el gym
anduviera (0.8 kg, 4 m/s de techo) y pasó a ser un quad de 5" de persecución
real, comprable, con arrastre cuadrático, motores no lineales y batería que cae
— ver `sim3d.rs`, sección "EL APARATO CONCRETO".

**La afirmación defendible del proyecto, hoy, es ésta y nada más grande:** la
red no pilota, **corrige a quien pilota** (regla 3.15). Con la red en cero el
piloto híbrido ES el piloto clásico. Decirlo de otra forma es maquillar.

**LAS TAREAS DEL GYM SON ANDAMIO, NO EL PRODUCTO.** Corrección explícita del
usuario, y hay que tenerla presente porque es fácil confundirse leyendo el
currículo: entregar un paquete y perseguir una moto son **con qué se entrena**,
no para qué existe el aparato. Su palabra: *"nuestro dron no es sólo para ir
del punto A al punto B; por algo le estamos poniendo que aprenda en vivo"*. Lo
que se pide de verdad es un robot al que se le pueda decir **una tarea que
nadie entrenó**: encadenar los puntos que sean, recoger cajas y llevarlas,
clasificarlas y después llevarlas. El currículo enseña las MANIOBRAS; la
generalidad tiene que salir de que la tarea sea algo que se le *dice*, más lo
que aprende en vuelo (`vivo.rs` + `repertorio.rs`).

Consecuencia para quien diseñe acá: **medir "¿sirve para la tarea del gym?" no
alcanza como criterio de diseño.** Antes de atar algo a la entrega —un sensor,
una salida, una recompensa— hay que preguntarse si sobrevive a que la tarea
cambie. Lo que no sobreviva, va aislado y parametrizado, como ya está la física
por vehículo.

**La vara de ambición es el NIVEL DE AUTONOMÍA de Boston Dynamics, con nuestra
filosofía de costo.** Otra vez textual: *"ellos usan miles de dólares en
cómputo, miles de sensores caros... queremos igualarlos o superarlos, pero con
nuestra filosofía"*. O sea que la comparación honesta NO es "¿hacemos piruetas
como Atlas?" sino **"¿el aparato se las arregla solo en algo que no vio antes,
con un cerebro de 11 KB en un chip de dos dólares?"**. Eso hace que el
presupuesto (regla 3.14) deje de ser una restricción incómoda y pase a ser la
tesis del proyecto: si hiciera falta cómputo caro para lograrlo, no habríamos
probado nada.

**Aclaración del 2026-09-21, porque ya generó una confusión real y le costó un
mal rato al usuario:** el párrafo de arriba compara una PROPIEDAD —autonomía
por dólar de cómputo—, no compara PRODUCTOS. Nunca dijo "el aparato termina
evitando obstáculos como un DJI" ni "hace lo que hace un Atlas de Boston
Dynamics". Un asistente externo leyó la ambición del proyecto como si fuera
eso, se la devolvió al usuario como una promesa incumplida, y por un rato se
la creyó. **No competir con DJI ni con Boston Dynamics en capacidad absoluta
no es una noticia nueva ni una degradación del proyecto — nunca fue la meta.**
Si en algún momento este archivo se lee como que sí lo es, está mal escrito y
hay que corregirlo, no creerle.

**El producto concreto que el usuario tiene en mente, dicho el 2026-09-21 y
que no hay que volver a perder de vista:** no un competidor de DJI, sino un
bicho autónomo del tamaño y precio de un juguete que (1) **te sigue** —
`persecucion.rs`/`/cabina` ya entrenan justo eso—, (2) **va de un punto a otro
detectando lo que tiene adelante** —paredes, subidas, postes— sin chocarse
—el currículo de `mision.rs` ya apunta ahí—, y (3) **puede buscar un objeto
marcado y llevarlo a otro lado**: la palabra del usuario fue un imán en el
dron y otro en una caja, que se activa o desactiva. Eso no es una tarea nueva:
es la tarea de entrega de siempre (dos párrafos abajo) con un mecanismo
concreto — un electroimán que prende y apaga, ni un gramo más de inteligencia
que la que ya hace falta para llegar al punto. Nada de esto está resuelto en
hardware real todavía (sección 7: 0% probado), pero tampoco depende de nada
que no esté ya en esta lista. Una pantalla de estado (un OLED chico, del orden
de dos dólares, con texto o una cara) es mejora de INTERFAZ, no de cerebro: no
compite por bits de sensor ni por ciclos del kernel (regla 3.14) y se agrega
al final, cuando el resto ya vuele.

**Seguir un vehículo en movimiento (la moto del usuario) es UN caso de uso,
no el objetivo del dron.** Corrección del usuario, 2026-09-26: *«es un caso
de uso, sí, pero no es el objetivo del dron»*. El objetivo son las tres
cosas de arriba —seguirte, ir de un punto a otro esquivando, buscar un objeto
marcado y llevarlo— y, por encima, hacer tareas que se le DICEN y que nadie
entrenó. La persecución se entrena con `movil.rs` (la cinemática del
vehículo) y `persecucion.rs` (cuánto tiempo lo tuvo a tiro), y es una tarea
DISTINTA de la entrega.

**La tarea original, todavía vigente y sin cerrar**: entregar un paquete.
Despegar del suelo, cruzar hasta el punto B esquivando lo que haya, posarse ahí
sin romperse, despegar de nuevo, volver a la base y posarse. Eso es
`mision.rs`, y está armado como una cadena de puntos justamente porque cada
eslabón es una de las maniobras que el aparato tiene que saber hacer. No es una
demo aparte del gym: **es** el gym.

**El hardware ya elegido es un DBPOWER FPV (placa `UD-U818H`) que el usuario ya
tiene.** Es clase juguete —no puede perseguir una moto— y ES A PROPÓSITO un
cuerpo distinto del quad rápido simulado: el objetivo de fondo, dicho por el
usuario, es que el mismo enfoque (cerebro binario barato + física programada +
corrección aprendida) sirva algún día para "un helicóptero autónomo o un robot
volador en Marte". **El cuerpo cambia, el enfoque no.** Por eso la física vive
aislada en constantes de `sim3d.rs` y no mezclada con `iafz.rs` ni con
`gym3d::consignas_hibridas`: cuando haga falta un perfil de vehículo nuevo, se
agrega un perfil, no se reescribe encima del que funciona. Detalle del aparato
y de las mediciones reales del usuario: bitácora 7.1.

**SIN GPS. Dicho por el usuario el 2026-09-25, y no estaba escrito en ningún
lado:** *"la idea de nuestro dron es que evolucione en tiempo real para que no
necesite GPS"*, y el 2026-09-25 lo repitió: *"es innegociable, es el corazón
del proyecto"*. **Implementado el mismo día (bitácora 7.39):** `sim3d::Sensores`
ya no da posición absoluta; da odometría de flujo óptico + IMU con error de
escala, sesgo y deriva de rumbo (`SIN_GPS`, `ESCALA_FLUJO`, `SESGO_FLUJO`,
`SESGO_GUINADA`). La posición deriva y nadie la corrige. **Todo número medido
antes del 2026-09-25 se midió CON GPS y no es comparable.** Un objetivo o una
moto no se geolocalizan: se VEN, relativos al dron (`persecucion::visto_desde`).
Nunca proponer nada que asuma posición absoluta.

**Un cuerpo más para esa lista, dicho el 2026-09-21: un carrito de control
remoto con el mismo chip.** Es más simple que cualquier dron —sin altitud, sin
actitud, sin motores que se caen del cielo si se equivocan— y es un
subconjunto de lo que `Guiado` ya resuelve: posición y rumbo sobre un plano,
nada más. No se arranca ahora; se anota acá para que la próxima vez que se
elija un perfil de vehículo nuevo, sea un perfil más en `sim3d.rs` y no una
sorpresa.

---

## 2. Infraestructura que YA existe (no la descubras de nuevo)

| Qué | Dónde | Detalle |
|---|---|---|
| **Repertorio de entrega en el visor** | `genomes/DRON3D_ENTREGA.repertorio.bin` (+ `.vertical.adn`, `.posar.adn`) | Si existe, el visor vuela ESO (celdas por situación + módulos por función) con `repertorio::Volador`, el mismo del gym; recarga en vivo. `/estado` → `cerebro` dice qué módulo decide. Hoy no hay ninguno instalado (bitácora 7.57). |
| **Adaptación en vuelo** | tecla **`V`** en el visor | El dron prueba variantes de sí mismo mientras vuela y se queda con las que miden mejor, por nicho (`vivo.rs` + `repertorio.rs`). Lo aprendido se guarda en `genomes/DRON3D_REPERTORIO.bin` al apagarla y se retoma al prenderla. `/estado` publica nicho, fase y el mapa entero. |
| **Decirle una tarea** | caja «decirle una tarea» del visor · `/tarea?t=` · `dron tarea "..."` | `despegar Z; ir X Y [Z]; posar [X Y]; agarrar [kg]; soltar; caja X Y a X Y [kg]; volver`. Sin reentrenar: el cerebro sabe maniobras, la tarea es la cadena. Con varias `caja`, el ORDEN lo elige el dron (el recorrido más corto, sobre el mapa si ya lo conoce). Agarrar/soltar cambia la masa en vuelo (el imán). |
| **Mapa y ruta** | `plan.rs` · anillos celestes en el visor | Las tareas dichas vuelan con MAPA: cada poste que la cámara ve queda anotado (fusión con incertidumbre, que crece con la deriva) y la ruta lo rodea, recalculada a 10 Hz. En el visor el mapa dura toda la sesión: un camino ya recorrido se vuelve a hacer sabiendo dónde está cada poste. El gym NO lo usa (mide maniobras con la vara reactiva, regla 3.2). |
| **Cabina de persecución** | **`/cabina`** en el mismo 8080 | Manejás la moto en primera persona (apaisado, dos pulgares) y una ventanita muestra **cómo te ve el dron**, con el mismo campo de visión del sensor simulado. El botón *grabar* guarda tu manejo en `recorridos/*.mandos` y el entrenador lo suma solo al currículo. Es la forma de probar la persecución y de generar datos al mismo tiempo. |
| **Refinar un nicho fuera de línea** | `dron refinar [gens]` | El mismo lazo anclado de `vivo.rs`, pero contra misiones del gym en vez de vuelo real: miles de empujones por minuto. Hoy refina «crucero tapado» con misiones de esquive AISLADO. Imprime la vara honesta, el elite anterior y el refinado sobre el MISMO lote held-out, y los cuenta en castellano (llega limpio / roza el poste). Ver bitácora 7.30. |
| **Ver el entrenamiento desde el visor** | panel «entrenamiento corriendo» · `/entrenamientos` | Qué está haciendo AHORA cada `entrenarmision` que corra en otro proceso: generación, peldaño, mejor contra lo que necesita, **cuántas veces cruzó el umbral sin confirmar** (el número que no rebota) y hace cuánto escribió. Lee el log del proceso por `/proc/PID/fd/1`, así que **funciona con las corridas que ya están en el aire** y no hace falta tocar el entrenador (que está al lado del camino caliente). El panel viejo leía `etapas::PIZARRA`, que `entrenarmision` no escribe nunca: estaba permanentemente vacío. Ver `progreso.rs`. |
| Visor web | puerto **8080** | **Lo levanta un servicio systemd de usuario**: `dron-visor.service`, habilitado. Se respawnea solo si lo matás. |
| Acceso remoto | Tailscale | `http://100.83.97.54:8080` (verificado). El servidor escucha en `0.0.0.0`; `web.rs` ya imprime la IP del tailnet al arrancar. No hace falta túnel ni proxy. |
| Genoma 3D principal | `genomes/DRON3D_64.adn` | Lo sobrescribe `entrenarmision` si no se le pasa `salida=`. Hacé backup antes de experimentar, o pasá `salida=RUTA`. |
| Genoma de persecución | `genomes/DRON3D_PERSECUCION.adn` | Lo vuela la cabina. Un archivo que falta no rompe nada — vuela el clásico. (La mezcla de expertos por régimen se retiró por medición y su código se borró el 2026-09-26.) |

Comandos para el servicio:
```bash
systemctl --user status dron-visor      # ver si corre
systemctl --user restart dron-visor     # recargar tras recompilar
```

**Regla del visor único:** existe UN solo visor, en el 8080, servido por ese
servicio. No levantes instancias paralelas en otros puertos "para probar". Si
necesitás ver un cambio, recompilá y reiniciá el servicio.

---

## 3. Reglas que no se negocian

> **Sobre estas reglas.** Se auditaron enteras contra el proyecto de hoy
> (bitácora 7.10). El resultado fue que casi ninguna estorbaba: se ampliaron
> 3.2, 3.7, 3.12 y 3.14, se retiró una (la vieja 3.10, cuyo objeto de estudio
> ya no existe) y se agregó 3.18. **Esta lista es mutable por medición**
> (regla 3.7, corolario del usuario), pero 3.2 y 3.15 no: son el instrumento
> con el que se sabe si algo funcionó.

### 3.1 La red nunca toca los motores

La red emite **consignas de navegación** (velocidad deseada adelante/lateral en
marco del cuerpo, velocidad de guiñada, corrección de altura). `Guiado`
(`pid3d.rs`) las convierte en actitud, `Estabilizador3` en torques, `Motores`
en empuje con retardo de ESC. Esa cadena es lo que hace el proyecto trasladable
a hardware: el día que se suba a un dron real, `Estabilizador3` se borra y lo
reemplaza el controlador comercial.

Corolario: **`Estabilizador3` tiene que parecerse a un controlador comercial**,
y eso es motivo válido para tocarlo (no así `Guiado::consignas`, que es la vara
— ver 3.2). Por eso el lazo de altura pasó a ser cascada posición → velocidad →
empuje con tope de régimen: no es una concesión, es lo que hace cualquier
ArduPilot. Por la misma razón el lazo de giro tiene I con *iterm relax* y
anti-windup, como Betaflight: sin ella una carga corrida (la caja del imán)
dejaba al dron inclinado 8° para siempre (bitácora 7.37). Los topes están en 3.0 m/s de subida y 2.5 m/s de bajada, **el doble
de lo que aguanta el tren de aterrizaje a propósito**, para que posarse suave
siga siendo algo que el dron tiene que lograr y no algo que el controlador le
regala.

### 3.2 La vara honesta es sagrada

`Guiado::consignas` (`pid3d.rs`) es el piloto clásico contra el que se mide el
genoma. **No se toca para que los números queden lindos.** Existe justamente
para que no se puedan maquillar. Si el genoma no le gana, se dice.

**Lo que esta regla NO prohíbe, y hubo que aclararlo porque ya pasó:** arreglar
un BUG de la vara. La vara honesta no es "el número que le sale al clásico", es
"lo mejor que el modelo físico sabe hacer". Si el clásico está peor de lo que la
física permite, eso es un defecto del instrumento y arreglarlo es obligatorio,
no opcional. Pasó de verdad: el clásico no podía aterrizar porque bajaba de
costado, se le agregó la regla de aproximación (alinear antes de descender,
igual que PX4) y pasó de 1/3 a 3/3 en posarse.

La distinción, que es la que importa:

- **Bajar la vara porque el genoma no le gana** → prohibido, es maquillaje.
- **Subir la vara porque tenía un defecto** → obligatorio, aunque el genoma
  quede peor en la comparación. Suele quedar peor. Se dice igual.

**Y la consecuencia que hay que ejecutar en el mismo cambio, porque ya se
omitió una vez:** cuando la vara se mueve, **toda comparación anterior queda
inválida**. No es que envejezca: deja de significar. Si no se re-mide el genoma
contra la vara nueva, los dos efectos quedan mezclados y ya no se puede saber
cuánto mejoró el genoma y cuánto mejoró la vara. Eso exactamente pasó con la
tabla de la posada, y quedó anotado en la bitácora como deuda: *"los dos efectos
están mezclados en esta tabla y separarlos necesitaría re-medir el genoma viejo
contra la vara nueva, que no se hizo"*. O se re-mide, o la tabla se marca como
no comparable. No hay tercera opción.

### 3.3 Todo lo que se afirma tiene que ser verificable en el visor

Si se dice "el dron esquiva postes", el usuario tiene que poder VER el poste y
VER que lo esquiva. **Una capacidad que se AFIRMA pero no se puede mirar ni
probar en el visor no está terminada.**

La regla muerde sobre lo que se afirma y sobre lo que se instala, no sobre lo
que se explora: un experimento en el scratchpad que todavía no es una
afirmación no necesita trabajo de visor. Lo que no se puede hacer es anunciar
una capacidad, o instalar un genoma que dice tenerla, sin que se pueda mirar.

Esto ya mordió: el gym entrenaba con postes desde hacía rato y **el visor no
tenía obstáculos en absoluto** —ni dibujados ni en la física ni en lo que veía
la red—, así que "esquiva postes" era imposible de verificar o desmentir desde
afuera. Hoy el visor tiene banco de pruebas:

- **punto A / punto B**: click en el piso mueve la partida o el objetivo
  (`/punto?cual=a|b&x=&y=&z=`).
- **postes en vivo**: click tira un poste mientras el dron vuela
  (`/poste?x=&y=&r=&h=`), con la MISMA física del gym (`Obstaculo::choca`:
  rozarlo tumba) y **visibles para la red** (`camara.ver_con`, no `ver`).
- **quitar postes** (`/control?k=c`) y **volar de nuevo** (`/control?k=r`).
- **cámara 100% libre** (`OrbitControls`): un dedo rota, dos pellizcan zoom y
  panean. El punto A/B se coloca con raycast contra un plano invisible, así que
  funciona desde CUALQUIER ángulo, no uno predeterminado. Más «cámara fija»,
  que suelta el seguimiento para mirar una entrega entera desde un punto quieto.
- **un poste puesto justo donde ya está el dron no da tiempo de reacción a
  nadie** — es física, no un bug. Con margen sí esquiva. Verificado con
  `/estado` en vivo, no es una suposición.
- **la misión de entrega, con la tecla `M`**: arma la cadena de seis tramos
  entre los puntos A y B que pusiste, con los postes que hayas tirado.

**La regla de abajo de esta regla: la condición de "llegó" vive en UN solo
lugar.** `mision::Seguidor` lo usan el gym y el visor. Lo mismo con la
traducción de salidas del cerebro a consignas (`gym3d::consignas_hibridas_punto`),
que estuvo copiada en seis archivos. Mientras esas dos cosas estén duplicadas, el
navegador puede mostrar un dron que no es el que se entrenó y nadie se entera.

**Y la otra mitad, que costó una interfaz entera**: si mirar puede romper lo
que estás mirando, el visor dejó de ser un instrumento de medición. El modo de
colocación quedaba armado e invisible al minimizar un panel, y el siguiente
toque teletransportaba el dron en media entrega. Detalle en bitácora 7.6.

Cuando agregues una capacidad al gym y la vayas a afirmar o instalar,
agregala al visor en el mismo cambio.

### 3.4 No se maquillan resultados

Si un cambio empeora las cosas, se dice con los números en la mano y se
revierte. Los intentos fallidos se documentan con la medición que los descartó
— valen tanto como los que salieron bien, porque evitan que alguien los repita.

Corolario para la bitácora: una conclusión refutada **no se borra**, se marca
como refutada con el número que la refutó. Borrarla es perder la información de
que alguien ya recorrió ese camino.

### 3.5 Medir, no argumentar

Ninguna conclusión sobre comportamiento sale de razonar sobre el código. Sale
de instrumentar y mirar la traza. Ejemplos, todos reales y todos invisibles en
la aptitud:

- el genoma giraba sin parar (yaw de 0 a −6.78 rad en 9 s);
- orbitaba a 1.46–1.83 m del objetivo en vez de entrar;
- flotaba a **4.39 m persiguiendo un punto a 3.0**: llegaba perfecto en x/y y
  la distancia que le faltaba era casi toda vertical;
- terminaba **a 0.47 m de un punto con radio 1 m** y marcaba cero tramos,
  porque la condición de llegada pedía medio segundo sin una sola ráfaga;
- se quedaba a 3.3 m del objetivo en las rutas de dos postes **sin chocar
  ninguno**: sabía rodear, no sabía encadenar dos rodeos.

Ninguna de las cinco se podía deducir del código. Las herramientas que las
encontraron —`mision N 0`, `detalle`, `salidas`— están en la sección 4.

### 3.6 El código de sensores es parte de la arquitectura, no formato

El kernel dispara por **vecindad de Hamming**
(`popcount(~(contexto ^ pesos)) >= umbral`). Eso impone un requisito sobre cómo
se le habla: valores numéricamente parecidos tienen que dar patrones de bits
parecidos, o no existe ningún patrón de pesos que signifique "estoy cerca".
Está todo en `codigo.rs` con las mediciones. Antes de cambiar un campo de
`sensores`, leelo: el código binario que había hacía que estar a 14 m fuera
**más parecido** a estar encima del objetivo que estar a medio metro.

### 3.7 NO SE AVANZA CON LA FASE ANTERIOR INCOMPLETA

**Terminantemente prohibido.** Si la fase actual o la anterior están
incompletas, no se pasa a la siguiente: se termina primero. Vale para las tres
escalas en las que aparece esta tentación, y en las tres ya mordió:

**1. En el entrenador.** El currículo de misiones tenía una "válvula de escape"
que subía de peldaño al estancarse, sin haber aprobado. El resultado, medido:
el genoma llegó a la misión de entrega **sin saber esquivar dos postes y sin
saber posarse**, y terminó los peldaños con deuda escondida en cada uno. Un
entrenamiento clavado gritando "no aprendo a esquivar dos postes" vale MÁS que
uno que termina la escalera fingiendo. Hoy `gym3d::entrenar` **sólo sube de
peldaño aprobándolo**; saltar existe pero como decisión de una persona
(`entrenarmision ... saltar`) y lo grita en el log.

**2. En la sesión de trabajo.** Se hace una cosa, se termina, se verifica, y
recién ahí se empieza la siguiente. Nada de lanzar el entrenamiento largo
mientras el visor tiene bugs sin revisar, ni reportar el informe de capacidades
antes de haber confirmado que lo que muestra el visor es real. Si una fase
queda a medias, se anota como deuda **antes** de tocar otra cosa.

**3. En lo que se le dice al usuario.** No se anuncia una capacidad "lista"
mientras la verificación de esa misma capacidad está pendiente. Ver 3.12.

**Lo que esta regla NO prohíbe, y hubo que aclararlo porque congelaba el
proyecto:** cambiar la ARQUITECTURA que causó el bloqueo no es avanzar con la
fase anterior incompleta — **es terminarla**. Con cuatro peldaños de deuda en
persecución, la lectura literal decía "no se puede hacer nada", y lo que
destrabó fue exactamente lo contrario: medir POR QUÉ el mecanismo no podía
pasar esos peldaños y cambiar el mecanismo. La distinción es: agregar una
capacidad NUEVA encima de la deuda está prohibido; atacar la causa de la deuda
es el trabajo.

**Y lo que la regla EXIGE antes de moler generaciones, agregado en la auditoría
porque estaba disperso en cuatro lugares y por eso no se aplicaba:** un peldaño
trabado es primero una hipótesis sobre el peldaño, no sobre el genoma. Antes de
dejarlo correr mil generaciones más, contestá estas tres, en este orden:

1. **¿El umbral es alcanzable?** Compará contra el techo teórico (regla 3.16).
   Un peldaño estuvo 1700+ generaciones clavado pidiendo *más que la
   perfección* y el log no lo decía.
2. **¿Cuántas capacidades le estás pidiendo a la vez?** Separarlas en dos
   escalones destrabó tres peldaños "imposibles" al instante: llegar al peldaño
   de esquivar pasó de la generación 320 a la 38.
3. **¿Hay margen que ganar ahí?** Si el clásico ya saca el 98.8% del techo, no
   hay nada que aprender en ese peldaño, por más generaciones que le des.

Recién si las tres dan bien, el estancamiento es información sobre el genoma.
**Un peldaño mal planteado no se arregla con más generaciones**, y moler contra
él no es cumplir esta regla: es confundir obstinación con rigor.

Corolario del usuario, dicho explícito: **esta biblia describe el proyecto, no
lo gobierna.** Si una regla de acá quedó desmentida por una medición, gana la
medición y la regla se corrige en el mismo cambio. Lo único que no se negocia
es la vara honesta (3.2) y el contrato del residuo (3.15), porque son el
instrumento con el que se sabe si algo funcionó: aflojarlos no mejora el
proyecto, lo deja ciego.

### 3.8 Aprobar un peldaño no sirve de nada si después se olvida

El currículo tiene un agujero que sólo se ve si se lo mide, y cuando se lo midió
estaba abierto de par en par: el campeón había aprobado frenar, rumbo, ir a B y
sensores duros, y para cuando peleaba esquivar **ya no sabía hacer ninguno**.

```text
  gen 92  TRABADO en «peldaño 7: esquivar» hace 50 generaciones
        └ ⚠ DESAPRENDIÓ los peldaños [1, 2, 3, 4, 5, 6]
```

La causa es de diseño: la aptitud del peldaño N sólo evaluaba las misiones del
peldaño N, así que **nada castigaba olvidar lo anterior**, y la evolución
optimiza únicamente lo que se mide. El arreglo es `mision::examen_de_repaso`:
cada peldaño anterior sigue tomando examen, en fracción de su propio umbral.

**El examen de repaso NO cierra el agujero. Lo achica.** Medido de dos formas:

1. **El `min` sólo presiona el término que hoy es el peor.** La aptitud es
   `min(misiones propias, examen_de_repaso)`. Si el peldaño actual va muy mal,
   ES el mínimo, y **romper un peldaño viejo no mueve el número** hasta que ese
   peldaño viejo caiga por debajo del actual. Medido: `DESAPRENDIÓ los peldaños
   [4]` sostenido **1100+ generaciones con la protección puesta**, con el
   peldaño 5 al 19% de su umbral — o sea 81% de margen para degradar gratis.
2. **El margen del repaso podía ser más grande que toda la vara.** Arreglado: el
   piso del repaso es `max(umbral(p) × MARGEN_REPASO, clásico(p))`, con prueba
   (`el_piso_del_repaso_nunca_baja_del_clasico`). Por la identidad de 3.15,
   quedar debajo del clásico significa que la red ESTORBA.
   Vale en las dos tareas: en persecución desde 7.34 y en las misiones desde
   7.44 (`mision::PISO_CLASICO_REPASO`: 95% de lo que saca el clásico en cada
   misión vieja).

La regla general: **una escalera de currículo sin examen de repaso no construye,
reemplaza.** Si al subir un escalón no se sigue exigiendo el anterior, no se
acumula capacidad — se cambia una por otra, y al final quedan cero.

**Y el corolario, que costó equivocarse DOS VECES con el mismo error en dos
ejes distintos:** el examen tiene que cubrir **exactamente** lo que se exigió
para aprobar, en todos los ejes. Primero tomaba una sola misión de cada peldaño
(el genoma aprendió esa y olvidó las otras dos); arreglado eso, volvió a
aparecer porque el examen corría sólo con `Ruido::REAL` mientras aprobar exigía
LIMPIO y REAL. **Un examen más angosto que la capacidad que dice proteger no
protege nada, enseña a pasar el examen.** Si se toca `examen_de_repaso` para
abaratarlo —y da ganas, porque es lo que más encarece el entrenamiento— esto es
lo primero que hay que volver a verificar.

Señal secundaria que vale anotar: **cuando deja de olvidar, lo que sigue le sale
casi solo.** Los peldaños 3 y 4 los aprobó en dos generaciones seguidas, contra
las decenas que costaba antes cada uno.

**Éste sigue siendo el bloqueo real del proyecto**, no la mecánica de ninguna
maniobra. Detalle y costo medido del examen: bitácora, deuda 0.

### 3.9 Lo descartado se vuelve a medir cuando cambian las condiciones

Un "no sirve" es una medición, y toda medición se hizo **bajo condiciones**.
Cuando esas condiciones cambian, la conclusión no se hereda: **caduca**.

El caso que obligó a escribir esta regla: el subcircuito de la mosca estaba
apagado porque medía peor que IAFZ-64 solo, y de ahí se concluyó que la
topología real no aportaba. **Todas esas corridas se hicieron con el código de
sensores binario y las salidas con tanh saturada** — las dos cosas que después
se descubrieron rotas. La conclusión se sacó midiendo con el instrumento roto.
Peor: el documento de arquitectura ya listaba como primera causa candidata *"la
codificación de rumbo puede ser demasiado pobre"*, que es exactamente el defecto
que después se encontró. La respuesta estaba anotada como sospecha y se quedó
ahí un año porque nadie volvió a mirarla cuando el contexto cambió.

En la práctica: cada vez que se toque el código de sensores, el mapeo de
salidas, la función de aptitud o el currículo, **hay una lista de conclusiones
viejas que quedaron sin respaldo**. No hay que re-medirlas todas de inmediato,
pero sí **marcarlas como caducadas en el mismo cambio**, y no seguir citándolas
como si valieran.

### 3.10 (retirada) Buscar la conexión entre las dos mitades del proyecto

**Retirada en la auditoría (bitácora 7.10). El número no se reusa**, porque el
código cita reglas por número y reciclarlo haría que una cita vieja apunte a
otra cosa.

Decía que había que forzar cada tanto la pregunta "¿el órgano biológico que ya
tenemos hace justo la tarea en la que estamos trabados?". Se retira porque su
objeto de estudio ya no existe: el subcircuito del conectoma de *Drosophila* se
midió con el instrumento arreglado y tres semillas, se falseó, y el código se
borró del repo (bitácora 7.9). Dejarla en la lista de "no se negocia" invitaba a
re-litigar un resultado negativo que ya está cerrado con evidencia.

**Su parte viva sobrevive en 3.17**: mirar lo que YA está en el repo antes de
traer algo de afuera. Eso sigue valiendo — sólo que sin una mosca adentro.

### 3.11 `salida=` no es opcional en un script de experimento, NUNCA

Pasó, después de haber escrito la regla que lo prohibía: un script de
comparación corrió `entrenar3d 60 robusto semilla=2026` **sin `salida=`**, y eso
escribe por defecto en `genomes/DRON3D_64.adn` — el genoma principal, el que
sirve el visor. Se recuperó porque estaba commiteado un paso atrás; si no, se
perdía sin aviso, en medio de un experimento que no tenía nada que ver con él.

La lección no es "tener cuidado la próxima vez" — eso ya se sabía y no alcanzó.
Es: **todo comando de un script de experimento que pueda escribir un archivo
compartido lleva `salida=` explícito, sin excepción, aunque el experimento no
tenga intención de tocar ese archivo.**


### 3.12 Verificar en el visor ANTES de anunciar, no después

Esto costó tres rondas en una sola sesión y es el error más caro que se cometió
acá:

- Se anunció "el dron despega, entrega y esquiva" — **pero el visor cargaba el
  genoma viejo**, incompatible con el código nuevo. La medición era cierta; lo
  que se mostraba, no.
- Se volvió a anunciar. Esta vez el objetivo por defecto del visor era el MISMO
  punto que la partida: el dron no tenía ninguna tarea y parecía roto.
- Se volvió a anunciar. Esta vez el usuario estaba mirando el **piloto clásico**
  estrellarse contra un poste, que es lo que ese piloto hace siempre.
- Y debajo había un bug real: `web.rs` sobreescribía `Mundo::obj` con el tramo
  actual en cada paso de física, y el tramo de posada tiene z=0, así que al
  terminar una entrega **el objetivo quedaba clavado a ras del suelo para
  siempre**.

**Antes de decir "mirá cómo funciona", confirmá los CUATRO.** Los tres primeros
fallaron uno por vez en esa sesión; el cuarto apareció después y nadie lo había
pensado nunca:

1. **qué genoma está cargado** (`md5sum`);
2. **cuál es el objetivo real** (si coincide con la partida, no hay tarea);
3. **qué piloto está activo** (si dice CLASICO, no estás mirando la red);
4. **qué traducción de salidas usa** — `web.rs` volaba `consignas_red` (la red
   REEMPLAZA al piloto) mientras el gym y el informe usaban
   `consignas_hibridas_punto` (la red CORRIGE). Los mismos bits leídos con dos
   significados distintos. Con la traducción correcta, el mismo genoma pasó de
   "da vueltas a 4.35 m" a "se queda a 0.90 m". Ver bitácora 7.7.

El comando que chequea los cuatro está en la sección 5.

### 3.13 Un número clavado a mano es deuda del día en que cambia la física

Al pasar el aparato de "0.8 kg, 4 m/s" a un quad de persecución real (31.8 m/s
de techo), el guiado clásico —con modelo del aparato y CERO ruido de
aprendizaje— se estrellaba en 4 a 6 de 6 recorridos. Las ganancias de
`Estabilizador3` estaban escritas a mano para la inercia vieja; reescalarlas no
alcanzó. El piso de compensación de gravedad venía de la actitud extrema vieja;
corregirlo tampoco alcanzó solo.

**La telemetría, no el razonamiento, encontró la causa:** trazando el lazo de
actitud AISLADO converge sin oscilar — el problema no estaba en
`Estabilizador3`. Trazando `Guiado::consignas_persiguiendo`, el pitch oscilaba
0.45↔1.5 rad cada ~0.8 s: un ciclo límite genuino. `actitud_desde_velocidad`
convertía error de velocidad en ángulo con ganancia lineal fija, que es la
aproximación de ángulo chico — válida a 4 m/s (~6°), **falsa a 25 m/s** (~68°).
El arreglo fue separar TRIM (el ángulo que la física de arrastre dice que hace
falta, `sim3d::angulo_de_crucero`) de CORRECCIÓN (un P chico sobre el residuo):
16-17 caídas de 18 antes, **0 de 18 después**.

Las dos reglas generales:

- **Una ganancia "afinada a mano" es una medición bajo condiciones, como
  cualquier otra (3.9)** — y las condiciones eran la inercia Y el rango de
  ángulos operado. El problema no era la constante de proporcionalidad: era que
  la relación dejó de ser lineal en el rango nuevo.
- **Cuando dos arreglos razonados no alcanzan, el tercero no se adivina: se
  aísla el lazo por partes y se traza cada una por separado** hasta encontrar
  cuál oscila. Sin esa traza se podría haber seguido ajustando ganancias del
  lazo equivocado indefinidamente.

### 3.14 Todo tiene que entrar en un chip barato — pero el chip es un ESP32, no un Cortex-M0

El destino no es una GPU ni una Raspberry Pi: es un microcontrolador barato
arriba del dron. Eso es lo que justifica el kernel binario entero — XNOR +
popcount sobre 128 bits, sin coma flotante en las capas, 11.4 KB de genoma que
vive en flash y no en RAM. **Ese principio no se toca.**

**Lo que SÍ cambió, por decisión explícita del usuario en la auditoría:** el
piso de hardware es **clase ESP32** (240 MHz, FPU, dos núcleos), no un Cortex-M0
de 48 MHz. Su palabra: *"menos de eso no vamos a meterle la IA en un chip porque
nos limitaría mucho"*. El nombre de la regla igual sobrevive —un ESP32-C3 pelado
cuesta del orden de dos dólares—, pero **el número del presupuesto no**: estaba
calculado contra un chip que no es el que se va a usar, y por la regla 3.9 esa
conclusión caduca cuando cambian las condiciones.

**Cómputo, medido** (`persecucion::costo_de_computo`): una decisión completa
—sensores + termómetro + red + guiado con modelo— cuesta **5.8 µs** en un núcleo
de escritorio. Contra el M0 supuesto (~100× más lento) daba 17× de margen;
contra un ESP32 (del orden de 30× más lento que un núcleo de escritorio para
este trabajo) da **~175 µs contra los 10 ms de decidir a 100 Hz: ~57× de
margen**. Esa cuenta era aritmética bajo un supuesto. **Medido sin el chip el
2026-09-26** (`tools/chip/medir.sh`, bitácora 7.50): el cerebro de a bordo
(`csrc/iafz_64_abordo.c`: sólo inferencia, sin librerías, 1.4 KB de programa
más los 11.4 KB del genoma en flash) compilado para RISC-V de 32 bits con FPU
y corrido en un emulador cuesta **~28.700 instrucciones por decisión** (1.015
sumas flotantes) y da EXACTAMENTE lo mismo que el kernel del gym (prueba
`abordo::igual_al_kernel_del_gym`). A 160-240 MHz son ~0.12-0.18 ms contra
10 ms a 100 Hz: **55-80× de margen**. Sin FPU (ESP32-C3) las sumas van por
software: ~80-110 mil instrucciones, 14-20× de margen (esto sí estimado).
Falta medir ciclos en el Xtensa del ESP32-S3 con el toolchain de Espressif.

**Popcount en el chip:** ni Xtensa ni RISC-V sin Zbb (ESP32-C3) tienen
instrucción de popcount, así que el cerebro de a bordo lo hace por SWAR (sumas
y máscaras de 32 bits) — ya está incluido en las 28.700 instrucciones medidas.

**Consecuencia práctica, que es lo que la regla vieja estaba tapando: hay
lugar para que el cerebro crezca.** Con ~57× de margen, el presupuesto ya no es
la razón para no tener una tercera capa ni más salidas. El reparto real del
costo por decisión, contado del kernel (`csrc/iafz_64.c`), ordena las opciones:

| Pieza | Trabajo por decisión | Flash |
|---|---|---|
| proyección de MEMORIA (32 bits × 64 neuronas) | ~1024 sumas flotantes | 8192 B |
| dos capas binarias (64 + 64 neuronas) | 256 popcount de 64 bits | 2176 B |
| proyección de SALIDAS (4 × 64) | ~128 sumas flotantes | 1024 B |

La memoria recurrente se lleva el **89% del trabajo flotante** y el **72% del
flash**. Las capas binarias son baratas y las salidas son lo más barato que hay:
**una salida más cuesta 256 bytes y ~32 sumas** — o sea que "sólo cuatro
salidas" nunca fue una restricción de presupuesto (ver 3.1: son cuatro porque
son las cuatro consignas que acepta un controlador de vuelo real, no porque no
entren más).

**Flash**: 11392 bytes por genoma, medido en disco.

**Y el presupuesto que de verdad está apretado, que faltaba en esta lista:
los BITS.** El kernel tiene 128 bits de contexto: **96 de sensores
(`iafz::SENSOR_BITS`) y 32 de memoria**. Ése es el recurso escaso del proyecto
—no los µs, que sobran— y es el que hoy bloquea la idea del anillo de
proximidad (la visión de obstáculos usa 13 de 96 bits y un anillo cuesta
bastante más). Las reglas:

- **No hay bits gratis.** Un campo nuevo se paga sacándoselo a otro, y **qué se
  pierde se mide, no se supone**. El reparto completo, con la medición de cada
  decisión, está en el comentario de `gym3d::sensores_guia`, "El presupuesto de
  96 bits, y por qué se movió".
- **En termómetro un campo cuesta ~3× más bits que en binario.** Es el precio de
  la regla 3.6 y está pago a propósito: sin eso el kernel no puede representar
  "estoy cerca".
- **Ensanchar los 96 bits invalida TODOS los genomas**, igual que cambiar el
  código de sensores (sección 4). No es una migración, es un reentrenamiento.

**Idea pendiente, con el motivo escrito (regla 3.19), del 2026-09-21: agrandar
la red directamente (una capa más, más neuronas) en vez de entrenar algo más
grande en la compu y después "pasarle los pesos" a la red chica.** Lo segundo
no es viable tal cual se pensó: las capas binarias son matrices de un tamaño
fijo, evolucionadas por mutación, no por gradiente — no hay un "cargar los
pesos" de una red de otra forma a otra sin inventar antes una destilación o
poda que hoy no existe para este kernel. Lo primero sí es viable porque el
presupuesto de CÓMPUTO del chip ya tiene ~57× de margen (arriba en esta misma
regla): una red más grande cabría entera en el ESP32 sin truco de compresión.
**No se hace ahora porque nada de lo medido hasta hoy (7.33, este A/B) señala
que falte capacidad** — los atascos fueron de crédito compartido entre celdas
y de ruido de instrumento, no de una red que se quede corta. Agrandarla ahora
agrandaría el espacio de búsqueda evolutiva justo cuando el problema ya es que
no converge — probablemente empeoraría el síntoma actual, no lo arreglaría.
Se retoma el día que haya evidencia MEDIDA de que la red satura o le falta
representar algo, no antes.

**Esto NO aplica a `Estabilizador3`.** Por la regla 3.1 ese archivo se borra al
pasar a hardware y lo reemplaza el controlador de vuelo comercial, que corre en
su propio micro (clase F4/F7) y que cualquier dron ya tiene. El presupuesto del
chip barato es para el CEREBRO: sensores → termómetro → red → consignas.

### 3.15 Las ecuaciones que ya conocemos van programadas, no evolucionadas

La evolución es cara y es mala redescubriendo física de manual. Si una relación
se puede escribir en una línea a partir del modelo del aparato, **se escribe**, y
lo aprendido se ocupa de lo que el modelo no captura.

No es una preferencia estética, es una medición doble de la misma sesión:

```text
  guiado clásico SIN el trim físico   16-17 caídas de 18     (inservible)
  guiado clásico CON el trim físico    0 caídas de 18, 81-100% a tiro
  red evolucionando desde cero        892 de 7161 de aptitud (no encontró nada)
  red corrigiendo sobre el modelo   12420 de 12600, en 60 generaciones
```

La relación velocidad→ángulo es fuertemente no lineal (6° a 4 m/s, 68° a
25 m/s). Es una ecuación conocida —`sim3d::angulo_de_crucero`, tres líneas— y
pedirle a una red binaria de 96 bits que la encuentre sola por selección natural
gastó todo el presupuesto evolutivo sin resultado. Puesta la ecuación, la misma
red con la misma semilla pasa a mejorar al modelo en 60 generaciones.

El corolario que cambia cómo se leen los números: **con la red en cero, el
piloto híbrido ES el piloto clásico**. Entonces el piso de aptitud ya no es "un
dron que no sabe volar" sino la vara honesta entera, y por eso el umbral de
aprobación sube a *superar* al clásico (`SUPERAR_AL_CLASICO`, 1.02×) en vez de
alcanzar una fracción de él. La regla 3.2 sigue intacta: la vara no se toca para
que los números queden lindos.

**Esa identidad es además una herramienta, no sólo una propiedad**: apagar la
red da el piloto clásico exacto sin duplicar la cadena de control. Se usa en
`sin_nada_que_esquivar_es_exactamente_el_clasico` y en
`capacidades::volar_con(..., usar_red: false)`. Cada vez que necesites la vara
en un contexto nuevo, apagá la red — no escribas un segundo piloto.

**Y el corolario de entrenamiento, que costó dos corridas trabadas debajo de
la vara (bitácora 7.54): el residuo NACE en cero.** Una población al azar, o
sembrada con un genoma de otra época, arranca DEBAJO del clásico y puede no
encontrar nunca el «no corregir» que ya vale más. Toda corrida nueva se
siembra con `dron residuocero` + `sembrar=RUTA` (y `sembrartodo`), como el
aprendizaje de política residual de la industria.

Y la afirmación que se puede hacer sobre el proyecto se vuelve más chica y más
honesta: **la red no pilota, corrige a quien pilota.** Hay que decirlo así.

### 3.16 Un umbral de aprobación tiene techo, y hay que calcularlo

Consecuencia directa de subir el umbral a "superar al clásico": en el peldaño
más fácil de persecución, el clásico ya sacaba 12353.3 sobre un **techo teórico
de 12500** (el 98.8% de la perfección absoluta). El umbral (`clásico × 1.02`)
pedía 12600.3: **más que la perfección**. El entrenamiento estuvo 1700+
generaciones clavado porque le estaba pidiendo algo matemáticamente imposible, y
nada en el log lo decía — el cartel de "TRABADO" es idéntico para "esto es
difícil" y para "esto no se puede".

La regla: **cualquier umbral que se calcule como "más que X" tiene que
compararse contra el techo teórico antes de instalarse**, porque "más que X" no
tiene ningún motivo para quedar por debajo del máximo posible — depende de qué
tan cerca de ese máximo esté X. Acá el techo se calcula fácil
(`persecucion::techo_teorico`) y el umbral queda en
`min(clásico × 1.02, techo × 0.995)`. Hay prueba
(`ningun_umbral_pide_mas_que_la_perfeccion`) que lo verifica para todo el
currículo.

### 3.17 Buscar cómo ya lo resolvió la industria, ANTES de que el usuario tenga que pedirlo

Pasó una vez y no puede volver a pasar: la persecución se estrellaba en curva, y
la investigación de cómo lo resuelven DJI, Skydio y los autos autónomos sólo
arrancó porque el usuario tuvo que decirlo explícitamente. Su palabra exacta:
*"si yo no te lo decía, tú nunca lo intentaste, nunca se te ocurrió"*. Tiene
razón, y es más caro que cualquier bug — un bug se arregla en una tarde y este
hábito, si no se corrige, se repite en cada problema nuevo.

**La regla:** cuando algo no funciona y el proyecto no tiene una respuesta
propia, buscar primero cómo lo resuelve quien ya lo vende funcionando —empresas
del rubro, papers, hasta videojuegos si aplica— **antes** de seguir ajustando
parámetros a ciegas o de esperar a que el usuario lo pida. No es "consultar si
hay dudas": es el primer paso, no el último recurso.

**Y antes de salir a buscar afuera, mirá lo que YA está en el repo** (esto venía
de la vieja regla 3.10, y es lo único de ella que sobrevivió a que la mosca se
borrara): la solución a un problema trabado suele estar en una pieza que el
proyecto ya tiene y que nadie conectó. El termómetro comprimido que arregló el
kernel es un código poblacional; la identidad "red en cero = clásico" resultó
ser una herramienta de medición y no sólo una propiedad. Las dos estaban
adentro del repo antes de que nadie las usara para eso.

**La segunda mitad, que importa tanto como la primera: no alcanza con imitar,
hay que evaluar si se puede superar.** El usuario lo pidió explícito: *"que nos
mejoren y que nos hagan mejor que ellas, si es posible"*. Adoptar la técnica de
la industria es el piso, no el techo.

Casos donde ya pagó: el *iterm relax* de Betaflight (la I del lazo de giro
rompía la vara hasta que se copió cómo lo resuelven ellos — y copiarlo mal,
por magnitud en vez de por la parte rápida, la anulaba: bitácora 7.37), la
prealimentación de aceleración (3.15), el aterrizaje de
precisión de PX4 —alinear ANTES de descender, que arregló la vara honesta—, y
sumar las amenazas de todos los obstáculos visibles en vez de quedarse con el
peor (*potential fields*, y el propio diagnóstico público de por qué Skydio
mapea en vez de reaccionar).

### 3.18 El instrumento de medición también se audita

Regla nueva, de la auditoría (bitácora 7.10), y la más cara de las que faltaban:
**es la tercera vez que el proyecto saca una conclusión con un aparato de medir
roto.**

1. **El veredicto del conectoma** se midió con el código de sensores binario y
   las salidas saturadas — las dos rotas. Costó repetir el experimento un año
   después (3.9).
2. **`telemetria_estancado`** evaluaba al campeón como piloto monolítico
   mientras la aptitud lo medía dentro de la mezcla: reportaba 11951.0 donde el
   número real era 12182.6. *Un diagnóstico que contradice al número que dice
   diagnosticar es peor que no tener diagnóstico.*
3. **`capacidades`, capacidad 3** («¿se queda quieto?») acumulaba la distancia
   AL OBJETIVO y la imprimía como «deriva». No medía quietud en absoluto: un
   dron orbitando a 0.89 m de radio puntuaba igual que uno clavado. Al
   arreglarlo, el genoma pasó de **3/3 a 0/3** y el clásico marcó 3/3 quedándose
   a 1 cm del punto.

Las reglas 3.2, 3.5 y la sección 4 apuntan todas HACIA los instrumentos —la vara
es sagrada, medí no argumentes, cerrá con `capacidades`—. Ninguna apuntaba A los
instrumentos. Lo que exige ésta, antes de creerle a un número:

- **Verificá que la función calcule la cantidad que su nombre dice.** El bug 3
  estuvo meses a la vista: la bitácora 7.8 imprimió «deriva 0.89 / 0.89 / 0.89» al lado de
  «queda a 0.89 m» y nadie preguntó por qué eran idénticos. **Dos métricas que
  dan el mismo número dígito por dígito son la misma métrica**, no dos
  confirmaciones independientes.
- **Ningún umbral de aprobación se elige a gusto: se calibra contra la vara
  honesta.** `QUIETO = 0.50 m` quedó 25× más permisivo que el 0.02 m del
  clásico, a propósito, para que reprobar no pudiera ser un artefacto de haber
  apretado la vara. Apagar la red da la vara gratis (3.15).
- **Un criterio de aprobado con tolerancia fija es un imán.** Si «llegar» es
  «estar a menos de 1 m», la evolución encuentra 0.9 m y se queda ahí, porque
  más cerca no paga. Medido: las cinco rutas terminan entre 0.89 y 0.95 m, y el
  clásico entre 0.30 y 0.49. No se arregla bajando la tolerancia a mano (eso es
  mover la vara) — se arregla si la aptitud sigue pagando por acercarse adentro
  del radio, **y hay que imprimir el número crudo al lado del veredicto** para
  que el imán se vea.
- **Cuando el instrumento cambia, todo lo que midió queda sin respaldo** (3.9),
  y las tablas viejas se marcan en el mismo cambio.
- **Estresar la VARA, no sólo al genoma: `dron estres N`.** La vara no
  aprende, así que todo lo que la rompe bajo una avería real es un defecto
  del modelo o del controlador — nunca «el genoma no aprendió». La primera
  corrida encontró cuatro defectos que ninguna prueba medía (guiñada sin I,
  aterrizaje que dependía de la masa exacta, simulador sin mezclador de
  motores, y una avería de «GPS congelado» que no tenía sentido en un dron
  sin GPS: bitácora 7.38). Correrlo cada vez que
  se toque `sim3d`, `pid3d` o la aproximación.

### 3.19 Leer las reglas siempre. Y lo que ya existe, se conecta.

Regla del usuario, sobre cómo se trabaja acá, no sobre el dron:

**Leer este archivo entero antes de tocar nada, en cada sesión, sin excepción.**
Es la regla 0 — las demás no sirven si ésta no se cumple primero.

**Si dos piezas ya construidas obviamente deberían estar conectadas, se
conectan.** No se deja una capacidad terminada sin usar porque nadie preguntó
explícito. El caso que la disparó: `repertorio.rs` (un genoma por nicho, MAP-
Elites) existía, probado y verificado en vuelo, mientras el entrenamiento del
gym seguía siendo un solo genoma peleando el olvido catastrófico con el mismo
mecanismo de siempre — dos piezas del propio proyecto sin conectar, cuando la
conexión era obvia.

**La única salida es una razón escrita, no el silencio.** Si algo obvio NO se
conecta, tiene que decir explícitamente por qué —costo, riesgo medido, orden
de prioridad— en el mismo lugar donde se nota el hueco. Un hueco sin
explicación no es una decisión, es un olvido.

### 3.20 Ninguna decisión automática se toma con UNA muestra

La sección 4 ya decía "una sola semilla no decide nada" — para las decisiones
que toma **una persona**. Faltaba la mitad que importa más: **las decisiones
que el código toma solo**, miles de veces, sin que nadie las mire.

Costó tres veces el mismo error (bitácora 7.30), en tres lugares distintos, y
las tres veces el síntoma fue una conclusión falsa que se sostuvo sola:

| Dónde | Qué aceptaba con una muestra | Qué costó |
|---|---|---|
| `gym3d::entrenar` | cruzar el umbral en UNA generación | "15/15 en la generación 100" que no existía; diez generaciones después, −96.6 |
| `repertorio::refinar_nicho` | ganarle al ancla en UNA misión | una semilla de cuatro terminó 760 puntos PEOR que donde arrancó |
| `vivo.rs` | ganarle al ancla en UNA ventana de 4 s | sin medir todavía — y es el que corre EN VUELO |
| `gym3d::entrenar` (la confirmación misma) | aprobar con UNA confirmación extra | «aprobó esquivar en gen 16» aprueba en 1/12 sorteos reales (bitácora 7.33) |
| `gym3d::CONFIRMACIONES = 3` | tres muestras, contra un guardián consultado 425 veces por corrida | «aprobó esquivar en gen 933» aprueba en 3/12 sorteos reales (bitácora 7.34) |
| `gym3d::CONFIRMACIONES = 6` | seis muestras, calculadas para p = 0.16; un módulo con p ≈ 0.55 las pasa el 2.8% de las veces | el módulo posar «aprobó posarse en gen 10» y aprueba 6-7/12 sorteos (bitácora 7.58); hoy son 12 |

Los seis están arreglados y los cuatro primeros se arreglan igual: **pedir más
de un sorteo antes de creerle**. `aprobo` exige `CONFIRMACIONES = 6` escenarios
independientes, `LOTE = 4`, `MUESTRAS_PARA_PROMOVER = 2`. Hay una prueba
(`ningun_ascenso_se_decide_con_una_sola_muestra`) que falla si alguno baja.

**La cuarta fila es la que más duele, y por eso queda escrita aparte.** No es
un bug nuevo: es el MISMO bug, ocurrido DENTRO del código que se escribió para
arreglar el primero (7.30), después de que esta regla ya existiera con tabla
y todo. La confirmación de "no aprobar por un solo sorteo benévolo" pedía,
ella misma, un solo sorteo de confirmación. Nadie se preguntó cuántas muestras
tenía el propio mecanismo de blindaje. La lección que hay que aplicar la
próxima vez que se escriba un `if` de este tipo: **la pregunta "¿cuántas
muestras tiene esto atrás?" se hace SOBRE EL MECANISMO DE VERIFICACIÓN
también, no sólo sobre lo que verifica.** Un guardián de una sola muestra
guarda tan poco como lo que dice guardar.

**Y la quinta fila es un nivel más profundo, no una repetición de la cuarta.**
Ahí las muestras ya eran tres —o sea "más de una", que es lo que las cuatro
filas de arriba piden— y el falso aprobado pasó igual. Lo que faltaba era
notar que **el guardián no se consulta una vez: se consulta cada vez que el
campeón cruza el umbral**, y eso, contado del log, son 425 veces en una corrida
de 1200 generaciones. Medido (bitácora 7.34):

```text
  cruces del umbral en una corrida                    425
  probabilidad de pasar UNA confirmación (medida)    0.16
  falsos aprobados esperados con k=3   0.16³ × 425 = 1.7   (el A/B tuvo 1)
  falsos aprobados esperados con k=6   0.16⁶ × 425 = 0.007
```

Las dos tasas de supervivencia intermedias del log dieron 0.160 y 0.162, o sea
que los sorteos son independientes y la cuenta vale. Y subir de 3 a 6 **no
cuesta nada**: la comprobación se corta en la primera confirmación que falla,
así que la cuarta la paga 1 cruce de 425.

**La regla, que es la que hay que aplicar y no estaba escrita: cuántas muestras
necesita un guardián depende de CUÁNTAS VECES se lo consulta.** No se elige por
"más de una": se elige para que `p^k × consultas ≪ 1`. Un guardián que corre
una vez necesita pocas muestras; uno que corre cientos de veces por corrida
necesita que el falso positivo sea improbable **acumulado sobre toda la
corrida**, no sobre un intento.

**Lo que hay que hacer cada vez que escribas un `if` que acepta, rechaza,
aprueba o asciende algo medido:** contar cuántas muestras tiene atrás. Si es
una, no es una medición — es un sorteo. Y si el número que compara viene de
un escenario sorteado al azar, es un sorteo **dos veces**. Y después contar
**cuántas veces se va a ejecutar ese `if` en una corrida**, porque eso es lo
que decide si las muestras alcanzan.

**El corolario que duele y hay que decir igual:** el aparato de medir mintió
durante una corrida entera y nada lo delató. No lo cazó la batería de
pruebas, lo cazó leer el log de una corrida que se creía terminada. Así que
esto NO se da por cerrado con los arreglos que haya: cada vez que aparezca un
resultado bueno y limpio, la primera pregunta es **"¿cuántas muestras tiene
esto atrás?"**, antes de festejarlo. La quinta fila se cazó exactamente así,
y otra vez leyendo el log de una corrida que se creía terminada.

### 3.21 Hacer, no narrar

Regla del usuario, sobre cómo se reporta. **Es inquebrantable.**

**No se gastan tokens contando lo que se va a hacer.** Nada de anunciar el
plan, enumerar opciones que no se van a tomar, ni ir describiendo cada paso
mientras se da. Se hace el trabajo, y al final se reporta: qué se midió, qué
dio, qué quedó roto y qué falta.

Su palabra: *"concentres lo que nos queda en hacer el trabajo antes que en
contar lo que vas a hacer"*. El motivo es de presupuesto —el contexto y los
tokens son finitos, y narrar consume los dos— pero el efecto secundario es
mejor todavía: un informe al final se puede contrastar con los números;
un relato en tiempo real no.

**Lo que esta regla NO prohíbe:** checkpoints cortos cuando una corrida es
larga, y preguntar una decisión de producto que el usuario no dio (sección
5). Un checkpoint es una línea con un número adentro, no un párrafo.

**Y lo que esta regla NO autoriza:** esconder un problema para que el informe
quede prolijo. Las reglas 3.4 y 3.12 mandan sobre ésta — si algo se rompió o
quedó sin verificar, se dice, aunque alargue el informe.

### 3.22 Si una regla traba el trabajo, se abre una excepción ESCRITA

Regla del usuario, pedida explícita: *"si alguna regla se opone en tu contra
en lo de tu trabajo, crea una cápsula para ese caso en específico, porque no
quiero que te limites"*.

**Ninguna regla de este archivo es motivo para no hacer el trabajo.** Si una
traba algo que hay que hacer, no se frena y no se pide permiso: se abre la
excepción, se hace, y **se escribe la excepción con su motivo en el mismo
cambio**, acá o al lado de la regla que se está exceptuando. Una excepción sin
escribir es una regla rota; una escrita es una regla que aprendió un caso.

Ya pasó dos veces en este archivo y las dos veces la excepción mejoró la
regla: 3.2 tuvo que aclarar que arreglar un BUG de la vara no es bajarla, y
3.7 que cambiar la arquitectura que causó el bloqueo no es avanzar con la
fase anterior incompleta — esa segunda aclaración destrabó el proyecto
entero.

**Las dos que NO admiten excepción** siguen siendo las de siempre, y por el
mismo motivo: 3.2 (la vara honesta) y 3.15 (el contrato del residuo). No son
reglas de trabajo, son el instrumento con el que se sabe si algo funcionó —
exceptuarlas no destraba nada, deja ciego.

**Y una excepción no es una licencia para maquillar:** 3.4 (no se maquillan
resultados) y 3.12 (verificar antes de anunciar) mandan sobre ésta.

---

## 4. Cómo se mide acá (o cómo perder 40 minutos)

- **La aptitud es el PEOR caso**, no el promedio: `aptitud_robusta` toma el
  mínimo sobre 3 escenarios × 3 niveles de ruido. Un cambio que mejora 8 de 9
  casos y no toca el peor **no mueve el número**. Pasó: tres variantes del bono
  de aterrizaje dieron 256.0 idéntico dígito por dígito, porque ninguna se
  activaba en el caso que definía el mínimo.
- **Cuando el campeón se clava en el mismo número, mirá los nueve, no el
  mínimo.** `cargo run --release -- mision N 0 detalle` —con **N del 1 al 15,
  1-based, igual que `sorteos` y `celdas`**; era 0-based sólo en `mision` y esa
  trampa ya costó un diagnóstico equivocado— imprime la tabla de
  3 misiones × 3 ruidos. La primera vez que se usó, el campeón estaba clavado
  40 generaciones y la tabla lo explicó en un segundo:

  ```text
    misión         limpio       real       duro
    1               986.3      969.9       20.0
    2                -7.4       -0.5       10.9     <-- objetivo DETRÁS
    3              1057.9     1064.9     1000.5
  ```

  Cumplía dos de tres con mil puntos y sacaba −7.4 en la otra **con sensores
  perfectos**, así que no era ruido: era la única misión cuyo objetivo estaba
  detrás del dron. No estaba estancado en todo, estaba estancado en uno.
- **"El valor no se movió en N generaciones" NO prueba que esté muerto.** Es la
  trampa más tentadora, porque un número clavado *parece* una conclusión.
  Medido, las dos caras:

  ```text
    valor clavado  650 generaciones  →  se destrabó SOLO y siguió subiendo
    valor clavado 1880 generaciones  →  lo maté yo diciendo "convergencia total"
  ```

  La diferencia es de grado, no de naturaleza. Si vas a cortar una corrida
  larga, que sea por **costo de oportunidad declarado** —"prefiero gastar estas
  horas en otra cosa"— y no disfrazado de diagnóstico técnico. (Pero antes de
  moler, pasá las tres preguntas de 3.7: el peldaño puede estar mal planteado.)
- **Casi siempre el que define el mínimo es el ruido DURO, no el escenario.** Es
  el precio de la aleatorización de dominio y está bien que se pague: el número
  sólo significa algo afuera si vale en el peor sensor. Pero conviene saberlo
  antes de salir a buscar un problema de navegación que no existe.
- **Cuando el entrenador se traba, se diagnostica SOLO.** Cada 50 generaciones
  de estancamiento, `mision::telemetria_estancado` contesta las tres preguntas
  que destraban: **cuál de los nueve casos arrastra el mínimo**; **cómo falla**
  (no es lo mismo chocar un poste que quedarse sin reloj a 3 m que posarse
  afuera de la plataforma); y **si está desaprendiendo lo de atrás**.
- **Y si el problema no aparece ahí, mirá lo que EMITE el cerebro**:
  `mision N 0 salidas` imprime media, mínimo, máximo y % de saturación de las
  cuatro salidas crudas. Así se encontró la salida de altura clavada en 6.61 y
  saturada el 81% del tiempo — un candado del que la evolución no podía salir
  porque ahí la derivada de la tanh valía 0.018.
- **Y eso vale también para las decisiones que toma el CÓDIGO solo, no sólo
  para las que tomás vos.** Es el error más caro de la bitácora 7.30, y estaba
  en dos lugares a la vez: el entrenador aprobaba un peldaño por cruzar el
  umbral en UNA generación (con escenarios re-sorteados cada generación, eso
  mide el sorteo — el "15/15 en la generación 100" de 7.29 no existió), y el
  lazo anclado ascendía un aspirante por ganarle al ancla en UNA misión (con
  eso, una de cuatro semillas terminó 760 puntos por debajo de donde
  arrancó). Los dos se arreglan igual: pedir más de un sorteo antes de
  creerle. Si vas a escribir un `if` que acepta o rechaza algo medido,
  preguntate cuántas muestras tiene atrás.
- **Antes de creerle a un «aprobó», corré `dron sorteos`.** Mide el MISMO
  piloto en el MISMO peldaño N veces cambiando sólo el sorteo de escenarios, e
  imprime sorteo por sorteo más «aprueba en K de N». Tres formas, y las tres
  hacen falta:

  ```bash
  dron sorteos 9 genoma=RUTA n=12   # un genoma suelto
  dron sorteos 9 rep=RUTA    n=12   # un REPERTORIO (el banco entero, no una celda)
  dron sorteos 9 clasico     n=12   # la vara honesta, misma escala, mismos sorteos
  ```

  La tercera es la que más veces faltó: sin ella, «el genoma no lo sabe hacer»
  y «este peldaño no lo aprueba nadie» se ven idénticos. Medido así, el
  clásico aprueba el peldaño 7 en 11 de 12 y el peldaño 8 en **2 de 12**
  (bitácora 7.34).
- **Una sola semilla no decide nada.** La búsqueda evolutiva es caótica:
  cualquier cambio en la aptitud reordena toda la trayectoria. Para comparar dos
  variantes hay que promediar varias semillas. **Una conclusión de una sola
  semilla se escribe como indicio, no como resultado** — buena parte de la
  bitácora es de una semilla y está dicho ahí.
- **La aptitud no es el comportamiento.** Siempre cerrá con
  `cargo run --release -- capacidades`, que mide cosas en castellano. Un genoma
  puede subir de aptitud y empeorar en capacidades. **Y `capacidades` tampoco es
  sagrado: auditalo (3.18)** — una de sus siete capacidades midió la cantidad
  equivocada durante meses.
- **Cambiar la aptitud invalida los números viejos.** Las tablas dejan de ser
  comparables. Actualizalas o marcalas.
- **Cambiar el significado de las salidas invalida los genomas.** Hay que
  reentrenar; no hay migración posible.
- **Y cambiar el CÓDIGO DE SENSORES los invalida todavía más.** Un genoma
  entrenado con el código binario viejo alimentado con el termómetro nuevo no
  vuela mal: vuela al azar.
- **No cambies nada del camino caliente con un A/B corriendo.** *Excepción,
  escrita el día que trabó (regla 3.22):* un A/B se puede **cerrar a
  propósito** —matar las corridas, guardar los logs, anotar qué dieron— y
  entonces el camino caliente queda libre. Lo prohibido es tocarlo con las
  corridas vivas, que es lo que mezcla dos físicas en una misma medición.
  Cerrar una corrida que ya contestó lo que se le preguntó no es perder una
  medición: es terminarla. Pasó: se tocó
  `Estabilizador3` a mitad de un A/B de 8 corridas y las 3 primeras quedaron con
  una física distinta de las 5 siguientes. Hubo que tirar la medición. El camino
  caliente es `sensores`, `consignas_hibridas_punto`, `sim3d`, `pid3d` y la aptitud.
- **`cargo test --release` NO reconstruye `target/release/dron`.** Costó una
  corrida entera: `cargo test` compila el arnés de pruebas, no el ejecutable. El
  síntoma que lo delata es un peldaño trabado reportando **exactamente el mismo
  número** que antes del arreglo.

  ```bash
  cargo build --release            # NO alcanza con cargo test
  ls -la --time-style=+%H:%M:%S target/release/dron src/loquetocaste.rs
  ```
- **La batería de pruebas también es un instrumento, y «pasó» no significa
  nada si no pasa TODAS las veces.** Medido el 2026-09-21: `cargo test
  --release` reprobaba **4 corridas de 6**, siempre la misma prueba y siempre
  en un peldaño distinto, porque `mision::VARIANTE` es un atómico global y las
  pruebas corren en paralelo pisándose los escenarios. Esta sesión reportó
  «105 passed» dos veces sobre corridas que tuvieron suerte. Arreglado con un
  candado (`mision::escenarios_para_pruebas`, que toman las pruebas de
  `mision` **y de `repertorio`**) y, lo que lo cerró de verdad, quitándole a la
  prueba la suposición que la exponía: medía dos veces y comparaba, o sea que
  además de la propiedad medía que nada hubiera cambiado entre las dos. Con
  `aptitud_orden_y_terminos` devolviendo los tres números de una sola pasada,
  **14 corridas seguidas 107/107**. Dos reglas: **un `cargo test` que hay que
  correr dos veces no es una verificación, es un sorteo** (la 3.20 aplicada al
  arnés), y **una prueba que mide dos veces para comparar mide también que
  nada cambió entre las dos** — si la propiedad se puede verificar con una
  sola medición, se verifica con una sola. Si tocás algo que vuela misiones,
  corré la batería varias veces seguidas, no una.

---

## 5. Cómo se trabaja

- **Hacer, no narrar (regla 3.21).** No se cuenta lo que se va a hacer: se
  hace y se reporta al final, con los números. Checkpoints cortos sí, párrafos
  de plan no.
- **Cero human in the loop.** No preguntes para confirmar cada paso; decidí,
  hacé, y reportá con evidencia. Preguntá sólo si es una decisión de producto
  que el usuario no te dio.
- **Diagnosticar antes de tocar.** Instrumentá, mirá la traza, encontrá la
  causa. Los arreglos a ciegas cuestan corridas de 4 minutos cada una.
- **Antes de decir "mirá el visor", correr esto** (los cuatro campos de 3.12):

  ```bash
  curl -s localhost:8080/estado | python3 -c "
  import json,sys; d=json.load(sys.stdin)
  print('piloto :', 'RED' if d['ia'] else 'CLASICO (no es la red!)')
  print('objetivo: (%.1f,%.1f,%.1f)'%(d['ox'],d['oy'],d['oz']))
  print('partida : (%.1f,%.1f,%.1f)'%(d['px'],d['py'],d['pz']))
  print('mision  :', len(d['mision']), 'tramos ·', d['misionEstado'] or 'ninguna')"
  md5sum genomes/DRON3D_64.adn   # ¿es el genoma que creés que es?
  grep -n "consignas_hibridas_punto" src/web.rs src/mision.rs src/capacidades.rs
  ```

  Si el objetivo coincide con la partida, el dron no tiene nada que hacer y va a
  parecer roto. Si dice CLASICO, no estás mirando la red. Si el genoma no es el
  que entrenaste, no estás mirando nada. **Y si el visor y el gym no nombran la
  MISMA función de traducción, no estás mirando el mismo dron.**
- **Los experimentos CORTOS van al scratchpad**, con copia del repo, para no
  contaminar el árbol real ni pisar el genoma bueno. **Las corridas LARGAS no:**
  el scratchpad vive en `/tmp`, que es tmpfs y se vacía en cada reboot —costó
  dos veces los logs y genomas de un A/B entero (bitácora 7.35)—. Una corrida
  de horas va a su propia carpeta `genomes/EXP_AAAAMMDD*/`, con el binario
  copiado adentro (así recompilar no cambia la física a mitad de corrida).
  Y para matarla, `pgrep`/`pkill` con un patrón que no se matchee a sí mismo
  (`dro[n].bin`): `pkill -f` con el patrón literal mata al propio shell. `entrenarmision` y `entrenarpersecucion`
  aceptan `salida=RUTA` justamente para eso (regla 3.11).
- **Se puede diagnosticar SIN esperar a que termine la corrida.** El campeón se
  guarda cada 20 generaciones, así que `mision N 0 detalle genoma=RUTA` sobre el
  archivo de salida dice qué está pasando ahora mismo.
- **Los comentarios del código llevan la medición que los justifica**, con
  números. Es el estilo del proyecto: el porqué vale más que el qué.
- **Lo que se aprende se escribe en el mismo cambio.** Las reglas van a este
  archivo; las mediciones, a `BITACORA.md`. Un hallazgo que no se anota se
  vuelve a pagar.

---

## 6. El mapa del código

| Archivo | Qué hace |
|---|---|
| `sim3d.rs` | Física: cuerpo rígido, **mezclador X con airmode (`mezclar_x`)**, averías (`Averia`, conectadas a las misiones por `mision::ESTRES`), (**aparato de persecución, techo 31.8 m/s**), motores con retardo, ruido de sensores, cámara, obstáculos, **posarse vs. estrellarse** (`tocar_suelo`), `angulo_de_crucero`/`velocidad_techo` (el sobre de vuelo). |
| `codigo.rs` | **Cómo se le habla al cerebro**: termómetro comprimido. Leelo antes de tocar `sensores` (regla 3.6). |
| `mision.rs` | Misiones encadenadas (despegar → ir → posarse → volver), `Seguidor`, la escalera de 15 peldaños, las averías de `dron estres` (`ESTRES`) y las herramientas de diagnóstico (`detalle`, `salidas`). |
| `movil.rs` | El vehículo perseguido: cinemática de bicicleta con límites reales, los 6 recorridos de referencia (`recorridos::todos`) y el manejo en vivo/grabado (`Mando`, `grabacion`). |
| `cabina.html` | La cabina de persecución (`/cabina`): manejar la moto en primera persona, ver la cámara del dron, grabar el manejo. |
| `persecucion.rs` | El gym de PERSECUCIÓN: seguir un `Movil`, no llegar a un punto fijo. Aptitud = fracción del recorrido "a tiro". También `limitar_sacudida`, `visto_desde` (la moto se VE relativa al dron, sin GPS), la clasificación por régimen (informativa) y las pruebas que hacen cumplir 3.14/3.15. |
| `pid3d.rs` | `Estabilizador3` (actitud + cascada de altura, ganancias EN UNIDADES DE INERCIA — ver 3.13), `Guiado` (la vara honesta + velocidad→actitud + `consignas_persiguiendo` con trim de crucero), `consignas_esquivando` y `tope_por_mirada` (la vara con ojos). |
| `gym3d.rs` | Sensores (96 bits, `sensores_guia`/`Guia`), `consignas_hibridas_punto` (salidas→consignas: la red CORRIGE al clásico), `altura_de_aproximacion` (aterrizaje en dos velocidades), bucle evolutivo compartido por misión y persecución. |
| `iafz.rs` | Puente al kernel C de la red binaria. 96 bits de entrada, 4 salidas. |
| `plan.rs` | **Mapa** de lo visto (`Mapa`, fusión con incertidumbre y envejecimiento), **ruta** sobre el mapa (grafo de visibilidad + Dijkstra) y **orden de entregas** (todas las permutaciones hasta 7 cajas). Sólo en tareas dichas (`Mision::planifica`). |
| `pid.rs` | El PID con anti-windup e *iterm relax* (`actualizar_con_tope`). |
| `progreso.rs` | El panel «entrenamiento corriendo» del visor: lee el log de cada `entrenar*` vivo. |
| `crtp.rs` / `csrc/iafz_64_abordo.c` / `tools/chip/` | Hardware: puente CRTP por WiFi para el LiteWing; el cerebro que va en el chip y su medición en emulador. |
| `puente.rs` / `dbpower.rs` / `tools/ojo.py` | Hardware: consignas → MSP, perfil del DBPOWER (sin conectar todavía), percepción desde una cámara real. |
| `capacidades.rs` | La prueba de capacidades en castellano. `volar_con(..., usar_red: false)` da el piloto clásico exacto para calibrar umbrales (3.15/3.18). |
| `vivo.rs` | **Adaptación en vuelo**: búsqueda local anclada mientras vuela. El ancla sólo cae ante algo medido mejor; el mérito se calcula con sensores de a bordo, nunca con la verdad del simulador. |
| `repertorio.rs` | **Un genoma por situación** + **módulo vertical** (`VERTICAL`: la altura en un genoma aparte, que sólo entrenan despegar/posarse; 7.56) (MAP-Elites, Cully et al. Nature 2015). Nueve nichos; aprender en uno no puede pisar los otros — el olvido catastrófico vuelto imposible por construcción. |
| `web.rs` / `visor.html` | El visor. Física en Rust a 200 Hz, Three.js sólo dibuja. |

---

## 7. Estado hoy

> **Éste es el ÚNICO lugar que dice qué es verdad ahora.** La historia de
> cómo se llegó está en `BITACORA.md` (la versión larga de esta sección, con
> todas las tablas viejas, quedó en la entrada 7.40).

**Sin GPS desde el 2026-09-25** (sección 1, bitácora 7.39). **Todo número
anterior se midió con GPS y no se compara con nada de hoy.**

(Ver arriba: persecución retomada el 2026-09-26.)

**Genoma instalado**: `genomes/DRON3D_64.adn` (v5, md5 `4c52dce5…`). Se
entrenó CON GPS y contra la vara vieja: vuela en el visor, pero **no se cita
ningún número suyo**. Se reemplaza cuando un genoma entrenado sin GPS apruebe
`dron sorteos` (≥ 10/12 por peldaño) y `capacidades`.

**Posarse con la física nueva, 96 sorteos (bitácora, 2026-10-10): clásico 64, f1 65, f2 66 — empate (p ≈ 0.9-1.0); la tendencia de n=24 no se sostuvo.**

**Entrenamiento (TERMINADO, 1200 gen; campeones empatan con el clásico, no se instalan: bitácora 2026-10-10)**: `genomes/EXP_20260927d/f1`, `f2` — **física nueva** (efecto suelo + aparato variable por misión, 7.59); la vara ahí aprueba posarse 13/24 y entrega 11/24 — la entrega
con repertorio + **módulo vertical** (altura) + **módulo posar** (todo el
aterrizaje), bitácoras 7.56-7.57. Con sólo el vertical: posarse 1/12 → 5-8/12,
entrega 1/12 → 4-5/12 (clásico 11/12 y 9/12). **Genoma de persecución
instalado** (7.55, md5 `44a27191…`).

**Tareas en entrenamiento: entrega (con repertorio) y persecución.** Ninguna
es EL objetivo (sección 1): son las maniobras con las que se arman las tareas
dichas. La persecución ya anda sin GPS con el clásico (6/6 recorridos al
100%); la red todavía no le gana.

**La escalera tiene 15 peldaños** (bitácoras 7.43-7.44): se agregó «ruta
larga» (14), la entrega (15) usa rutas aleatorias y la entrega con ruido DURO
salió (la vara no aterriza con ruido DURO).

**La vara con la física de hoy (efecto suelo + aparato variable, 7.59), 24
sorteos:** peldaños 1-11 24/24, posarse 13, ida y vuelta 12, ruta larga 12,
entrega 11. **La tabla de abajo es la vieja (aparato nominal): no comparable.**

**La vara (clásico), sin GPS, 48 sorteos por peldaño, aparato NOMINAL:**

```text
  peldaño   1-11   12   13   14 larga   15 entrega
  aprueba    48    45   41      41          40
```

**`dron estres`** (7.43): el peldaño 7 aguanta todas las averías; al posar
sólo cae «40% menos empuje»; en la entrega, 40% menos empuje 10/24 y el
resto 23-24/24.

### Lo que falta, en orden (regla 3.7: cada uno cierra antes del siguiente)

Se actualiza en el mismo cambio que cierra un ítem. Hardware va aparte.

1. ~~Corregir la deriva sin GPS con lo que la cámara ve~~ **HECHO**
   (`sim3d::Baliza`, bitácora 7.42): en 40 m ida y vuelta, 9/24 → 24/24.
2. ~~Calibrar la entrega~~ **HECHO** (bitácoras 7.43-7.44): la vara pasa
   entre el 83% y el 100% en todos los peldaños. Aterrizar con ruido DURO:
   **mejorado sin sensores nuevos** (bajada a divergencia constante, 7.49:
   entrega 36 → 40-41 de 48 vuelos), todavía no alcanza para volver a exigirlo
   en la escalera.
3. **Reentrenar sin GPS** contra la vara de 1-2, con dos semillas, y pasar el
   campeón por `sorteos`, `estres` y `capacidades` antes de instalarlo.
4. ~~El orbitar~~ **HECHO como indicio** (`autoridad_de_sosten`, 7.45): el
   genoma instalado pasó de moverse 0.23-0.46 m a 0.04-0.06 m (clásico
   0.01-0.05). Lo confirma el reentrenamiento.
5. ~~Piso contra el clásico en el repaso de las misiones~~ **HECHO**
   (`PISO_CLASICO_REPASO`, por peldaño y no por misión: 7.44 y 7.47).
6. ~~La tarea se recibe en runtime~~ **HECHO** (`dron tarea`, `/tarea`, caja
   del visor; 7.45): una tarea que nadie entrenó —buscar una caja, llevarla,
   volver— se cumple 9/9 con el clásico, el genoma y en el visor.

7. ~~Mapa, ruta y varias cajas~~ **HECHO** (`plan.rs`, bitácora 7.46): con
   12 postes, 6/36 sólo reactivo → 24-26/36 con mapa. Orden de cajas: óptimo
   hasta 7, «más cercana + 2-opt» con más. **SLAM con anclas** (7.48): lo
   anotado cerca de donde se prendió corrige al dron sin marcas ni sensores
   nuevos; ruido real, 180 m: error propio 8.2 → 3.8 m. Abierto: con ruido
   DURO y sin dos anclas a la vista, el rumbo deriva (el supuesto del
   giróscopo, a medir con la placa).

### Pendiente A PROPÓSITO (decisión del usuario, no deuda)

- **Visión 360°**: el dron es ciego fuera del cono de 160°. *"No tendría
  sentido preparar un anillo de cámaras si ni siquiera puede volar solito con
  una cámara"*. Es además una pregunta de hardware. La vara ya no vuela
  rápido hacia donde no mira (`tope_por_mirada`).
- **Agrandar la red** (regla 3.14): se retoma con evidencia medida de que la
  red satura.

### Hardware (se hace cuando haya placa)

**Qué comprar, decidido el 2026-09-26** (bitácora 7.51): **no** convertir el
DBPOWER (empuje/peso ~1.35 y ~133 g: no le entran el ESP32, el flujo y la
caja; placa no programable; motores con reductora). Primer aparato: **LiteWing**
(ESP32-S3, firmware abierto derivado de Crazyflie, ~50-60 USD) con su módulo de
posicionamiento (flujo óptico PMW3901 + distancia al piso), 80-100 USD con
baterías. El DBPOWER queda como segundo cuerpo para más adelante.


- ✅ Puente MSP (`puente.rs`), piloto de seguridad, percepción desde cámara
  real (`tools/ojo.py`). Probados en seco, no contra una placa.
- ✅ Puente CRTP para el LiteWing (`crtp.rs`, `dron litewing`): las cuatro
  consignas como «consigna de altura» del commander de Crazyflie, por WiFi.
  Formato verificado contra la doc de ESP-Drone y `cflib`; probado contra un
  dron falso por UDP. **Falta con el aparato:** los signos (`--signos`, hélices
  sacadas), la versión del protocolo y la telemetría de vuelta (log CRTP).
- ✅ Cerebro de a bordo (`csrc/iafz_64_abordo.c`): medido en emulador, 28.700
  instrucciones por decisión, idéntico al del gym (regla 3.14).
- ❌ Perfil del DBPOWER (`dbpower.rs`) sin conectar al lazo; masa sin pesar.
- ❌ Flujo óptico real (el modelo sin GPS supone ±8% de escala y 0.5°/s de
  deriva de rumbo: hay que medirlo). **Lo primero con la placa: el dron
  quieto 5 minutos registrando el rumbo** — sin brújula, esa deriva manda
  todo el SLAM (7.48).
- ❌ Cero pruebas en placa. Falta la FC brushed 1S (~$35).

```bash
cargo run --release -- sorteos 14 clasico n=24   # la vara
cargo run --release -- estres 14                  # la vara bajo averías
cargo run --release -- capacidades                # en castellano
```
