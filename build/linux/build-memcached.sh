#!/usr/bin/env bash
# Builds Memcached for Linux or macOS from the release tarball, with libevent compiled in.
# With "windows", repackages nono303's Cygwin build instead (Memcached has no official
# Windows build); that runs on any host, as nothing is compiled.
#   build-memcached.sh <version> [windows]   ->  out/memcached-<version>-<os>-<arch>.tar.gz
[ "${2:-}" = windows ] && export TARGET_OS=windows TARGET_ARCH=x86_64
. "$(dirname "$0")/lib.sh"
VERSION="$1"
sum_var="MEMCACHED_SHA256_${VERSION//./_}"
[ -n "${!sum_var:-}" ] || { echo "no checksum for Memcached $VERSION in versions.env ($sum_var)" >&2; exit 1; }
src="$(unpack "$(fetch "https://memcached.org/files/memcached-$VERSION.tar.gz" "${!sum_var}")")"

stage="$WORK/stage-memcached-$VERSION-$OS"
rm -rf "$stage" && mkdir -p "$stage"
cp "$src/COPYING" "$stage/LICENSE"

if [ "$OS" = windows ]; then
    win_var="MEMCACHED_WINDOWS_SHA256_${VERSION//./_}"
    [ -n "${!win_var:-}" ] || { echo "no checksum for the Windows build of Memcached $VERSION ($win_var)" >&2; exit 1; }
    log "Memcached $VERSION (Windows): repackaging nono303's build"
    zip="$WORK/src/memcached-$VERSION-windows.zip"
    curl -fsSL --retry 3 -o "$zip" "https://github.com/nono303/memcached/archive/refs/tags/$VERSION.zip"
    actual="$(sha256sum "$zip" | cut -d' ' -f1)"
    [ "$actual" = "${!win_var}" ] || { rm -f "$zip"; echo "checksum mismatch: nono303/memcached $VERSION" >&2; exit 1; }
    rm -rf "$WORK/memcached-win" && mkdir -p "$WORK/memcached-win"
    # libevent 2.1 for Windows 8 and newer (2.0 is only for Windows 7); the plain build runs
    # on every x64 CPU (the -avx/-avx2 ones need those instructions, -tls isn't used locally).
    unzip -q -j "$zip" "memcached-$VERSION/libevent-2.1/x64/*" -d "$WORK/memcached-win"
    cp "$WORK/memcached-win/memcached.exe" "$WORK"/memcached-win/cyg*.dll "$stage/"
    cat > "$stage/NOTICE" <<EOF
memcached.exe is Memcached $VERSION (BSD licence, see LICENSE), built for Windows with
Cygwin by nono303: https://github.com/nono303/memcached
cygwin1.dll and the other cyg*.dll files are Cygwin libraries (LGPLv3 and others); their
sources are at https://cygwin.com/
EOF
    package memcached "$VERSION" "$stage"
    exit 0
fi

deps="$WORK/deps-memcached"
rm -rf "$deps" && mkdir -p "$deps"
log "Memcached $VERSION ($OS $ARCH): libevent"
(cd "$(libevent_src)" && ./configure --prefix="$deps" --disable-shared --enable-static --with-pic \
    --disable-openssl --disable-samples --disable-libevent-regress --disable-debug-mode >/dev/null && make -j"$JOBS" install >/dev/null)

log "Memcached $VERSION ($OS $ARCH): compiling"
cd "$src"
# Only the static libevent exists in $deps, so it's linked in. No TLS (local only), no SASL.
./configure --with-libevent="$deps" --disable-sasl --disable-tls --disable-coverage --disable-docs >/dev/null
make -j"$JOBS" memcached >/dev/null
install -m 755 memcached "$stage/memcached"
strip "$stage/memcached"
check_deps "$stage/memcached"
"$stage/memcached" -V
package memcached "$VERSION" "$stage"
