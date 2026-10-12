#!/usr/bin/env python3
"""
EL OJO — puente de la cámara real a los sensores visuales del kernel.

Para qué existe
---------------
La biblia dice que para hardware falta, entre otras cosas, "una cámara real en
vez de `RasgosVisuales` calculado analíticamente del estado exacto del
simulador". Hoy el dron NO tiene percepción: los tres números visuales que la
red consume se calculan con trigonometría a partir de la posición exacta de los
obstáculos, que en el aparato real nadie te regala.

Esto mide, con una cámara de verdad, si esos tres números se pueden producir
desde píxeles y en qué rango salen. No entrena nada ni vuela nada: contesta la
pregunta que bloquea el paso a hardware.

Lo que este puente SÍ puede dar, y lo que NO
--------------------------------------------
Los 96 bits que ve el kernel NO son visuales. El reparto real
(`gym3d::sensores_guia`) es:

    error al punto de guía          30 bits   ← GPS / estimador de posición
    velocidad relativa al objetivo  15 bits   ← GPS / estimador
    cómo me estoy moviendo          18 bits   ← IMU + estimador
    actitud                         12 bits   ← IMU
    altura sobre el suelo            7 bits   ← barómetro / lidar
    VISIÓN                          14 bits   ← LA CÁMARA (esto)
                                    -------
                                    96 bits

O sea que una cámara cubre **14 de 96 bits (15%)**. El otro 85% sale del
controlador de vuelo, no de píxeles. Cualquier plan que diga "la cámara alimenta
los 96 sensores" está mal planteado: los otros 82 bits son inercia, altura y
posición.

Y dentro de esos 14 bits hay una trampa más: **el flujo óptico ya no se
consume**. `flujo_x`/`flujo_y` se sacaron del código de sensores (ver el
comentario "El reparto de los 96 bits" en `gym3d.rs`) porque duplicaban peor lo
que velocidad y altura ya daban con más resolución. Lo que queda es:

    obs_horizontal   6 bits   ±1.4 rad   rumbo a la amenaza
    obs_proximidad   5 bits    0..1.0    amenaza acumulada
    obs_altura       3 bits   ±8.0 m     cuánto sobresale por encima del dron

El flujo sigue siendo el cálculo intermedio correcto —la divergencia se deriva
de él— pero no es una entrada. Calcularlo y mandarlo tal cual no conecta con
nada.

**Y la divergencia TAMPOCO entra ya a los 96 bits** (bitácora 7.18): es
exactamente `vz/altura`, y las dos magnitudes viajan por separado con más
resolución, así que sus 3 bits se dieron a `obs_altura`. Se sigue midiendo acá
porque es la única cantidad que se puede comparar de frente entre la cámara
real y el simulador — el termómetro de si la percepción desde píxeles funciona.

`obs_altura` NO se puede sacar de flujo óptico monocular: necesita profundidad
o conocer el tamaño del obstáculo. En esta máquina el nodo de profundidad
(`/dev/video2`, formato Z16) existe pero no entrega imagen sin librealsense 1.x.

Por qué la divergencia es la comparación limpia
------------------------------------------------
En el simulador `divergencia = vz / altura` (`sim3d.rs`), que es la tasa de
expansión del campo visual: el inverso del tiempo hasta el impacto. Una cámara
mide EXACTAMENTE esa misma cantidad física, como la divergencia del campo de
flujo (∂u/∂x + ∂v/∂y). Así que acá no se está aproximando nada: es el mismo
número por dos caminos, y por eso se pueden comparar de frente.

Uso
---
    <venv>/bin/python tools/ojo.py                 # 10 s de medición
    <venv>/bin/python tools/ojo.py --segundos 30
    <venv>/bin/python tools/ojo.py --camara 2

Acercá la mano a la lente: la divergencia tiene que ponerse POSITIVA y grande
(la escena se expande). Alejala: negativa. Movela a un costado: `obs_horizontal`
se va hacia ese lado.
"""

import argparse
import math
import sys
import time
import urllib.request
import urllib.error

import numpy as np

try:
    import cv2
except ImportError:
    sys.exit(
        "falta OpenCV. Instalalo en un venv, sin tocar el sistema:\n"
        "  python3 -m venv /tmp/visionenv\n"
        "  /tmp/visionenv/bin/pip install opencv-python-headless\n"
        "  /tmp/visionenv/bin/python tools/ojo.py"
    )

