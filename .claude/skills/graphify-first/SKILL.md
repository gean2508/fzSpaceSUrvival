---
name: graphify-first
description: OBLIGATORIO antes de tocar codigo en este repo. Consulta el grafo de graphify (graphify-out/) para ubicar el codigo, sus vecinos y quien lo llama, en vez de grepear a ciegas. Usar al empezar CUALQUIER tarea que lea o modifique archivos del proyecto, y al terminarla para dejar el grafo actualizado.
---

# graphify primero, grep despues

El repo tiene un grafo de codigo en `graphify-out/` (raiz) y otro en
`native/graphify-out/`. Es la forma barata de ubicarse: una consulta al grafo
reemplaza cinco greps y varias lecturas de archivos enteros.

## Antes de tocar nada (siempre)

1. Mira si el grafo esta fresco — barato, una linea:

   ```sh
   ls -l graphify-out/graph.json
   ```

   Si es mas viejo que el ultimo commit que toco codigo, refrescalo en segundo
   plano y segui trabajando: `graphify update . `

2. Preguntale al grafo por el concepto que vas a tocar, NO por el archivo:

   ```sh
   graphify explain "camara"            # que es, y sus vecinos
   graphify path "vuelo" "camara"       # como se conectan dos cosas
   ```

3. Recien ahi abri archivos, y solo los que el grafo senalo.

## Ahorro de tokens (parte del contrato)

- **Nunca** vuelques `graph.json` (6 MB) ni `manifest.json` a la conversacion.
  Se consultan con `graphify explain` / `graphify path`, que devuelven un parrafo.
- `GRAPH_REPORT.md` son ~63 KB: leelo con `grep -n` sobre la seccion que importa,
  jamas entero.
- Salidas largas de `graphify update` van a un archivo del scratchpad y se lee el
  `tail`.
- Si el grafo ya te dio la respuesta, no la re-verifiques con grep "por las dudas".

## Al terminar la tarea

Si tocaste codigo, dejá el grafo al dia antes de dar el trabajo por hecho:

```sh
graphify update . > /tmp/.../graphify.log 2>&1   # en background, y tail al final
```

Si el refactor borro codigo y el grafo queda con menos nodos, agrega `--force`.

## Cuando NO aplica

Cambios que no tocan codigo del proyecto (editar `biblia.md`, mirar `git log`,
responder una pregunta sin abrir archivos). No gastes una consulta al grafo para
eso.
