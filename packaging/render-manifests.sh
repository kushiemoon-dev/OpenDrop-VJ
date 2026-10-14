#!/usr/bin/env bash
# Renders the Scoop and winget manifests for a published release.
#
#   packaging/render-manifests.sh 0.2.0 [OUT_DIR]
#
# Downloads the release's Windows zip, hashes it and fills the templates in
# packaging/scoop and packaging/winget. Run it only after the release assets
# exist. Nothing is submitted anywhere: the output is plain files.
set -euo pipefail

version="${1:?usage: render-manifests.sh VERSION [OUT_DIR]}"
out="${2:-dist-manifests}"
here="$(cd "$(dirname "$0")" && pwd)"
url="https://github.com/kushiemoon-dev/OpenDrop-VJ/releases/download/v${version}/OpenDrop-Native-${version}-windows-x64.zip"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
echo "Downloading $url"
curl -fsSL -o "$tmp/release.zip" "$url"

sha="$(sha256sum "$tmp/release.zip" | cut -d' ' -f1)"
sha_upper="$(printf '%s' "$sha" | tr 'a-f' 'A-F')"
release_date="$(date -u +%Y-%m-%d)"

render() {
  sed -e "s|@VERSION@|${version}|g" \
      -e "s|@SHA256@|${sha}|g" \
      -e "s|@SHA256_UPPER@|${sha_upper}|g" \
      -e "s|@RELEASE_DATE@|${release_date}|g" "$1" > "$2"
}

mkdir -p "$out/scoop" "$out/winget/${version}"
render "$here/scoop/opendrop-vj.json.in" "$out/scoop/opendrop-vj.json"
for template in "$here"/winget/*.yaml.in; do
  name="$(basename "${template%.in}")"
  render "$template" "$out/winget/${version}/${name}"
done

echo "Rendered into $out (sha256 $sha):"
find "$out" -type f | sort
