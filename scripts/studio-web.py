#!/usr/bin/env python3
"""**HA Studio en el navegador**, sin instalar nada.

HA Studio es egui nativo: necesita una ventana. Este script le da una pantalla
virtual (Xvfb), lo corre ahí adentro y sirve una página que muestra esa pantalla
en vivo y le manda los clics, la rueda y el teclado (extensión XTEST). Se usa
desde el celu por tailnet:

    ./editor_assets web            # y abrir http://<ip-tailnet>:8790

Escucha SÓLO en la IP de Tailscale (la laptop tiene ufw cerrado salvo tailnet):
no queda expuesto a la wifi. Ctrl+C lo apaga todo.
"""

from __future__ import annotations

import asyncio
import io
import json
import os
import subprocess
import sys
import time
from pathlib import Path

from aiohttp import web
from PIL import Image
from Xlib import X, XK, display as xdisplay
from Xlib.ext import xtest

RAIZ = Path(__file__).resolve().parent.parent
PANTALLA = os.environ.get("STUDIO_DISPLAY", ":77")
ANCHO, ALTO = 1600, 900
PUERTO = int(os.environ.get("STUDIO_PORT", "8790"))
FPS = 12
CALIDAD = 70

PAGINA = """<!doctype html><html lang="es"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1,viewport-fit=cover">
<title>HA Studio</title>
<style>
html,body{margin:0;height:100%;background:#171a21;overflow:hidden}
#p{position:absolute;inset:0;display:flex;align-items:center;justify-content:center}
img{max-width:100%;max-height:100%;touch-action:none;user-select:none;-webkit-user-select:none;cursor:default}
#t{position:fixed;left:-1000px;top:0;opacity:0}
#k{position:fixed;right:10px;bottom:10px;font:600 14px system-ui;padding:10px 14px;border-radius:8px;
border:0;background:#3a3a50;color:#fff;opacity:.85}
</style></head><body>
<div id="p"><img id="v" alt="HA Studio" draggable="false"></div>
<input id="t" autocapitalize="off" autocomplete="off" autocorrect="off">
<button id="k" type="button">Teclado</button>
<script>
const v=document.getElementById('v'),t=document.getElementById('t');
const ws=new WebSocket((location.protocol==='https:'?'wss://':'ws://')+location.host+'/ws');
ws.binaryType='blob';
let url=null;
ws.onmessage=e=>{const u=URL.createObjectURL(e.data);v.onload=()=>{if(url)URL.revokeObjectURL(url);url=u};v.src=u};
const send=o=>ws.readyState===1&&ws.send(JSON.stringify(o));
function pos(e){const r=v.getBoundingClientRect();return{x:Math.round((e.clientX-r.left)*v.naturalWidth/r.width),y:Math.round((e.clientY-r.top)*v.naturalHeight/r.height)}}
v.addEventListener('pointerdown',e=>{v.setPointerCapture(e.pointerId);send({t:'move',...pos(e)});send({t:'down',b:e.button===2?3:e.button===1?2:1});e.preventDefault()});
v.addEventListener('pointermove',e=>send({t:'move',...pos(e)}));
v.addEventListener('pointerup',e=>{send({t:'move',...pos(e)});send({t:'up',b:e.button===2?3:e.button===1?2:1})});
v.addEventListener('contextmenu',e=>e.preventDefault());
v.addEventListener('wheel',e=>{send({t:'move',...pos(e)});send({t:'wheel',d:e.deltaY>0?5:4});e.preventDefault()},{passive:false});
const tecla=e=>{if(e.target===t&&e.key.length===1)return;send({t:e.type==='keydown'?'kd':'ku',k:e.key});e.preventDefault()};
window.addEventListener('keydown',tecla);window.addEventListener('keyup',tecla);
t.addEventListener('input',()=>{for(const c of t.value){send({t:'kd',k:c});send({t:'ku',k:c})}t.value=''});
document.getElementById('k').onclick=()=>t.focus();
</script></body></html>"""

NOMBRES = {
    "Enter": "Return", "Backspace": "BackSpace", "Escape": "Escape", "Tab": "Tab",
    "Delete": "Delete", "ArrowLeft": "Left", "ArrowRight": "Right", "ArrowUp": "Up",
    "ArrowDown": "Down", "Control": "Control_L", "Shift": "Shift_L", "Alt": "Alt_L",
    "Meta": "Super_L", "Home": "Home", "End": "End", "PageUp": "Prior", "PageDown": "Next",
    " ": "space", "Insert": "Insert",
}


