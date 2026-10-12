#!/usr/bin/env bash
# Mide el cerebro de a bordo (csrc/iafz_64_abordo.c) en un procesador de 32
# bits SIN tener el chip: lo compila para RISC-V (el núcleo del ESP32-C3, con y
# sin FPU), lo corre en un emulador (unicorn) con un genoma de verdad, cuenta
# instrucciones por decisión y comprueba que da EXACTAMENTE lo mismo que la
# versión de escritorio. Uso:  tools/chip/medir.sh [genoma.adn]
#
# Requiere clang (con destino riscv32) y, una sola vez:
#   python3 -m venv /tmp/venv_chip && /tmp/venv_chip/bin/pip install unicorn pyelftools
set -euo pipefail
cd "$(dirname "$0")/../.."
GEN=${1:-genomes/DRON3D_64.adn}
PY=${PY:-/tmp/venv_chip/bin/python}
T=$(mktemp -d)
clang --target=riscv32-unknown-elf -march=rv32imfc -mabi=ilp32f -O2 -ffreestanding -fno-pic -mno-relax \
      -c csrc/iafz_64_abordo.c -o "$T/abordo.o"
if llvm-objdump-18 -r "$T/abordo.o" 2>/dev/null | grep -q R_RISCV; then
  echo "el código de a bordo llama a algo externo: no se puede correr pelado"; exit 1
fi
echo "código de a bordo (RISC-V con FPU): $(llvm-size-18 "$T/abordo.o" | awk 'NR==2{print $1}') bytes de programa; el genoma, 11392 bytes en flash"
clang -O2 tools/chip/nativo.c csrc/iafz_64_abordo.c -o "$T/nativo"
"$T/nativo" "$GEN" > "$T/nativo.txt"
"$PY" tools/chip/emular.py "$T/abordo.o" "$GEN" "$T/emu.txt"
diff -q "$T/nativo.txt" "$T/emu.txt" >/dev/null && echo "emulado == escritorio, bit a bit, 50 decisiones"
"$PY" tools/chip/contar_flotantes.py "$T/abordo.o" "$GEN"
rm -rf "$T"
