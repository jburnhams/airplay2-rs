#!/bin/bash
set -e

# Build directory for shairport-sync
BUILD_DIR="$(pwd)/target/shairport-sync"
SRC_DIR="$BUILD_DIR/src"
PREFIX="$BUILD_DIR"

if [ -f "$PREFIX/bin/shairport-sync" ]; then
    echo "shairport-sync is already built at $PREFIX/bin/shairport-sync"
    # exit 0
fi

echo "Installing required dependencies..."
if command -v apt-get >/dev/null 2>&1; then
    sudo apt-get update
    sudo apt-get install -y --no-install-recommends \
        build-essential git autoconf automake libtool \
        libpopt-dev libconfig-dev libasound2-dev \
        avahi-daemon libavahi-client-dev libssl-dev \
        libsoxr-dev xxd pkg-config
fi

mkdir -p "$BUILD_DIR"

echo "Cloning ALAC..."
if [ ! -d "$BUILD_DIR/alac" ]; then
    git clone https://github.com/mikebrady/alac.git "$BUILD_DIR/alac"
fi
cd "$BUILD_DIR/alac"
autoreconf -fi
./configure
make
sudo make install
sudo ldconfig || true

echo "Cloning shairport-sync..."
if [ ! -d "$SRC_DIR" ]; then
    git clone https://github.com/mikebrady/shairport-sync.git "$SRC_DIR"
fi

cd "$SRC_DIR"

echo "Building shairport-sync..."
autoreconf -fi
./configure \
    --prefix="$PREFIX" \
    --with-alsa \
    --with-pipe \
    --with-stdout \
    --with-avahi \
    --with-ssl=openssl \
    --with-soxr \
    --with-metadata \
    --with-apple-alac \
    --with-airplay-2

make -j$(nproc)
make install

echo "shairport-sync successfully built and installed to $PREFIX/bin/shairport-sync"
