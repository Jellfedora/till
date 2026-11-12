#!/usr/bin/env bash
# Publie une nouvelle version : ./scripts/release.sh 0.2.0
# Change la version dans Cargo.toml, commite, pose le tag vX.Y.Z et pousse ;
# GitHub Actions construit alors les archives macOS, Linux et Windows et publie
# la release, que Ronnie propose ensuite d'installer.
set -euo pipefail
cd "$(dirname "$0")/.."

version=${1:?usage: scripts/release.sh X.Y.Z}
[[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "version invalide : $version (attendu X.Y.Z)" >&2; exit 1; }
[[ -z $(git status --porcelain) ]] || { echo "des modifications ne sont pas commitées" >&2; exit 1; }
[[ $(git branch --show-current) == master ]] || { echo "à lancer depuis master" >&2; exit 1; }
grep -q "^## \[$version\]" CHANGELOG.md || { echo "CHANGELOG.md n'a pas de section [$version]" >&2; exit 1; }

# Seulement la version du paquet (la première du fichier) ; marche avec les sed GNU et BSD.
sed -i.bak "1,/^version = \".*\"/s/^version = \".*\"/version = \"$version\"/" Cargo.toml
rm -f Cargo.toml.bak
cargo check --quiet # met Cargo.lock à jour
git diff --quiet || git commit -am "Release v$version"
git tag -a "v$version" -m "Till $version"
git push origin master "v$version"
echo "v$version poussée : suis la compilation dans l'onglet Actions du dépôt."
