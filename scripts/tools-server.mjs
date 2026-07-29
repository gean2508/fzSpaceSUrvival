#!/usr/bin/env node
/**
 * Herramientas remotas (FR1) — hub web THIN-CLIENT.
 *
 * Corre en la COMBO (donde viven los assets/proyecto). Lo abrís desde el celu por Tailscale
 * y editás los archivos data-driven del juego (manifest de arte, audio, mundo). El celu sólo
 * manda acciones; TODO se procesa y se guarda acá. Ver PLAN_HERRAMIENTAS_REMOTAS.md.
 *
 * Uso:  node scripts/tools-server.mjs            (lee token de TOOLS_TOKEN o genera uno)
 * Bind: 0.0.0.0:4600  → alcanzable por LAN y por la IP Tailscale.
 *
 * Seguridad: sólo se tocan los recursos declarados en RESOURCES (nada de rutas arbitrarias);
 * escritura valida JSON + hace backup .bak; los WRITES piden token (los reads son abiertos,
 * ya que Tailscale limita el acceso a tu tailnet). NO publicar esta URL a Internet.
 */
import { createServer } from "node:http";
import { readFileSync, writeFileSync, existsSync, copyFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { execSync } from "node:child_process";
import { randomBytes } from "node:crypto";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const PORT = Number(process.env.TOOLS_PORT || 4600);
const TOKEN = process.env.TOOLS_TOKEN || randomBytes(4).toString("hex");

// Recursos editables (sandbox): id → { label, ruta relativa al repo, tipo }.
const RESOURCES = {
  assets: {
    label: "Assets (arte)",
    path: "native/crates/ha-game/assets/art/manifest.json",
    type: "json",
    note: "manifest de arte del juego. Tras guardar, corré `npm run sync:art` y recompilá.",
  },
  audio: {
    label: "Musica / audio",
    path: "native/assets/audio.json",
    type: "json",
    note: "audio.json (crate ha-audio). Recompilá para oírlo en el juego.",
  },
  world: {
    label: "Mundo",
    path: "native/crates/ha-game/assets/world_edits.json",
    type: "json",
    note: "overrides del mundo (world_edits.json). Se reflejan al recargar/recompilar.",
  },
};

let tailscaleIp = "";
try {
  tailscaleIp = execSync("tailscale ip -4", { encoding: "utf8" }).trim().split("\n")[0] || "";
} catch { /* tailscale opcional */ }

function send(res, code, body, type = "application/json") {
  res.writeHead(code, { "content-type": type, "cache-control": "no-store" });
  res.end(typeof body === "string" ? body : JSON.stringify(body));
}

function readResource(id) {
  const r = RESOURCES[id];
  if (!r) return null;
  const abs = resolve(ROOT, r.path);
  const text = existsSync(abs) ? readFileSync(abs, "utf8") : "";
  return { id, label: r.label, note: r.note, exists: existsSync(abs), text };
}

function writeResource(id, text) {
  const r = RESOURCES[id];
  if (!r) return { ok: false, error: "recurso desconocido" };
  if (r.type === "json") {
    try { JSON.parse(text); } catch (e) { return { ok: false, error: "JSON invalido: " + e.message }; }
  }
  const abs = resolve(ROOT, r.path);
  // Sandbox: la ruta resuelta DEBE quedar dentro del repo.
  if (!abs.startsWith(ROOT)) return { ok: false, error: "ruta fuera del repo" };
  if (existsSync(abs)) copyFileSync(abs, abs + ".bak"); // backup
  writeFileSync(abs, text);
  return { ok: true };
}

const server = createServer((req, res) => {
  const url = new URL(req.url, "http://localhost");
  const path = url.pathname;

  if (path === "/") return send(res, 200, HUB_HTML(), "text/html; charset=utf-8");
  if (path === "/api/list") {
    return send(res, 200, Object.entries(RESOURCES).map(([id, r]) => ({ id, label: r.label })));
  }
  if (path.startsWith("/api/res/")) {
    const id = path.slice("/api/res/".length);
    if (req.method === "GET") {
      const data = readResource(id);
      return data ? send(res, 200, data) : send(res, 404, { error: "no existe" });
    }
    if (req.method === "PUT") {
      if ((url.searchParams.get("token") || req.headers["x-token"]) !== TOKEN) {
        return send(res, 403, { error: "token invalido" });
      }
      let body = "";
      req.on("data", (c) => (body += c));
      req.on("end", () => send(res, 200, writeResource(id, body)));
      return;
    }
  }
  if (path === "/api/action/sync-art" && req.method === "POST") {
    if ((url.searchParams.get("token") || req.headers["x-token"]) !== TOKEN) {
      return send(res, 403, { error: "token invalido" });
    }
    try {
      const out = execSync("npm run sync:art", { cwd: ROOT, encoding: "utf8", timeout: 180000 });
      return send(res, 200, { ok: true, out: out.slice(-1500) });
    } catch (e) {
      return send(res, 200, { ok: false, error: ((e.stdout || "") + (e.stderr || e.message || "")).slice(-800) });
    }
  }
  send(res, 404, { error: "not found" });
});

server.listen(PORT, "0.0.0.0", () => {
  const lines = [
    "",
    "  HERRAMIENTAS REMOTAS  —  hub listo",
    "  ----------------------------------",
    `  Local:     http://localhost:${PORT}`,
    tailscaleIp ? `  Tailscale: http://${tailscaleIp}:${PORT}   (abrilo desde el celu)` : "  (Tailscale no detectado)",
    `  Token (para GUARDAR): ${TOKEN}`,
    "  Recursos: " + Object.keys(RESOURCES).join(", "),
    "",
  ];
  console.log(lines.join("\n"));
});

function HUB_HTML() {
  return `<!doctype html><html lang="es"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Herramientas — Agencia Espacial</title>
<style>
:root{color-scheme:dark}
*{box-sizing:border-box}
body{margin:0;font:15px/1.4 system-ui,sans-serif;background:#0a0f14;color:#cfe3f2}
header{padding:12px 16px;background:#0e1620;border-bottom:1px solid #1d2c3a;position:sticky;top:0}
header b{color:#7fd0ff}
.tabs{display:flex;gap:6px;padding:8px 12px;flex-wrap:wrap;background:#0c141d;border-bottom:1px solid #16222e}
.tab{padding:8px 12px;border:1px solid #24384a;border-radius:6px;background:#10202c;color:#bfe0f5;cursor:pointer}
.tab.on{background:#12324a;border-color:#3f7fb0;color:#eaf6ff}
main{padding:12px}
.note{color:#8fb0c8;font-size:13px;margin:6px 0}
textarea{width:100%;height:56vh;background:#06101a;color:#d8ecff;border:1px solid #23384a;border-radius:8px;padding:10px;font:13px/1.4 ui-monospace,monospace;resize:vertical}
.bar{display:flex;gap:8px;align-items:center;margin-top:8px;flex-wrap:wrap}
button{padding:9px 16px;border:1px solid #3f7fb0;border-radius:8px;background:#12324a;color:#eaf6ff;font-weight:600;cursor:pointer}
input{padding:8px;border:1px solid #24384a;border-radius:6px;background:#06101a;color:#d8ecff}
#msg{font-size:13px}
.ok{color:#7fe0a0}.err{color:#ff8f8f}
</style></head><body>
<header><b>Agencia Espacial</b> — Herramientas remotas (thin-client)</header>
<div class="tabs" id="tabs"></div>
<main>
  <div class="note" id="note"></div>
  <textarea id="ta" spellcheck="false"></textarea>
  <div class="bar">
    <label>Token: <input id="tok" size="12" placeholder="para guardar"></label>
    <button id="save">Guardar</button>
    <button id="reload">Recargar</button>
    <button id="apply">Aplicar (sync:art)</button>
    <span id="msg"></span>
  </div>
</main>
<script>
let cur=null;
const $=(s)=>document.querySelector(s);
async function boot(){
  const list=await (await fetch('/api/list')).json();
  const tabs=$('#tabs');
  list.forEach((r,i)=>{
    const b=document.createElement('div');b.className='tab'+(i===0?' on':'');b.textContent=r.label;
    b.onclick=()=>{document.querySelectorAll('.tab').forEach(t=>t.classList.remove('on'));b.classList.add('on');load(r.id);};
    tabs.appendChild(b);
  });
  if(list[0])load(list[0].id);
}
async function load(id){
  cur=id;$('#msg').textContent='';
  const d=await (await fetch('/api/res/'+id)).json();
  $('#note').textContent=d.note||'';
  $('#ta').value=d.text||'';
}
$('#reload').onclick=()=>cur&&load(cur);
$('#save').onclick=async()=>{
  if(!cur)return;
  const tok=$('#tok').value.trim();
  const r=await fetch('/api/res/'+cur+'?token='+encodeURIComponent(tok),{method:'PUT',body:$('#ta').value});
  const j=await r.json();
  $('#msg').innerHTML=j.ok?'<span class=ok>Guardado (.bak creado)</span>':'<span class=err>'+(j.error||'error')+'</span>';
};
$('#apply').onclick=async()=>{
  const tok=$('#tok').value.trim();
  $('#msg').textContent='Aplicando sync:art...';
  const j=await (await fetch('/api/action/sync-art?token='+encodeURIComponent(tok),{method:'POST'})).json();
  $('#msg').innerHTML=j.ok?'<span class=ok>sync:art aplicado (recompila para verlo)</span>':'<span class=err>'+((j.error||'error')+'').slice(0,220)+'</span>';
};
boot();
</script>
</body></html>`;
}
