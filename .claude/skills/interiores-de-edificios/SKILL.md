---
name: interiores-de-edificios
description: OBLIGATORIO antes de tocar cómo se entra a un edificio, su interior, sus pisos y escaleras, las puertas con vida, las ventanas encendidas de noche o los hostiles que te siguen adentro. Fija el patrón «interior como espacio aparte» (banda de pisos propia, plano generado puro, puerta con flecha y botón propio) y sirve para replicarlo en otro proyecto de mundo abierto 2D.
---

# Interior como espacio aparte

1. **Una banda de niveles por edificio** (`z >= Z0`) en una zona del mundo donde no hay nada: el exterior no se cuela y el interior puede ser más grande que la huella (tope 1,5×).
2. **El plano es puro** (`generar(lote)`): BSP de ambientes con vanos, ventanas, puerta al frente, hall con escalera. Nunca un piso al que no se llega (test de conectividad).
3. **Reusar lo que el mundo ya tiene**: muebles/camas/cofres por nivel, escaleras caminables, arte de paredes por kit.
4. **Entrar/salir** = botón propio + flecha amarilla exacta sobre la puerta (GTA SA). No compite con USAR.
5. **La puerta es un objeto con vida y nivel**; los hostiles se juntan afuera y la golpean; rota deja entrar.
6. **Ventanas de la fachada MEDIDAS del pixel** (`medir-ventanas.py`) y prendidas por la misma simulación que duerme/vela a la gente de adentro.
7. Probar con `HA_INTERIORTEST=1` y mirar las capturas.
