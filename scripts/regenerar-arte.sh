#!/usr/bin/env bash
# Corre TODOS los importadores/post-fix de arte en el orden correcto (biblia §21, «Láminas de LimeZu → ciudad»).
# Uso, desde la raíz del repo:   ./scripts/regenerar-arte.sh && ./build-y-publicar.sh
# Necesita el pack crudo (PRUEBAS-CODIGOS/) donde lo esperan los importadores.
set -euo pipefail
cd "$(dirname "$0")/.."
for s in import-completos import-modulares recortar-modulares cercas-reparar reasignar-lotes import-casas-medidas; do
  echo "==> scripts/$s.py"
  python3 "scripts/$s.py"
done
echo "==> arte regenerado"