# --- lo que el kernel espera (gym3d::sensores_guia, no inventado) -----------
# Los rangos del termómetro. Un valor que se sale de acá satura el campo y deja
# de distinguirse: por eso importa medir si la cámara real cae adentro.
RANGO_OBS_HORIZONTAL = 1.4   # rad, ±  (== Camara::campo_obstaculos)
RANGO_OBS_PROXIMIDAD = 1.0   # 0..1
RANGO_DIVERGENCIA = 4.0      # 1/s, ±

# Medio campo de visión de la cámara SIMULADA para obstáculos, en radianes.
# Una webcam de laptop típica anda en 0.5-0.6 rad de medio campo: MUCHO más
# angosta. Eso no es un defecto del script, es un dato de hardware que conviene
# tener medido antes de comprar nada.
CAMPO_SIMULADO = 1.4


def divergencia_del_flujo(flujo, fps):
    """Divergencia del campo de flujo en la MISMA convención que el simulador.

    La cuenta, porque es fácil equivocarse y la primera versión de esto estaba
    mal por un factor 2 (regla 3.18: el instrumento también se audita).

    Si la cámara se acerca a una superficie a velocidad `v` desde distancia `d`,
    la imagen se escala por `k ≈ 1 + (v/d)·dt` por fotograma. Para una expansión
    uniforme alrededor del centro, un punto `x` se mueve a `x·k`, así que el
    flujo es `u = x·(k−1)` y por lo tanto:

        ∂u/∂x = k − 1          (en PÍXELES: ya es adimensional, no hay que
        ∂v/∂y = k − 1           normalizar por el tamaño de la imagen)

    La divergencia 2D es la suma de las dos, o sea `2(k−1)` — el DOBLE de la
    tasa de expansión. Pero `sim3d` define `divergencia = vz / altura`, que es
    `(k−1)·fps` sin el 2. Así que hay que dividir por dos para hablar el mismo
    idioma que la red entrenada; si no, todo sale 2× y satura un campo de 3 bits
    que sólo llega a ±4.0.
    """
    du_dx = np.gradient(flujo[..., 0], axis=1)
    dv_dy = np.gradient(flujo[..., 1], axis=0)
    return float(np.mean(du_dx + dv_dy) * fps / 2.0)


def anillo_de_proximidad(flujo, fps, sectores=5):
    """El ANILLO que el kernel consume: qué tan tapado está cada sector.

    Esto es lo que una cámara produce naturalmente —divergencia por columnas de
    la imagen— y desde que el kernel pasó a un anillo (bitácora 7.20) es
    exactamente la misma forma de dato, sin traducción intermedia. Antes había
    que colapsar las columnas a un rumbo promedio y una amenaza sumada, que es
    una abstracción con forma de simulador y además borra la información de
    dónde está el hueco.
    """
    du_dx = np.gradient(flujo[..., 0], axis=1)
    dv_dy = np.gradient(flujo[..., 1], axis=0)
    div = (du_dx + dv_dy) * fps / 2.0
    ancho = div.shape[1]
    bordes = np.linspace(0, ancho, sectores + 1).astype(int)
    # Sólo lo que se ACERCA cuenta como estorbo; alejarse no tapa nada.
    # Normalizado contra el rango de divergencia que el termómetro espera.
    return [
        float(np.clip(np.mean(div[:, bordes[i]:bordes[i + 1]]), 0.0, None)
              / RANGO_DIVERGENCIA * 4.0)
        for i in range(sectores)
    ]


