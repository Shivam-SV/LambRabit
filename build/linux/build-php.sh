#!/usr/bin/env bash
# Builds fully static `php` (CLI) and `php-fpm` with static-php-cli for one release
# line; the newest patch release of that line is used.
#   build-php.sh <major.minor>   ->  out/php-<version>-<os>-<arch>.tar.gz (Linux or macOS)
. "$(dirname "$0")/lib.sh"
LINE="$1"

spc_sum_var="SPC_SHA256_$(echo "${OS}_$ARCH" | tr '[:lower:]' '[:upper:]')"
spc_tar="$(fetch "https://github.com/crazywhalecc/static-php-cli/releases/download/$SPC_VERSION/spc-$OS-$ARCH.tar.gz" "${!spc_sum_var}")"
build="$WORK/php-$LINE"
mkdir -p "$build"
tar xzf "$spc_tar" -C "$build"
cd "$build"

log "PHP $LINE: toolchain"
./spc doctor --auto-fix
log "PHP $LINE: sources"
./spc download --with-php="$LINE" --for-extensions="$PHP_EXTENSIONS" --for-libs="$PHP_LIBS" --retry=3
log "PHP $LINE: compiling"
# static-php-cli keeps the compiler's output in its log; show it when the build fails.
./spc build "$PHP_EXTENSIONS" --with-libs="$PHP_LIBS" --build-cli --build-fpm || {
    tail -n 80 log/spc.output.log >&2 2>/dev/null || true
    exit 1
}

VERSION="$(buildroot/bin/php -r 'echo PHP_VERSION;')"
stage="$WORK/stage-php-$VERSION"
rm -rf "$stage" && mkdir -p "$stage"
install -m 755 buildroot/bin/php buildroot/bin/php-fpm "$stage/"
cp source/php-src/php.ini-development source/php-src/php.ini-production source/php-src/LICENSE "$stage/"
"$stage/php" -v
modules="$("$stage/php" -m)"
echo "$modules"
# The cache clients are the reason these builds exist instead of distro packages. (Read
# once: `php -m | grep -q` can kill php with SIGPIPE, which pipefail reports as missing.)
for ext in redis memcached igbinary; do
    grep -qx "$ext" <<<"$modules" || { echo "PHP $VERSION was built without $ext" >&2; exit 1; }
done
# GD without these silently lacks imagecreatefromjpeg() and friends.
for feature in "JPEG Support" "WebP Support" "FreeType Support"; do
    "$stage/php" -r "exit(gd_info()['$feature'] ? 0 : 1);" || { echo "PHP $VERSION's GD has no $feature" >&2; exit 1; }
done
check_deps "$stage/php" "$stage/php-fpm"
# Every library compiled in (OpenSSL, ICU, libiconv, libmemcached...), as their licences require.
./spc dump-license --for-extensions="$PHP_EXTENSIONS" --dump-dir="$stage/licenses"
[ -n "$(ls -A "$stage/licenses")" ] || { echo "no licences were collected for PHP $VERSION" >&2; exit 1; }
package php "$VERSION" "$stage"
