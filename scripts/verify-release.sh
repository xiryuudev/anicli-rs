#!/usr/bin/env bash
# Unduh aset rilis/pre-rilis dari GitHub lalu uji binernya secara lokal.
#
#   scripts/verify-release.sh v0.9.0-rc1
#
# Yang diuji: checksum, --version, registry config, dan rantai penuh tiap source
# (search → detail → episode → stream) lewat `sourcecheck`.
set -euo pipefail

TAG="${1:?pakai: scripts/verify-release.sh <tag>}"
REPO="${REPO:-xiryuudev/anicli-rs}"
WORK=".scratch/verify-$TAG"
API="https://api.github.com/repos/$REPO"

if [[ -z "${GH_TOKEN:-}" ]]; then
  echo "error: GH_TOKEN belum di-set" >&2
  exit 2
fi

mkdir -p "$WORK"
echo "== rilis $TAG =="
curl -fsSL -H "Authorization: Bearer $GH_TOKEN" -H 'Accept: application/vnd.github+json' \
  "$API/releases/tags/$TAG" -o "$WORK/release.json"
python3 - "$WORK/release.json" <<'PY'
import json,sys
d=json.load(open(sys.argv[1]))
print("prerelease:", d["prerelease"], "| assets:", len(d["assets"]))
for a in d["assets"]:
    print(" -", a["name"], a["size"])
PY

# Unduh biner linux + checksum
curl -fsSL -H "Authorization: Bearer $GH_TOKEN" -H 'Accept: application/vnd.github+json' \
  "$API/releases/tags/$TAG" | python3 -c '
import json,sys
d=json.load(sys.stdin)
for a in d["assets"]:
    if a["name"] in ("anicli-x86_64-linux","sourcecheck-x86_64-linux","SHA256SUMS-x86_64-linux.txt"):
        print(a["browser_download_url"])
' | while read -r url; do
  name=$(basename "$url")
  echo "unduh $name"
  curl -fsSL -H "Authorization: Bearer $GH_TOKEN" "$url" -o "$WORK/$name"
done

chmod +x "$WORK/anicli-x86_64-linux" "$WORK/sourcecheck-x86_64-linux"
cd "$WORK"
if [[ -f SHA256SUMS-x86_64-linux.txt ]]; then
  echo "== checksum =="
  sed 's#  dist/#  #' SHA256SUMS-x86_64-linux.txt > sums.txt
  sha256sum -c sums.txt
fi

export ANICLI_CONFIG="$PWD/config/config.toml"
export ANICLI_DATA_DIR="$PWD/data"
echo "== versi =="
./anicli-x86_64-linux --version

echo "== registry =="
./anicli-x86_64-linux --no-tui 2>&1 | grep -E "installed|fallback" | head -3

status=0
for source in anichin anipm mkissa samehadaku otakudesu; do
  echo "== $source =="
  if ./sourcecheck-x86_64-linux --source "$source" --query "naruto" 2>&1 | tee "$source.log" | grep -E "^search|^detail|^judul|^episodes|^streams|^  - Episode 1 |tidak|gagal" | head -8
  then :; else status=1; fi
  if grep -qE "^(search|episodes) *: 0|tidak didukung|panik|panicked" "$source.log"; then
    echo "  !! masalah pada $source"
    status=1
  fi
done

echo "== selesai (status=$status) =="
exit "$status"
