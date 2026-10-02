#!/usr/bin/env python3
"""Mide fps y tirones del juego EN EL CELU desde afuera (SurfaceFlinger), sin
instrumentar el APK. Uso: celu-fps.py [segundos=30] [etiqueta] [serial]"""
import shlex, subprocess, sys, time

seg = float(sys.argv[1]) if len(sys.argv) > 1 else 30
etiq = sys.argv[2] if len(sys.argv) > 2 else ""
ser = sys.argv[3] if len(sys.argv) > 3 else "733493b90511"
adb = ["adb", "-s", ser]

def sh(*a):
    return subprocess.run(adb + ["shell", " ".join(shlex.quote(x) for x in a)], capture_output=True, text=True, timeout=40).stdout

capa = next((l.strip() for l in sh("dumpsys", "SurfaceFlinger", "--list").splitlines()
             if l.startswith("SurfaceView[com.hungerarena") and "BLAST" in l), None)
if not capa:
    sys.exit("no encuentro la capa del juego (¿está abierto?)")
sh("dumpsys", "SurfaceFlinger", "--latency-clear", capa)
time.sleep(seg)
out = sh("dumpsys", "SurfaceFlinger", "--latency", capa).splitlines()
ts = [int(l.split()[1]) for l in out[1:] if len(l.split()) == 3 and l.split()[1] not in ("0", "9223372036854775807")]
if len(ts) < 3:
    sys.exit("sin frames")
dt = sorted((b - a) / 1e6 for a, b in zip(ts, ts[1:]))
n = len(dt)
p = lambda q: dt[min(n - 1, int(n * q))]
tot = (ts[-1] - ts[0]) / 1e9
print(f"{etiq:14s} fps={len(ts)/tot:5.1f} p50={p(.5):5.1f}ms p95={p(.95):5.1f} p99={p(.99):5.1f} max={dt[-1]:6.1f} "
      f">33ms={sum(d>33.4 for d in dt):3d} >50ms={sum(d>50 for d in dt):3d}  ({len(ts)} frames/{tot:.0f}s)")
