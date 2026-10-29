#!/usr/bin/env bash
# Builds Redis (redis-server, redis-cli, redis-benchmark) for Linux or macOS from the
# release tarball. Windows uses the community redis-windows build instead (see runtimes.rs).
#   build-redis.sh <version>   ->  out/redis-<version>-<os>-<arch>.tar.gz
. "$(dirname "$0")/lib.sh"
VERSION="$1"
sum_var="REDIS_SHA256_${VERSION//./_}"
[ -n "${!sum_var:-}" ] || { echo "no checksum for Redis $VERSION in versions.env ($sum_var)" >&2; exit 1; }

log "Redis $VERSION ($OS $ARCH): compiling"
src="$(unpack "$(fetch "https://download.redis.io/releases/redis-$VERSION.tar.gz" "${!sum_var}")")"
cd "$src"
# The core only: Redis 8's bundled modules (JSON, time series, search…) need extra
# toolchains. No TLS (local only, and it would tie the binaries to one OpenSSL), no systemd.
# Linux keeps Redis's bundled jemalloc; macOS uses its own allocator, as Redis does there.
make -C src -j"$JOBS" BUILD_TLS=no USE_SYSTEMD=no BUILD_WITH_MODULES=no >/dev/null

stage="$WORK/stage-redis-$VERSION"
rm -rf "$stage" && mkdir -p "$stage"
for bin in redis-server redis-cli redis-benchmark; do
    install -m 755 "src/$bin" "$stage/$bin"
    strip "$stage/$bin" 2>/dev/null || true
done
# redis-check-aof/rdb are the server binary under another name.
ln -s redis-server "$stage/redis-check-aof"
ln -s redis-server "$stage/redis-check-rdb"
cp redis.conf "$stage/redis.conf.sample"
cp LICENSE* COPYING* "$stage/" 2>/dev/null || true
check_deps "$stage/redis-server" "$stage/redis-cli"
"$stage/redis-server" --version
package redis "$VERSION" "$stage"
