# Respaldo — lo que vivía fuera del repo

Esto estaba en el scratchpad de la sesión (`/tmp/...`), que **se borra al
reiniciar la máquina**. Se copió acá antes de un reboot.

## `entrenamientos/` — BORRADO el 2026-09-26

Los genomas de las corridas viejas eran ilegibles con el código de hoy y
además se entrenaron CON GPS (el dron no tiene GPS desde la bitácora 7.39).
Quedan en el historial de git si alguna vez hace falta repetir una medición
histórica.

## `camara/`

- `ojo_demo.*`, `ojo2.*` — capturas reales de la cámara de la laptop: video
  (`.mkv`, sin pérdida) y `.csv` con los tres campos visuales por fotograma.
  Se reproducen con `tools/ojo.py --video ARCHIVO`.
- `loom_*.mp4` — las secuencias sintéticas de acercamiento con `v` y `d`
  conocidos, que se usaron para validar la matemática del puente contra verdad
  de campo (bitácora 7.16).

## Lo que NO hace falta respaldar

- El venv de OpenCV: se rehace con
  `python3 -m venv ENV && ENV/bin/pip install opencv-python-headless`.
- El servicio del visor: es systemd de usuario, sobrevive al reboot solo.
