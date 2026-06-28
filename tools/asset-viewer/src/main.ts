import "./style.css";
import {
  assignKindOverride,
  fetchAssets,
  formatBytes,
  formatDate,
  openInBlender,
} from "./api";
import { AssetViewer } from "./viewer";
import type { AssetEntry, MeshStats } from "./types";

const CATEGORY_LABELS: Record<string, string> = {
  "blender-staging": "Blender staging (sin catálogo)",
  "blender-pipeline": "Pipeline Blender",
  "game-models": "Game models (drop-in)",
  "test-fixtures": "Test fixtures",
  "blend-source": "Archivos .blend",
  other: "Otros",
};

const app = document.getElementById("app")!;

let allAssets: AssetEntry[] = [];
let selected: AssetEntry | null = null;
let meshStats: MeshStats | null = null;
let status = "Escaneando repositorio…";
let watchTimer = 0;
let watchEnabled = false;
let focusKind = "";

const els = {
  counts: null as HTMLDivElement | null,
  list: null as HTMLDivElement | null,
  status: null as HTMLDivElement | null,
  detail: null as HTMLDivElement | null,
  hint: null as HTMLDivElement | null,
  canvas: null as HTMLCanvasElement | null,
  search: null as HTMLInputElement | null,
  category: null as HTMLSelectElement | null,
  extFilter: null as HTMLSelectElement | null,
  wireCb: null as HTMLInputElement | null,
  rotCb: null as HTMLInputElement | null,
  kindInput: null as HTMLInputElement | null,
};

let viewer: AssetViewer | null = null;

function initShell(): void {
  app.innerHTML = `
    <header>
      <h1>HA <span>Asset</span> Viewer</h1>
      <span class="tag">dev tool · no runtime</span>
      <div class="spacer"></div>
      <button type="button" id="btn-watch">Watch: OFF</button>
      <button type="button" id="btn-refresh">↻ Rescan</button>
    </header>
    <aside>
      <div class="filters">
        <input id="search" type="search" placeholder="Buscar por nombre o ruta…" />
        <select id="category"><option value="">Todas las categorías</option></select>
        <select id="ext-filter">
          <option value="">Todos los tipos</option>
          <option value=".glb">.glb</option>
          <option value=".gltf">.gltf</option>
          <option value=".blend">.blend</option>
        </select>
        <div class="kind-row">
          <label>Kind juego</label>
          <input id="kind-input" type="text" placeholder="flora_tree_oak" />
        </div>
        <div id="counts" class="counts"></div>
      </div>
      <div id="asset-list" class="asset-list"></div>
    </aside>
    <main>
      <canvas id="viewport"></canvas>
      <div class="viewer-overlay">
        <label><input id="wire-cb" type="checkbox" /> Wireframe</label>
        <label><input id="rot-cb" type="checkbox" /> Auto-rotate</label>
        <button type="button" id="btn-frame">Encuadrar</button>
        <button type="button" id="btn-assign" class="accent">Usar en juego</button>
      </div>
      <div id="hint" class="viewer-hint">Selecciona un .glb o .gltf para previsualizar.</div>
      <div id="error-banner" class="error-banner" hidden></div>
    </main>
    <footer>
      <section>
        <h4>Estado</h4>
        <div id="status" class="mono"></div>
      </section>
      <section>
        <h4>Asset / mesh</h4>
        <div id="detail"></div>
      </section>
    </footer>
  `;

  els.counts = document.getElementById("counts") as HTMLDivElement;
  els.list = document.getElementById("asset-list") as HTMLDivElement;
  els.status = document.getElementById("status") as HTMLDivElement;
  els.detail = document.getElementById("detail") as HTMLDivElement;
  els.hint = document.getElementById("hint") as HTMLDivElement;
  els.canvas = document.getElementById("viewport") as HTMLCanvasElement;
  els.search = document.getElementById("search") as HTMLInputElement;
  els.category = document.getElementById("category") as HTMLSelectElement;
  els.extFilter = document.getElementById("ext-filter") as HTMLSelectElement;
  els.wireCb = document.getElementById("wire-cb") as HTMLInputElement;
  els.rotCb = document.getElementById("rot-cb") as HTMLInputElement;
  els.kindInput = document.getElementById("kind-input") as HTMLInputElement;

  for (const [key, label] of Object.entries(CATEGORY_LABELS)) {
    const opt = document.createElement("option");
    opt.value = key;
    opt.textContent = label;
    els.category!.append(opt);
  }

  viewer = new AssetViewer({
    canvas: els.canvas,
    onStats: (s) => {
      meshStats = s;
      renderDetails();
    },
    onStatus: (msg) => {
      status = msg;
      if (els.status) els.status.textContent = msg;
      hideError();
    },
    onError: (msg) => showError(msg),
  });

  els.search!.oninput = () => refreshList();
  els.category!.onchange = () => refreshList();
  els.extFilter!.onchange = () => refreshList();
  els.wireCb!.onchange = () => viewer?.setWireframe(els.wireCb!.checked);
  els.rotCb!.onchange = () => viewer?.setAutoRotate(els.rotCb!.checked);
  document.getElementById("btn-frame")!.onclick = () => viewer?.resetCamera();
  document.getElementById("btn-refresh")!.onclick = () => void loadAssets(true);
  document.getElementById("btn-assign")!.onclick = () => void assignToGame();
  document.getElementById("btn-watch")!.onclick = () => toggleWatch();

  const params = new URLSearchParams(location.search);
  focusKind = params.get("kind") ?? "";
  const focusRel = params.get("rel") ?? params.get("path") ?? "";
  if (focusKind && els.kindInput) els.kindInput.value = focusKind;

  void loadAssets(false).then(() => {
    if (focusRel) {
      const hit = allAssets.find((a) => a.rel === focusRel || a.path === focusRel);
      if (hit) void selectAsset(hit);
      return;
    }
    if (focusKind) {
      const hit = allAssets.find(
        (a) =>
          a.name === `${focusKind}.glb` ||
          a.catalogKind === focusKind ||
          a.rel.includes(`/${focusKind}.glb`),
      );
      if (hit) void selectAsset(hit);
    }
  });
}