def amenaza_por_columnas(flujo, fps, columnas=9):
    """Dónde está lo que se viene encima, y cuánto.

    Se parte la imagen en columnas y se mide la divergencia de cada una. La
    columna que más se expande es la amenaza; su posición da el rumbo y su
    magnitud la proximidad. Es el análogo honesto de `Camara::ver_con`, que en
    el simulador suma la amenaza de cada obstáculo visible y saca el rumbo
    promedio ponderado.
    """
    alto, ancho = flujo.shape[:2]
    du_dx = np.gradient(flujo[..., 0], axis=1)
    dv_dy = np.gradient(flujo[..., 1], axis=0)
    div = (du_dx + dv_dy) * fps / 2.0  # misma convención que `sim3d`

    bordes = np.linspace(0, ancho, columnas + 1).astype(int)
    fuerzas = np.array([
        float(np.mean(div[:, bordes[i]:bordes[i + 1]])) for i in range(columnas)
    ])
    # Sólo cuenta lo que se ACERCA (divergencia positiva): alejarse no es amenaza.
    positivas = np.clip(fuerzas, 0.0, None)
    total = float(positivas.sum())
    if total <= 1e-6:
        return 0.0, 0.0
    # La FUERZA es el promedio, no la suma: sumando las 9 columnas el número
    # salía 9x inflado y `obs_proximidad` saturaba el 100% del tiempo en la
    # validación contra verdad conocida, o sea que los 5 bits de ese campo
    # habrían quedado siempre en el tope — entrada muerta. El rumbo sí se pondera
    # con la suma, que es lo correcto ahí.
    fuerza = total / len(positivas)

    # Rumbo promedio ponderado por amenaza — igual criterio que el simulador,
    # que dejó de quedarse con el máximo justamente para no perder de vista el
    # segundo obstáculo (bitácora, deuda del anillo de proximidad).
    centros = (bordes[:-1] + bordes[1:]) / 2.0
    centro_img = ancho / 2.0
    return fuerza, float(
        np.sum(positivas * (centros - centro_img) / centro_img) / total
    )


