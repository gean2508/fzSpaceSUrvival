#!/usr/bin/env python3
"""**¿REEMPLAZA?** Juzga una pieza dibujada contra el/los asset(s) del pack que vendría a sustituir.

    python3 scripts/juez-arte.py nueva.png --contra pack1.png [pack2.png ...] [--lamina salida.png]

Imprime las medidas por celda (ver `scripts/lib/juez.py`), el veredicto (`SUPERA` / `A LA ALTURA` /
`POR DEBAJO`) y, con `--lamina`, arma la lámina A CIEGAS: la nueva mezclada con las del pack, con letras.
**Mirala antes de abrir `<lámina>.respuesta.json`.** Si se adivina la dibujada, no reemplaza.

Sale con código 1 si queda POR DEBAJO: un asset comprado sólo se saca por uno nuestro que pase (skill
`pixel-art` §0.1).
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from lib.juez import informe, juzgar, lamina_a_ciegas  # noqa: E402


def main():
    args = sys.argv[1:]
    if "--contra" not in args or args.index("--contra") == 0:
        raise SystemExit(__doc__)
    lamina = None
    if "--lamina" in args:
        i = args.index("--lamina")
        lamina = Path(args[i + 1])
        args = args[:i] + args[i + 2 :]
    i = args.index("--contra")
    nueva, refs = Path(args[0]), [Path(a) for a in args[i + 1 :]]
    r = juzgar(nueva, refs)
    print(informe(r, nueva.name))
    if lamina:
        print(f"  lámina a ciegas: {lamina_a_ciegas(nueva, refs, lamina)} (la respuesta, DESPUÉS de mirarla)")
    sys.exit(1 if r["veredicto"] == "POR DEBAJO" else 0)


if __name__ == "__main__":
    main()
