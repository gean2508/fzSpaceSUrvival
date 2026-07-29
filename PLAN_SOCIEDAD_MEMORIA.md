# PLAN — Sociedad viva con MEMORIA (F6 del vivario)

> Sub-plan de `PLAN_VIVARIO_VIDA_EMERGENTE.md` §F6. Colonos/NPCs que **recuerdan** lo que
> les hacés. No podar lo emergente ([[ai-life-emergence-no-prune]]). Fuente de conducta:
> `ha-shared/src/fauna_brain.rs`.

## 0. Visión (usuario, 2026-07-25)

Los colonos y NPCs de cada planeta deben ser **full inteligentes**: una **simulación de
sociedad/vida** donde **evolucionan** y **RECUERDAN**. Ejemplo: si le matás al papá de
alguien, **esa familia te recuerda** y reacciona (rencor). Reputación, familias,
relaciones — emergente, no scripteado.

## 1. Modelo

- **NPC** = agente con **cerebro IAFZ** (como la fauna) + **familia/linaje** + identidad
  persistente (`uid`). Vive cerca de los hábitats (reusa las anclas de `sync_colonists`);
  población = los colonos que enviaste (`colony.state.colonists`).
- **Memoria del jugador = reputación por FAMILIA** `[-1 hostil .. +1 amable]`, PERSISTE en
  `society.json`. Es "quién sos vos para ellos".
- **Eventos que la mueven**: matar a un miembro → su familia baja fuerte (rencor) + los
  **testigos** cercanos (otras familias) bajan un poco. (Ayudar/regalar → sube: F6.2.)
- **La conducta reacciona a la reputación**: rencor → te evitan/huyen (luego atacan);
  amables → se acercan. Tinte visual por disposición (debug). El cerebro maneja el
  movimiento; la disposición lo sesga (como la fauna hostil de F5).
- **Persistencia + activación por cuerpo** (como el vivario F2): instancia sólo el cuerpo
  donde estás; al salir, snapshot→`records`; al volver, respawn. Los NPCs y su memoria
  **no se pierden**.

## 2. Fases

- **F6.1 — Núcleo memoria** *(este turno)*: `society.rs` — NPCs con cerebro + familia;
  **reputación por familia persistente**; **evento de daño** (golpe cerca del NPC) → rencor
  de su familia + testigos; conducta sesgada por reputación (huida/acercamiento + tinte);
  activación por cuerpo + `society.json`; overlay debug (creativo). Reemplaza a los colonos
  "tontos" (`sync_colonists`). Compila verde.
- **F6.2 — Evolución + vínculos**: persistir el genoma del NPC (evoluciona); reproducción →
  hijos heredan familia + un "imprint" de la reputación (nacen prevenidos si la familia te
  odia); relaciones intra-familia (estigmergia).
- **F6.3 — Roles/sociedad**: trabajos (minero/granjero/guardia) emergentes por necesidad;
  la reputación abre/cierra comercio, ayuda, o desata hostilidad de grupo.
- **F6.4 — Multi-planeta**: sociedades por planeta con su temperamento (F5) → culturas
  distintas; la cámara (F7) te deja observarlas en remoto.

## 3. Riesgos / no-regresión

- Perf/batería: NPCs acotados (`MAX_NPCS`), cerebro a 15 Hz + cull por distancia + pausa en
  background. Ids de cerebro 72..120 (fauna 0..48, residentes 48..72). `MAX_AGENTS=1000`.
- Determinismo del sensor en `fauna_brain.rs`.
- No podar lo emergente: la memoria abre reacciones, no impone guiones.
- UI ASCII-only. Persistencia con `inventory_persist::persist_dir()`.

## 4. Estado

- [x] F6.1 — núcleo memoria: `society.rs` — NPCs con cerebro IAFZ (ids 72..120) + familia;
  **reputación por familia PERSISTE** (`society/society.json`); **evento de daño** (tap de
  habilidad cerca del cursor mata al NPC → rencor de su familia `-0.7` + testigos `-0.2`);
  conducta sesgada por reputación (rencor → huye/evita + tinte rojizo; aprecio → se acerca +
  verdoso); activación por cuerpo (como el vivario) + overlay debug creativo. Reemplaza a
  `sync_colonists` (colonos "tontos"). Población = `colony.state.colonists`, sembrada en
  familias de 3 junto a los hábitats lunares. Compila verde. Pendiente Redmi.
- [x] F6.2 — evolución + vínculos: el GENOMA de cada NPC persiste y MUTA al reproducirse
  (`g_<uid>.adn`, `iafz_sys::save/load`); reproducción con energía alta → el hijo hereda la
  FAMILIA (y con ella la memoria: nace prevenido si la familia te odia) + genoma mutado.
- [x] F6.3 — roles/hostilidad de grupo + recambio: conducta emergente por reputación
  (rep ≤ -0.55 → la familia te ENFRENTA/encara; recelo → huye; aprecio → se acerca;
  neutral → deambula) con tinte; muerte por vejez (`NPC_LIFESPAN`) → recambio generacional
  que habilita evolución real. (Comercio/ítems según reputación = follow-up.)
- [x] F6.4 — multi-planeta: en cuerpos no-Tierra/Luna se siembran NATIVOS deterministas con
  disposición base según el TEMPERAMENTO del planeta (espejo de la fauna F5) → culturas
  distintas: un planeta hostil te recibe con recelo; uno amigable, cordial. La Luna sigue
  siendo tus colonos. Todo compila verde.
</content>