def main():
    ap = argparse.ArgumentParser(description="cámara real → sensores visuales del kernel")
    ap.add_argument("--camara", type=str, default="0",
                    help="índice de cámara USB (0, 1, ...) O una URL de "
                         "streaming — sirve la cámara del CELULAR sin comprar "
                         "nada: instalá 'IP Webcam' (Android, gratis), abrí "
                         "la app, y pasá acá la URL que muestra terminada en "
                         "/video, ej. --camara http://192.168.1.5:8080/video")
    ap.add_argument("--video", type=str, default=None,
                    help="archivo de video en vez de la cámara. Sirve para "
                         "validar la cuenta contra una secuencia de verdad "
                         "conocida, y para cuando la cámara de la máquina no "
                         "entrega imagen (ver README de este archivo)")
    ap.add_argument("--segundos", type=float, default=10.0)
    ap.add_argument("--ancho", type=int, default=160,
                    help="se baja la resolución a propósito: el flujo es caro y "
                         "la cámara del dron tampoco va a ser 1080p")
    ap.add_argument("--grabar", type=str, default=None,
                    help="prefijo donde grabar: <prefijo>.mp4 con el video y "
                         "<prefijo>.csv con los tres campos por fotograma. "
                         "Sirve para repetir un experimento exacto con --video "
                         "en vez de depender de mover la mano igual dos veces")
    ap.add_argument("--visor", type=str, default=None,
                    help="URL del visor (ej. http://localhost:8080). Manda lo "
                         "que ve la cámara al dron EN VIVO: la percepción de "
                         "obstáculos deja de salir del simulador y sale de "
                         "píxeles de verdad")
    ap.add_argument("--campo", type=float, default=0.55,
                    help="medio campo de visión REAL de tu cámara, en radianes "
                         "(webcam de laptop ~0.5-0.6; el simulador usa 1.4)")
    args = ap.parse_args()

    # La cámara puede ser un índice USB o una URL de red (celular). OpenCV
    # abre las dos formas con el mismo `VideoCapture`, así que no hace falta
    # ninguna rama especial más abajo — el celular se comporta exactamente
    # como cualquier otra cámara para el resto del script.
    fuente_camara = args.camara
    try:
        fuente_camara = int(args.camara)
    except ValueError:
        pass  # es una URL, se deja como string

    if args.video:
        cap = cv2.VideoCapture(args.video)
        if not cap.isOpened():
            sys.exit(f"no pude abrir {args.video}")
        fps_archivo = cap.get(cv2.CAP_PROP_FPS) or 30.0
        # **Los tiempos reales, si el csv hermano está.** La cámara no entrega
        # fotogramas a cadencia fija: si se reproduce asumiendo los fps
        # declarados, la divergencia sale escalada por el error de cadencia
        # (es una tasa por segundo, no por fotograma). Con los tiempos de la
        # grabación, reproducir da lo mismo que capturar.
        import os
        tiempos = None
        csv_par = args.video.rsplit(".", 1)[0] + ".csv"
        if os.path.exists(csv_par):
            with open(csv_par) as f:
                filas = [l.split(",") for l in f.read().splitlines()[1:] if l]
            tiempos = [float(r[0]) for r in filas]
            print(f"  (cadencia real tomada de {os.path.basename(csv_par)})")
    else:
        tiempos = None
        cap = cv2.VideoCapture(args.camara)
        if not cap.isOpened():
            sys.exit(f"no pude abrir /dev/video{args.camara}")
        cap.set(cv2.CAP_PROP_FRAME_WIDTH, args.ancho)
        fps_archivo = None

    ok, previo = cap.read()
    if not ok:
        sys.exit("la cámara abrió pero no entrega fotogramas")
    previo = cv2.cvtColor(previo, cv2.COLOR_BGR2GRAY)
    if previo.shape[1] > args.ancho:
        _a = int(previo.shape[0] * args.ancho / previo.shape[1])
        previo = cv2.resize(previo, (args.ancho, _a), interpolation=cv2.INTER_AREA)
    alto_real, ancho_real = previo.shape

    print(f"EL OJO — cámara {args.camara}, {ancho_real}x{alto_real}")
    print(f"medio campo real {args.campo:.2f} rad  ·  el simulador asume "
          f"{CAMPO_SIMULADO:.2f} rad ({CAMPO_SIMULADO / args.campo:.1f}x más ancho)\n")
    print("acercá la mano a la lente → divergencia POSITIVA; alejala → negativa\n")
    print(f"  {'t':>5}  {'divergencia':>12} {'obs_horiz':>11} {'obs_prox':>9}   anillo  satura")
    print(f"  {'':5}  {'±4.0 (3 bits)':>12} {'±1.4 (6b)':>11} {'0-1 (5b)':>9}")

    # Grabación: video crudo + los tres campos por fotograma. El video se
    # guarda a la resolución REDUCIDA, que es la que de verdad se procesa —
    # grabar 640x480 y procesar 160 daría un archivo que no reproduce lo
    # medido, y repetir el experimento con --video daría otros números.
    video_out = None
    csv_out = None
    if args.grabar:
        # **Sin pérdida (FFV1), no mp4.** Medido: con mp4 la reproducción daba
        # 3.6% del rango en divergencia contra 14.0% de la captura en vivo. La
        # compresión con pérdida borra justo el grano fino que el flujo óptico
        # necesita para estimar movimiento, así que el archivo no reproducía lo
        # que se había medido — una grabación así no sirve para repetir un
        # experimento, que es para lo único que existe.
        video_out = cv2.VideoWriter(
            f"{args.grabar}.mkv", cv2.VideoWriter_fourcc(*"FFV1"), 15.0,
            (previo.shape[1], previo.shape[0]), False)
        csv_out = open(f"{args.grabar}.csv", "w")
        csv_out.write("t,divergencia,obs_horizontal,obs_proximidad\n")

    # Envío al visor: se avisa una vez que el ojo manda, y después sólo los
    # datos. Si el visor no está, se avisa y se sigue midiendo local — no
    # tiene sentido abortar una medición porque no haya nadie escuchando.
    visor_vivo = False
    if args.visor:
        try:
            urllib.request.urlopen(f"{args.visor}/ojo?on=1", timeout=2).read()
            visor_vivo = True
            print(f"  → el ojo MANDA en {args.visor}: la percepción de "
                  f"obstáculos sale de esta cámara\n")
        except Exception as e:
            print(f"  (no pude hablar con el visor: {e} — sigo midiendo local)\n")

    t0 = time.time()
    t_previo = t0
    muestras = []
    while time.time() - t0 < args.segundos:
        ok, cuadro = cap.read()
        if not ok:
            break
        gris = cv2.cvtColor(cuadro, cv2.COLOR_BGR2GRAY)
        # Reescalado explícito: pedirle la resolución a la cámara con
        # CAP_PROP_FRAME_WIDTH no sirve (devuelve 640x480 igual), y el flujo
        # denso a esa resolución corre a 2.5 fps — inservible para un lazo de
        # control. A 160 px va ~10x más rápido y la divergencia no pierde nada:
        # es un promedio sobre todo el campo, no un detalle fino.
        if gris.shape[1] > args.ancho:
            alto_nuevo = int(gris.shape[0] * args.ancho / gris.shape[1])
            gris = cv2.resize(gris, (args.ancho, alto_nuevo), interpolation=cv2.INTER_AREA)
        ahora = time.time()
        if fps_archivo:
            # Con un archivo el reloj de pared no dice nada: los fotogramas se
            # leen tan rápido como el disco los da. La cadencia real sale del
            # csv de la grabación si está, y si no de los fps declarados.
            i = len(muestras)
            if tiempos and 0 < i < len(tiempos):
                dt = max(tiempos[i] - tiempos[i - 1], 1e-3)
            else:
                dt = 1.0 / fps_archivo
            fps = 1.0 / dt
        else:
            dt = max(ahora - t_previo, 1e-3)
            fps = 1.0 / dt
        t_previo = ahora

        flujo = cv2.calcOpticalFlowFarneback(
            previo, gris, None,
            pyr_scale=0.5, levels=3, winsize=15,
            iterations=3, poly_n=5, poly_sigma=1.2, flags=0)
        previo = gris

        div = divergencia_del_flujo(flujo, fps)
        fuerza, rumbo_norm = amenaza_por_columnas(flujo, fps)
        anillo = [min(v, 1.0) for v in anillo_de_proximidad(flujo, fps)]
        # El rumbo normalizado (-1..1 del ancho) se pasa a radianes con el campo
        # REAL de esta cámara, no con el del simulador.
        obs_h = rumbo_norm * args.campo
        # La proximidad se normaliza contra el rango que el termómetro espera.
        obs_p = min(fuerza / RANGO_DIVERGENCIA, RANGO_OBS_PROXIMIDAD)

        sat = []
        if abs(div) > RANGO_DIVERGENCIA:
            sat.append("div")
        if abs(obs_h) > RANGO_OBS_HORIZONTAL:
            sat.append("horiz")
        if obs_p >= RANGO_OBS_PROXIMIDAD:
            sat.append("prox")

        if video_out is not None:
            video_out.write(gris)
            csv_out.write(f"{ahora - t0:.3f},{div:.4f},{obs_h:.4f},{obs_p:.4f}\n")
        if visor_vivo:
            try:
                q = "&".join(f"s{k}={v:.3f}" for k, v in enumerate(anillo))
                urllib.request.urlopen(f"{args.visor}/ojo?{q}", timeout=0.3).read()
            except Exception:
                pass  # un paquete perdido no arruina la medición

        muestras.append((div, obs_h, obs_p))
        barras = "".join("·-=#"[min(int(v * 3.99), 3)] for v in anillo)
        print(f"  {ahora - t0:5.1f}  {div:12.3f} {obs_h:11.3f} {obs_p:9.3f}   "
              f"[{barras}]  {','.join(sat) if sat else '-'}", flush=True)

    cap.release()
    if video_out is not None:
        video_out.release()
        csv_out.close()
        print(f"\n  grabado: {args.grabar}.mkv y {args.grabar}.csv")
    if visor_vivo:
        # Devolver la percepción al simulador: dejarla prendida sin nadie
        # alimentándola congelaría el último rumbo visto, y el dron seguiría
        # esquivando un fantasma.
        try:
            urllib.request.urlopen(f"{args.visor}/ojo?on=0", timeout=2).read()
            print("  → el ojo se apagó: el visor vuelve a su visión simulada")
        except Exception:
            pass
    if not muestras:
        sys.exit("no se capturó nada")

    m = np.array(muestras)
    print("\nRESUMEN — lo que una cámara REAL entrega, contra lo que el kernel espera\n")
    print(f"  {'campo':<16} {'medio':>9} {'mín':>9} {'máx':>9} {'rango kernel':>14} {'% del rango':>12}")
    for i, (nombre, rango) in enumerate([
            ("divergencia", RANGO_DIVERGENCIA),
            ("obs_horizontal", RANGO_OBS_HORIZONTAL),
            ("obs_proximidad", RANGO_OBS_PROXIMIDAD)]):
        col = m[:, i]
        usado = max(abs(col.min()), abs(col.max())) / rango * 100
        print(f"  {nombre:<16} {col.mean():9.3f} {col.min():9.3f} {col.max():9.3f} "
              f"{'±' + format(rango, '.1f'):>14} {usado:11.1f}%")

    print(f"\n  fotogramas: {len(muestras)}")
    print("\n  Si «% del rango» es muy chico, la cámara real no llega a excitar el")
    print("  campo y esos bits del termómetro quedan casi siempre en el mismo valor:")
    print("  la red entrenada con el rango completo estaría viendo una entrada muerta.")


if __name__ == "__main__":
    main()
