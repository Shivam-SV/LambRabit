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
./spc download --with-php="$LINE" --for-extensions="$PHP_EXTENSIONS" --retry=3
log "PHP $LINE: compiling"
./spc build "$PHP_EXTENSIONS" --build-cli --build-fpm

VERSION="$(buildroot/bin/php -r 'echo PHP_VERSION;')"
stage="$WORK/stage-php-$VERSION"
rm -rf "$stage" && mkdir -p "$stage"
install -m 755 buildroot/bin/php buildroot/bin/php-fpm "$stage/"
cp source/php-src/php.ini-development source/php-src/php.ini-production source/php-src/LICENSE "$stage/"
"$stage/php" -v
"$stage/php" -m
# The cache clients are the reason these builds exist instead of distro packages.
for ext in redis memcached igbinary; do
    "$stage/php" -m | grep -qx "$ext" || { echo "PHP $VERSION was built without $ext" >&2; exit 1; }
done
check_deps "$stage/php" "$stage/php-fpm"
# Every library compiled in (OpenSSL, ICU, libiconv, libmemcached...), as their licences require.
./spc dump-license --for-extensions="$PHP_EXTENSIONS" --dump-dir="$stage/licenses"
[ -n "$(ls -A "$stage/licenses")" ] || { echo "no licences were collected for PHP $VERSION" >&2; exit 1; }
package php "$VERSION" "$stage"
