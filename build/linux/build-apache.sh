#!/usr/bin/env bash
# Builds a relocatable Apache httpd: modules are shared (lampRabit loads the ones it
# needs), APR ships in lib/ and OpenSSL, PCRE2, zlib and expat are compiled in.
#   build-apache.sh <version>   ->  out/apache-<version>-<os>-<arch>.tar.gz (Linux or macOS)
. "$(dirname "$0")/lib.sh"
VERSION="$1"
sum_var="HTTPD_SHA256_${VERSION//./_}"
[ -n "${!sum_var:-}" ] || { echo "no checksum for httpd $VERSION in versions.env ($sum_var)" >&2; exit 1; }

deps="$WORK/deps-apache"
rm -rf "$deps" && mkdir -p "$deps"
export CFLAGS="-O2 -fPIC -fstack-protector-strong"

log "Apache $VERSION: static libraries"
(cd "$(zlib_src)" && ./configure --static --prefix="$deps" >/dev/null && make -j"$JOBS" install >/dev/null)
(cd "$(pcre2_src)" && ./configure --prefix="$deps" --disable-shared --enable-jit >/dev/null && make -j"$JOBS" install >/dev/null)
(cd "$(expat_src)" && ./configure --prefix="$deps" --disable-shared --without-docbook --without-examples --without-tests >/dev/null && make -j"$JOBS" install >/dev/null)
(cd "$(openssl_src)" && ./Configure --prefix="$deps" --libdir=lib no-shared no-tests -fPIC >/dev/null && make -j"$JOBS" >/dev/null && make install_sw >/dev/null)

log "Apache $VERSION: compiling"
src="$(unpack "$(fetch "https://archive.apache.org/dist/httpd/httpd-$VERSION.tar.bz2" "${!sum_var}")")"
cp -r "$(unpack "$(fetch "https://archive.apache.org/dist/apr/apr-$APR_VERSION.tar.bz2" "$APR_SHA256")")" "$src/srclib/apr"
cp -r "$(unpack "$(fetch "https://archive.apache.org/dist/apr/apr-util-$APR_UTIL_VERSION.tar.bz2" "$APR_UTIL_SHA256")")" "$src/srclib/apr-util"

prefix="$WORK/stage-apache-$VERSION"
rm -rf "$prefix"
cd "$src"
# Modules that would pull in system libraries (libxml2, Lua, nghttp2, curl…) stay off.
# The static OpenSSL needs libpthread and libdl, separate libraries before glibc 2.34
# (Debian 11). They go in LDFLAGS, which every link uses (ab's ignores LIBS); otherwise
# configure finds "OpenSSL libraries are unusable" and ab fails to link.
./configure \
    --prefix="$prefix" \
    --with-included-apr \
    --with-expat="$deps" \
    --with-pcre="$deps/bin/pcre2-config" \
    --with-ssl="$deps" \
    --with-z="$deps" \
    --enable-mods-shared=all \
    --enable-mpms-shared=all \
    --enable-ssl \
    --enable-proxy --enable-proxy-fcgi --enable-proxy-http --enable-proxy-balancer \
    --enable-rewrite --enable-headers --enable-expires --enable-deflate --enable-vhost-alias \
    --disable-proxy-html --disable-xml2enc --disable-http2 --disable-proxy-http2 \
    --disable-lua --disable-brotli --disable-md --disable-session-crypto \
    --without-crypto --without-sqlite3 --without-pgsql --without-mysql --without-odbc \
    --without-ldap --without-berkeley-db --without-gdbm --without-ndbm \
    LDFLAGS="-L$deps/lib -pthread -Wl,--no-as-needed -ldl" CPPFLAGS="-I$deps/include" LIBS="-ldl -lpthread" \
    `# APR's own UUID code instead of libuuid, which isn't on every system` \
    ac_cv_header_uuid_uuid_h=no ac_cv_header_uuid_h=no ac_cv_func_uuid_create=no ac_cv_func_uuid_generate=no \
    ac_cv_search_uuid_create=no ac_cv_search_uuid_generate=no >/dev/null
make -j"$JOBS" >/dev/null
make install >/dev/null

log "Apache $VERSION: making it relocatable"
# Keep what lampRabit runs; headers, manuals, build files and the sample site go.
rm -rf "$prefix"/{build,cgi-bin,error,htdocs,icons,include,man,manual,logs} "$prefix"/lib/pkgconfig
rm -f "$prefix"/lib/*.la "$prefix"/lib/*.a "$prefix"/lib/*.exp
find "$prefix/bin" -type f ! -name httpd ! -name ab ! -name htpasswd ! -name rotatelogs -delete
find "$prefix/conf" -mindepth 1 ! -name mime.types ! -name magic -exec rm -rf {} +
if [ "$OS" = macos ]; then
    # APR's dylibs are linked by their absolute build path; name them @rpath/… and give
    # every binary and module an rpath of ../lib, which works from bin/ and modules/ alike.
    for lib in "$prefix"/lib/*.dylib; do
        [ -L "$lib" ] && continue
        install_name_tool -id "@rpath/$(basename "$lib")" "$lib"
    done
    for f in "$prefix"/bin/* "$prefix"/modules/*.so "$prefix"/lib/*.dylib; do
        [ -L "$f" ] && continue
        otool -L "$f" | awk -v p="$prefix/lib/" 'index($1, p) == 1 {print $1}' | while read -r dep; do
            install_name_tool -change "$dep" "@rpath/$(basename "$dep")" "$f"
        done
        case "$f" in */lib/*) ;; *) install_name_tool -add_rpath '@loader_path/../lib' "$f" ;; esac
        strip_bin "$f"
        # Editing a binary invalidates its signature, which Apple Silicon refuses to run.
        codesign --force --sign - "$f"
    done
else
    for f in "$prefix"/bin/* "$prefix"/modules/*.so "$prefix"/lib/*.so*; do
        [ -L "$f" ] && continue
        patchelf --set-rpath '$ORIGIN/../lib' "$f"
        strip_bin "$f"
    done
fi
cp LICENSE NOTICE "$prefix/"
licenses "$prefix" "$WORK/openssl-$OPENSSL_VERSION" "$WORK/pcre2-$PCRE2_VERSION" "$WORK/zlib-$ZLIB_VERSION" "$WORK/expat-$EXPAT_VERSION"
check_deps "$prefix/bin/httpd" "$prefix"/modules/*.so
# Run it from another folder, as the app will, to prove nothing points at the build path.
moved="$WORK/moved-apache-$VERSION"
rm -rf "$moved" && cp -R "$prefix" "$moved"
"$moved/bin/httpd" -v
"$moved/bin/httpd" -d "$moved" -f /dev/null -t -C "LoadModule ssl_module modules/mod_ssl.so" -C "LoadModule mpm_event_module modules/mod_mpm_event.so" -C "ServerName localhost" -C "ErrorLog /dev/null"
rm -rf "$moved"
package apache "$VERSION" "$prefix"
