# Recorridos manejados por una persona

Cada `.mandos` es una sesión de manejo grabada desde la cabina
(`http://localhost:8080/cabina` → botón **grabar** → **guardar**).

Una línea por muestra, `acelerador giro`, muestreado a 20 Hz
(`movil::PASOS_POR_MUESTRA`). Se graban los MANDOS y no la trayectoria: al
reproducirlos por la misma cinemática dan el mismo recorrido bit a bit, así
que lo manejado es un caso de entrenamiento reproducible y no una anécdota.

**El entrenador los levanta solos.** `dron entrenarpersecucion` arma su
escalera con los 6 recorridos sintéticos más uno por cada archivo de acá, y
los pone AL FINAL porque son los más difíciles: un guion sintético es suave
por construcción, una persona conduciendo corrige todo el tiempo, frena tarde
y toma cada curva distinto. Además se evalúan siempre con todo el rango de
ruido de sensores — son lo más parecido a la realidad que tiene el proyecto.

Estos archivos **se versionan**: son datos, y datos que nadie puede volver a
generar igual. Si borrás uno, se pierde ese caso.
