# Shared helpers for the runtime builds (Linux and macOS; Windows only for repackaged
# third-party builds). Sourced by build-*.sh.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=versions.env
. "$HERE/versions.env"

# Archive names use Rust's names, as the app compares them with std::env::consts.
# TARGET_OS/TARGET_ARCH name the platform of a repackaged build (Memcached for Windows).
ARCH="${TARGET_ARCH:-$(uname -m)}"
[ "$ARCH" = "arm64" ] && ARCH="aarch64"
case "$(uname -s)" in Darwin) OS=macos ;; *) OS=linux ;; esac
OS="${TARGET_OS:-$OS}"
WORK="${WORK:-$PWD/work}"
OUT="${OUT:-$PWD/out}"
JOBS="$(getconf _NPROCESSORS_ONLN)"
mkdir -p "$WORK/src" "$OUT"

log() { printf '\n==> %s\n' "$*" >&2; }

# fetch <url> <sha256>: downloads into $WORK/src once and checks the checksum.
fetch() {
    local url="$1" sum="$2" file
    file="$WORK/src/$(basename "$url")"
    if [ ! -f "$file" ]; then
        curl -fsSL --retry 3 -o "$file.part" "$url"
        mv "$file.part" "$file"
    fi
    local actual
    actual="$( (sha256sum "$file" 2>/dev/null || shasum -a 256 "$file") | cut -d' ' -f1)"
    [ "$actual" = "$sum" ] || { rm -f "$file"; echo "checksum mismatch: $url" >&2; exit 1; }
    echo "$file"
}

# unpack <archive>: extracts into $WORK and prints the directory.
unpack() {
    local dir
    dir="$WORK/$(tar tf "$1" | head -1 | cut -d/ -f1)"
    rm -rf "$dir"
    tar xf "$1" -C "$WORK"
    echo "$dir"
}

# Static libraries every web server links in, so the result only needs glibc.
openssl_src() { unpack "$(fetch "https://github.com/openssl/openssl/releases/download/openssl-$OPENSSL_VERSION/openssl-$OPENSSL_VERSION.tar.gz" "$OPENSSL_SHA256")"; }
pcre2_src() { unpack "$(fetch "https://github.com/PCRE2Project/pcre2/releases/download/pcre2-$PCRE2_VERSION/pcre2-$PCRE2_VERSION.tar.gz" "$PCRE2_SHA256")"; }
zlib_src() { unpack "$(fetch "https://github.com/madler/zlib/releases/download/v$ZLIB_VERSION/zlib-$ZLIB_VERSION.tar.gz" "$ZLIB_SHA256")"; }
expat_src() { unpack "$(fetch "https://github.com/libexpat/libexpat/releases/download/R_${EXPAT_VERSION//./_}/expat-$EXPAT_VERSION.tar.gz" "$EXPAT_SHA256")"; }
libevent_src() { unpack "$(fetch "https://github.com/libevent/libevent/releases/download/release-$LIBEVENT_VERSION-stable/libevent-$LIBEVENT_VERSION-stable.tar.gz" "$LIBEVENT_SHA256")"; }

# package <name> <version> <dir>: tars <dir>'s contents as <name>-<version>-<os>-<arch>.tar.gz
# and writes the matching .sha256 next to it.
package() {
    local name="$1" version="$2" dir="$3" file
    file="$name-$version-$OS-$ARCH.tar.gz"
    if [ "$OS" = macos ]; then
        tar czf "$OUT/$file" -C "$dir" .
        (cd "$OUT" && shasum -a 256 "$file" > "$file.sha256")
    else
        tar czf "$OUT/$file" --owner=0 --group=0 -C "$dir" .
        (cd "$OUT" && sha256sum "$file" > "$file.sha256")
    fi
    log "Built $OUT/$file"
}

# strip_bin <file>…: drops symbols the binaries don't need to run.
strip_bin() {
    if [ "$OS" = macos ]; then
        strip -x "$@"
    else
        strip --strip-unneeded "$@"
    fi
}

# Fails when a binary needs a shared library other than the system's (glibc on Linux,
# libSystem and frameworks on macOS) or one shipped next to it (APR, as @rpath on macOS).
check_deps() {
    local bad
    if [ "$OS" = macos ]; then
        bad="$(otool -L "$@" | grep -v ':$' | awk '{print $1}' | grep -vE '^(/usr/lib/|/System/Library/|@rpath/libapr)' || true)"
    else
        bad="$(ldd "$@" 2>/dev/null | awk '/=>/ {print $1}' | grep -vE '^(libc|libm|libdl|libpthread|librt|libresolv|libcrypt|ld-linux.*|linux-vdso)\.so' | grep -vE '^libapr' || true)"
    fi
    if [ -n "$bad" ]; then
        echo "unexpected shared libraries:" >&2
        echo "$bad" | sort -u >&2
        exit 1
    fi
}