function showError(msg: string): void {
  const banner = document.getElementById("error-banner");
  if (!banner) return;
  banner.hidden = false;
  banner.textContent = msg;
}

function hideError(): void {
  const banner = document.getElementById("error-banner");
  if (banner) banner.hidden = true;
}

function toggleWatch(): void {
  watchEnabled = !watchEnabled;
  const btn = document.getElementById("btn-watch");
  if (btn) btn.textContent = watchEnabled ? "Watch: ON" : "Watch: OFF";
  if (watchEnabled) {
    watchTimer = window.setInterval(() => void loadAssets(true), 2500);
  } else {
    clearInterval(watchTimer);
  }
}

function filteredAssets(): AssetEntry[] {
  const q = els.search?.value.trim().toLowerCase() ?? "";
  const cat = els.category?.value ?? "";
  const ext = els.extFilter?.value ?? "";
  return allAssets.filter((a) => {
    if (cat && a.category !== cat) return false;
    if (ext && a.ext !== ext) return false;
    if (q && !a.rel.toLowerCase().includes(q) && !a.name.toLowerCase().includes(q))
      return false;
    return true;
  });
}

function refreshList(): void {
  if (!els.list || !els.counts) return;
  const filtered = filteredAssets();
  const blendCount = allAssets.filter((a) => a.ext === ".blend").length;
  els.counts.textContent = `${filtered.length} / ${allAssets.length} · ${blendCount} .blend`;

  const groups = new Map<string, AssetEntry[]>();
  for (const a of filtered) {
    const g = groups.get(a.category) ?? [];
    g.push(a);
    groups.set(a.category, g);
  }

  els.list.innerHTML = "";
  if (filtered.length === 0) {
    els.list.innerHTML = `<div class="empty">No hay assets que coincidan.</div>`;
    return;
  }

  for (const [catKey, items] of groups) {
    const group = document.createElement("div");
    group.className = "asset-group";
    const h3 = document.createElement("h3");
    h3.textContent = CATEGORY_LABELS[catKey] ?? catKey;
    group.append(h3);
    for (const asset of items) {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "asset-item" + (selected?.rel === asset.rel ? " active" : "");
      const name = document.createElement("div");
      name.className = "name";
      name.textContent = asset.name;
      const meta = document.createElement("div");
      meta.className = "meta";
      meta.innerHTML = `<span class="badge ${asset.ext.slice(1)}">${asset.ext.slice(1)}</span>
        <span>${formatBytes(asset.size)}</span>`;
      if (asset.inCatalog === true)
        meta.innerHTML += `<span class="badge catalog-yes">catalog</span>`;
      else if (asset.inCatalog === false)
        meta.innerHTML += `<span class="badge catalog-no">staging</span>`;
      btn.append(name, meta);
      btn.onclick = () => void selectAsset(asset);
      group.append(btn);
    }
    els.list.append(group);
  }

  if (els.hint) {
    const show3d = selected && (selected.ext === ".glb" || selected.ext === ".gltf");
    els.hint.style.display = show3d ? "none" : "flex";
  }
}