class Pantalla:
    def __init__(self):
        self.d = xdisplay.Display(PANTALLA)
        self.raiz = self.d.screen().root

    def captura(self) -> bytes:
        crudo = self.raiz.get_image(0, 0, ANCHO, ALTO, X.ZPixmap, 0xFFFFFFFF)
        im = Image.frombytes("RGB", (ANCHO, ALTO), crudo.data, "raw", "BGRX")
        buf = io.BytesIO()
        im.save(buf, "JPEG", quality=CALIDAD)
        return buf.getvalue()

    def _codigo(self, k: str):
        nombre = NOMBRES.get(k, k)
        sym = XK.string_to_keysym(nombre)
        if not sym and len(k) == 1:
            sym = ord(k)
        return self.d.keysym_to_keycode(sym) if sym else 0, (len(k) == 1 and k != k.lower())

    def evento(self, m: dict):
        t = m.get("t")
        if t == "move":
            xtest.fake_input(self.d, X.MotionNotify, x=int(m["x"]), y=int(m["y"]))
        elif t in ("down", "up"):
            xtest.fake_input(self.d, X.ButtonPress if t == "down" else X.ButtonRelease, int(m["b"]))
        elif t == "wheel":
            xtest.fake_input(self.d, X.ButtonPress, int(m["d"]))
            xtest.fake_input(self.d, X.ButtonRelease, int(m["d"]))
        elif t in ("kd", "ku"):
            kc, mayus = self._codigo(m.get("k", ""))
            if not kc:
                return
            shift = self.d.keysym_to_keycode(XK.string_to_keysym("Shift_L"))
            if t == "kd":
                if mayus:
                    xtest.fake_input(self.d, X.KeyPress, shift)
                xtest.fake_input(self.d, X.KeyPress, kc)
            else:
                xtest.fake_input(self.d, X.KeyRelease, kc)
                if mayus:
                    xtest.fake_input(self.d, X.KeyRelease, shift)
        self.d.sync()


async def main():
    ip = subprocess.run(["tailscale", "ip", "-4"], capture_output=True, text=True).stdout.split()
    host = os.environ.get("STUDIO_HOST") or (ip[0] if ip else "127.0.0.1")

    xvfb = subprocess.Popen(["Xvfb", PANTALLA, "-screen", "0", f"{ANCHO}x{ALTO}x24", "-nolisten", "tcp"],
                            stderr=subprocess.DEVNULL)
    time.sleep(1.0)
    env = dict(os.environ, DISPLAY=PANTALLA, WAYLAND_DISPLAY="", WGPU_BACKEND="gl",
               WINIT_UNIX_BACKEND="x11", LIBGL_ALWAYS_SOFTWARE="1")
    env.pop("WAYLAND_DISPLAY")
    binario = RAIZ / "native/target/debug/ha-asset-tools"
    studio = subprocess.Popen([str(binario), "studio"], cwd=RAIZ / "native", env=env)
    await asyncio.sleep(3)

    pant = Pantalla()
    clientes: set = set()

    async def pagina(_):
        return web.Response(text=PAGINA, content_type="text/html")

    async def ws(req):
        s = web.WebSocketResponse(max_msg_size=1 << 20)
        await s.prepare(req)
        clientes.add(s)
        try:
            async for msg in s:
                if msg.type == web.WSMsgType.TEXT:
                    pant.evento(json.loads(msg.data))
        finally:
            clientes.discard(s)
        return s

    async def emitir():
        anterior = b""
        while True:
            await asyncio.sleep(1 / FPS)
            if not clientes:
                continue
            cuadro = await asyncio.get_running_loop().run_in_executor(None, pant.captura)
            if cuadro == anterior:
                continue
            anterior = cuadro
            for c in list(clientes):
                try:
                    await c.send_bytes(cuadro)
                except Exception:
                    clientes.discard(c)

    def maximizar():
        """egui abre con su tamaño; sin gestor de ventanas no hay "maximizar",
        así que se la lleva a (0,0) y al tamaño de la pantalla virtual."""
        d = xdisplay.Display(PANTALLA)
        for w in d.screen().root.query_tree().children:
            try:
                attrs = w.get_attributes()
                if attrs.map_state == X.IsViewable:
                    w.configure(x=0, y=0, width=ANCHO, height=ALTO)
            except Exception:
                pass
        d.sync()

    app = web.Application()
    app.router.add_get("/", pagina)
    app.router.add_get("/ws", ws)
    runner = web.AppRunner(app)
    await runner.setup()
    await web.TCPSite(runner, host, PUERTO).start()
    print(f"HA Studio en http://{host}:{PUERTO}", flush=True)
    tarea = asyncio.create_task(emitir())
    try:
        vueltas = 0
        while studio.poll() is None:
            if vueltas < 30:  # la ventana tarda en aparecer; después se deja quieta
                maximizar()
            vueltas += 1
            await asyncio.sleep(1)
        print("HA Studio se cerró", flush=True)
    finally:
        tarea.cancel()
        studio.terminate()
        xvfb.terminate()


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        sys.exit(0)
