#!/bin/bash
# La vara (clásico) con la física nueva (efecto suelo + aparato variable), y
# con la nominal para comparar. 24 sorteos por peldaño.
cd /home/fzion/Documents/mods/dron
E=genomes/EXP_20260927d
for p in $(seq 1 15); do
  $E/dron.bin sorteos $p clasico n=24 > $E/vara_$p.txt 2>&1 &
  $E/dron.bin sorteos $p clasico n=24 fisicanominal > $E/vara_nominal_$p.txt 2>&1 &
  wait
done
echo LISTO > $E/vara.listo