function renderDetails(): void {
  if (!els.detail) return;
  els.detail.innerHTML = "";
  if (!selected) {
    els.detail.innerHTML = `<div class="mono">—</div>`;
    return;
  }
  const rel = document.createElement("div");
  rel.className = "mono";
  rel.textContent = selected.rel;
  els.detail.append(rel);

  const dl = document.createElement("dl");
  dl.className = "stats-grid";
  const rows: [string, string][] = [
    ["Tamaño", formatBytes(selected.size)],
    ["Modificado", formatDate(selected.mtimeMs)],
    ["Categoría", CATEGORY_LABELS[selected.category] ?? selected.category],
  ];
  if (selected.inCatalog !== null) {
    rows.push([
      "Catálogo",
      selected.inCatalog ? `✓ ${selected.catalogKind}` : "✗ no registrado",
    ]);
  }
  if (meshStats) {
    rows.push(
      ["Tris", String(meshStats.triangles)],
      ["Verts", String(meshStats.vertices)],
      ["Meshes", String(meshStats.meshes)],
      [
        "BBox (m)",
        `${meshStats.bbox.x.toFixed(2)} × ${meshStats.bbox.y.toFixed(2)} × ${meshStats.bbox.z.toFixed(2)}`,
      ],
    );
  }
  for (const [k, v] of rows) {
    const dt = document.createElement("dt");
    dt.textContent = k;
    const dd = document.createElement("dd");
    dd.textContent = v;
    dl.append(dt, dd);
  }
  els.detail.append(dl);

  if (selected.ext === ".blend") {
    const openBtn = document.createElement("button");
    openBtn.type = "button";
    openBtn.textContent = "Abrir en Blender";
    openBtn.style.marginTop = "8px";
    openBtn.onclick = () =>
      void openInBlender(selected!.rel).catch((e) => {
        status = String(e);
        if (els.status) els.status.textContent = status;
      });
    els.detail.append(openBtn);
  }
}

async function selectAsset(asset: AssetEntry): Promise<void> {
  selected = asset;
  meshStats = null;
  refreshList();
  renderDetails();

  const kindFromName = asset.name.replace(/\.(glb|gltf)$/i, "");
  if (els.kindInput && asset.inCatalog) els.kindInput.value = kindFromName;

  if (asset.ext === ".glb" || asset.ext === ".gltf") {
    try {
      await viewer?.loadGlb(asset.rel);
    } catch {
      /* onError handled */
    }
    renderDetails();
    refreshList();
  } else if (asset.ext === ".blend") {
    status = "Archivo .blend — usa «Abrir en Blender» abajo.";
    if (els.status) els.status.textContent = status;
  }
}

async function assignToGame(): Promise<void> {
  const kind = els.kindInput?.value.trim();
  if (!kind) {
    showError("Escribe un catalog kind (ej. flora_tree_oak) antes de asignar.");
    return;
  }
  if (!selected || (selected.ext !== ".glb" && selected.ext !== ".gltf")) {
    showError("Selecciona un .glb o .gltf válido en la lista.");
    return;
  }
  try {
    await assignKindOverride(kind, selected.rel);
    status = `Asignado: ${kind} ← ${selected.rel} (el juego recargará en ~1s)`;
    if (els.status) els.status.textContent = status;
    hideError();
  } catch (err) {
    showError(err instanceof Error ? err.message : String(err));
  }
}

async function loadAssets(silent: boolean): Promise<void> {
  if (!silent) {
    status = "Escaneando…";
    if (els.status) els.status.textContent = status;
  }
  try {
    const prevSel = selected?.rel;
    const data = await fetchAssets();
    allAssets = data.assets;
    if (prevSel) {
      const again = allAssets.find((a) => a.rel === prevSel);
      if (again && again.mtimeMs !== selected?.mtimeMs) {
        selected = again;
        if (again.ext === ".glb" || again.ext === ".gltf") {
          await viewer?.loadGlb(again.rel).catch(() => undefined);
        }
      }
    }
    const blendCount = allAssets.filter((a) => a.ext === ".blend").length;
    if (!silent) {
      status = `Listo — ${allAssets.length} archivos (${blendCount} .blend)`;
      if (els.status) els.status.textContent = status;
    }
  } catch (err) {
    status = `Error al escanear: ${err instanceof Error ? err.message : String(err)}`;
    if (els.status) els.status.textContent = status;
    allAssets = [];
  }
  refreshList();
  renderDetails();
}

initShell();
