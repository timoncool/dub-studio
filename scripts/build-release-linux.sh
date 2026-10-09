#!/bin/bash
# Builds the experimental Linux x86-64 Dub Studio: a .deb and an AppImage. Engines, CUDA libraries and
# models are downloaded in the app ("First run"); the bundle carries the frontend, the fonts and the PP-OCR
# models, which come from the folder given as the first argument (its models/ocr).
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
models_source="${1:?usage: build-release-linux.sh <folder with models/ocr>}"
staging="$root/desktop/src-tauri/staging"
ocr_files=(det.onnx cls.onnx rec_cyrillic.onnx rec_cyrillic.dict.txt rec_ch.onnx rec_ch.dict.txt)

for f in "${ocr_files[@]}"; do
    [ -f "$models_source/models/ocr/$f" ] || { echo "[ERROR] no models/ocr/$f under $models_source" >&2; exit 1; }
done

cd "$root"
npm --prefix frontend ci --no-audit --no-fund
npm --prefix frontend run build
cargo test --workspace

rm -rf "$staging"
mkdir -p "$staging/frontend" "$staging/models/ocr"
cp -r frontend/dist "$staging/frontend/dist"
cp -r fonts "$staging/fonts"
for f in "${ocr_files[@]}"; do cp "$models_source/models/ocr/$f" "$staging/models/ocr/"; done
echo "[OK] bundle staged in $staging"

npm --prefix desktop ci --no-audit --no-fund
cd desktop
npm exec tauri build -- --config src-tauri/tauri.linux-release.conf.json --bundles deb,appimage

find src-tauri/target/release/bundle \( -name '*.deb' -o -name '*.AppImage' \)
