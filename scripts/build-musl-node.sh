#!/bin/sh
set -eux

# tesseract-rs's build.rs hard-codes -DCMAKE_CXX_COMPILER=clang++ and -stdlib=libc++,
# so we need real clang + libc++ in the image (gcc/g++ from build-base is not enough).
# Alpine's libc++ links against llvm-libunwind (NOT the GNU libunwind, which conflicts);
# libc++abi symbols are bundled in libc++ itself, no separate package.
# Static libs (openssl-libs-static, zlib-static) are required because musl rust defaults
# to crt-static for build scripts. libc++-static + llvm-libunwind-static provide
# the .a archives we need to statically link the C++ runtime into the .node so
# downstream Alpine users don't need to apk-install libc++.
apk add --no-cache \
  build-base cmake git curl pkgconf perl \
  clang libc++-dev libc++-static llvm-libunwind-dev llvm-libunwind-static \
  tesseract-ocr-dev leptonica-dev \
  openssl-dev openssl-libs-static zlib-static

curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable -t x86_64-unknown-linux-musl
. /root/.cargo/env

# - target-feature=-crt-static: the .node is a cdylib and must dynamically link libc.
# - -l:libc++.a -l:libunwind.a: force-static-link the C++ runtime + unwinder so
#   the resulting .node has no dlopen-time dependency on libc++.so.1 (which is
#   not present on stock alpine / minimal node:20-alpine images).
#   -l:NAME (with the colon) tells the linker to look up an exact filename,
#   bypassing the usual .so > .a preference and shadowing the dylib request
#   that cc-rs emits via `cargo:rustc-link-lib=c++`.
export RUSTFLAGS="-C target-feature=-crt-static -C link-arg=-l:libc++.a -C link-arg=-l:libunwind.a"
npx napi build --cargo-cwd ../../crates/liteparse-napi --platform --release --js false --dts native.d.ts --target x86_64-unknown-linux-musl .
