#!/usr/bin/env bash
# Construit l'archive de l'OS courant dans dist/ : le fichier publié dans les
# releases GitHub, que Ronnie télécharge depuis sa page Téléchargements.
# Les noms (till-<cible>.tar.gz ou .zip) doivent correspondre à ceux que Ronnie
# cherche (src/modules.rs dans Ronnie).
set -euo pipefail
cd "$(dirname "$0")/.."

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
rm -rf dist
mkdir -p dist/stage

case "$(uname -s)" in
Darwin)
    # Apple Silicon seulement, comme Ronnie.
    rustup target add aarch64-apple-darwin
    cargo build --release --locked --target aarch64-apple-darwin
    app=dist/stage/Till.app
    mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
    cp target/aarch64-apple-darwin/release/till "$app/Contents/MacOS/till"
    iconset=dist/Till.iconset
    mkdir -p "$iconset"
    for s in 16 32 128 256 512; do
        sips -z $s $s assets/icon/icon.png --out "$iconset/icon_${s}x${s}.png" >/dev/null
        sips -z $((s * 2)) $((s * 2)) assets/icon/icon.png --out "$iconset/icon_${s}x${s}@2x.png" >/dev/null
    done
    iconutil -c icns "$iconset" -o "$app/Contents/Resources/Till.icns"
    rm -rf "$iconset"
    sed "s/{{VERSION}}/$version/g" packaging/macos/Info.plist >"$app/Contents/Info.plist"
    # Le certificat de Ronnie (SIGN_IDENTITY) garde l'accès au micro d'une
    # version à l'autre ; sans lui, signature ad hoc (le micro est redemandé
    # à chaque mise à jour).
    codesign --force --deep --sign "${SIGN_IDENTITY:--}" "$app"
    tar -C dist/stage -czf dist/till-aarch64-apple-darwin.tar.gz Till.app
    ;;
Linux)
    target=$(rustc -vV | sed -n 's/^host: //p')
    cargo build --release --locked
    cp target/release/till dist/stage/
    tar -C dist/stage -czf "dist/till-$target.tar.gz" till
    ;;
MINGW* | MSYS* | CYGWIN*)
    target=$(rustc -vV | sed -n 's/^host: //p')
    cargo build --release --locked
    cp target/release/till.exe dist/stage/
    (cd dist/stage && 7z a -tzip "../till-$target.zip" till.exe >/dev/null)
    ;;
*)
    echo "OS non géré : $(uname -s)" >&2
    exit 1
    ;;
esac

rm -rf dist/stage
ls -l dist
