#!/usr/bin/env bash
# Builds a relocatable Nginx: OpenSSL, PCRE2 and zlib are compiled in, and lampRabit
# passes the prefix and config on the command line.
#   build-nginx.sh <version>   ->  out/nginx-<version>-<os>-<arch>.tar.gz (Linux or macOS)
. "$(dirname "$0")/lib.sh"
VERSION="$1"

log "Nginx $VERSION: checking the release signature"
tarball="$WORK/src/nginx-$VERSION.tar.gz"
curl -fsSL --retry 3 -o "$tarball" "https://nginx.org/download/nginx-$VERSION.tar.gz"
curl -fsSL --retry 3 -o "$tarball.asc" "https://nginx.org/download/nginx-$VERSION.tar.gz.asc"
export GNUPGHOME="$WORK/gnupg"
rm -rf "$GNUPGHOME" && mkdir -m 700 "$GNUPGHOME"
for key in $NGINX_KEYS; do curl -fsSL "$key" | gpg --batch --quiet --import; done
gpg --batch --verify "$tarball.asc" "$tarball"
src="$(unpack "$tarball")"

openssl="$(openssl_src)"
pcre2="$(pcre2_src)"
zlib="$(zlib_src)"

log "Nginx $VERSION ($OS $ARCH): compiling"
cd "$src"
# File AIO and these hardening flags exist on Linux only. The ${…+…} form keeps an empty
# array from tripping `set -u` in macOS's bash 3.2.
platform=()
[ "$OS" = linux ] && platform=(--with-file-aio --with-ld-opt="-Wl,-z,relro,-z,now")
./configure \
    --prefix=. \
    --conf-path=conf/nginx.conf \
    --error-log-path=logs/error.log \
    --http-log-path=logs/access.log \
    --pid-path=logs/nginx.pid \
    --http-client-body-temp-path=temp/client_body \
    --http-proxy-temp-path=temp/proxy \
    --http-fastcgi-temp-path=temp/fastcgi \
    --http-uwsgi-temp-path=temp/uwsgi \
    --http-scgi-temp-path=temp/scgi \
    --with-openssl="$openssl" \
    --with-pcre="$pcre2" --with-pcre-jit \
    --with-zlib="$zlib" \
    --with-threads \
    --with-http_ssl_module \
    --with-http_v2_module \
    --with-http_realip_module \
    --with-http_gzip_static_module \
    --with-http_stub_status_module \
    --with-http_sub_module \
    --with-http_auth_request_module \
    --with-stream \
    --with-stream_ssl_module \
    --with-cc-opt="-O2 -fstack-protector-strong" \
    ${platform[@]+"${platform[@]}"} >/dev/null
make -j"$JOBS" >/dev/null

stage="$WORK/stage-nginx-$VERSION"
rm -rf "$stage" && mkdir -p "$stage/conf"
install -m 755 objs/nginx "$stage/nginx"
strip_bin "$stage/nginx"
cp conf/mime.types conf/fastcgi_params conf/fastcgi.conf conf/nginx.conf "$stage/conf/"
cp LICENSE "$stage/LICENSE"
licenses "$stage" "$openssl" "$pcre2" "$zlib"
check_deps "$stage/nginx"
"$stage/nginx" -V
package nginx "$VERSION" "$stage"
