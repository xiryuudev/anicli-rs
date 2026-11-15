#!/usr/bin/env bash
# Build anicli-rs secara native di Termux (aarch64) lalu pasang ke $PREFIX/bin.
#
# Pemakaian:
#   bash scripts/build-termux.sh            # build + install ke $PREFIX/bin/anicli
#   bash scripts/build-termux.sh --no-install
#   bash scripts/build-termux.sh --deps     # pkg install dependensi yang kurang
#
# Catatan: project ini memakai rustls dengan provider `ring` (bukan aws-lc-rs),
# jadi TIDAK butuh cmake/NASM — cukup clang + perl dari Termux.
set -euo pipefail

INSTALL=1
INSTALL_DEPS=0
for arg in "$@"; do
  case "$arg" in
    --no-install) INSTALL=0 ;;
    --deps) INSTALL_DEPS=1 ;;
    -h|--help) sed -n '2,12p' "$0"; exit 0 ;;
    *) echo "argumen tidak dikenal: $arg" >&2; exit 2 ;;
  esac
done

die() { echo "error: $*" >&2; exit 1; }

# 1) Pastikan ini Termux asli (F-Droid/GitHub), bukan Play Store.
if [ -z "${TERMUX_VERSION:-}" ] && [[ "${PREFIX:-}" != *com.termux* ]]; then
  die "bukan Termux. Install Termux dari F-Droid (versi Play Store tidak dirawat)."
fi
[ -n "${PREFIX:-}" ] || die "PREFIX kosong; jalankan di Termux."
[ -d "$PREFIX" ] || die "PREFIX ($PREFIX) tidak ada."

# 2) Arsitektur.
ARCH=$(uname -m)
case "$ARCH" in
  aarch64|arm64) ;;
  *) die "arsitektur $ARCH belum didukung script ini; target resmi: aarch64 (arm64)." ;;
esac

# 3) Dependensi.
PKGS=(rust clang perl make git)
MISSING=()
for bin in cargo clang perl make git; do
  command -v "$bin" >/dev/null 2>&1 || MISSING+=("$bin")
done
if [ "${#MISSING[@]}" -gt 0 ]; then
  echo "dependensi belum ada: ${MISSING[*]}"
  echo "jalankan: pkg install ${PKGS[*]}"
  if [ "$INSTALL_DEPS" -eq 1 ]; then
    pkg install -y "${PKGS[@]}"
  else
    die "pasang dulu dependensi di atas (atau ulangi dengan --deps)."
  fi
fi

ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"
echo "==> build anicli-rs di $ROOT (host target: $(rustc -vV | awk '/^host:/{print $2}'))"

# Termux rust secara default membangun untuk host aarch64-linux-android (Bionic),
# jadi tidak perlu --target/NDK. Pastikan target memang Android.
HOST=$(rustc -vV | awk '/^host:/{print $2}')
[[ "$HOST" == *android* ]] || echo "peringatan: host rust $HOST bukan Android; hasil mungkin bukan binary Termux." >&2

echo "==> cargo build --release --locked"
cargo build --release --locked

BIN="$ROOT/target/release/anicli"
[ -x "$BIN" ] || die "binary tidak ditemukan di $BIN"

echo "==> verifikasi binary"
file "$BIN" || true
if command -v readelf >/dev/null 2>&1; then
  readelf -h "$BIN" | awk '/Machine/{print "  " $0}'
  readelf -l "$BIN" 2>/dev/null | grep -i interpreter | sed 's/^/  /' || true
fi
./"$BIN" --version

# 4) Install ke $PREFIX/bin.
if [ "$INSTALL" -eq 1 ]; then
  DEST="$PREFIX/bin/anicli"
  install -Dm755 "$BIN" "$DEST"
  install -Dm755 "$ROOT/target/release/sourcecheck" "$PREFIX/bin/sourcecheck"
  echo "==> terpasang: $DEST"
  echo "   jalankan: anicli"
else
  echo "==> dilewati instalasi; binary: $BIN"
fi

cat <<'EOF'

Langkah berikutnya:
  1. Pasang aplikasi player Android (F-Droid):
       mpv-android  (is.xyz.mpv)   <- direkomendasikan
       VLC          (org.videolan.vlc)  <- opsional
  2. Pastikan termux-tools ada:  pkg install termux-tools
  3. Jalankan: anicli
     Player Android dipilih otomatis (intent VIEW -> mpv-android/VLC/browser).
     Ubah di config: [playback] android_player = "mpv" | "vlc" | "auto"
  4. Baca docs/TERMUX.md untuk troubleshooting.
EOF
