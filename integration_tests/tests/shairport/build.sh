#!/bin/bash
set -e

SHAIRPORT_VERSION="4.3.4"
TARGET_DIR="target/shairport-sync"
BIN_PATH="$TARGET_DIR/bin/shairport-sync"

# Move to repo root
cd "$(dirname "$0")/../../.." || exit 1
REPO_ROOT=$(pwd)

TARGET_DIR="$REPO_ROOT/target/shairport-sync"
BIN_PATH="$TARGET_DIR/bin/shairport-sync"

echo "Checking for shairport-sync cache at $BIN_PATH..."

if [ -x "$BIN_PATH" ]; then
    VERSION_OUT=$("$BIN_PATH" --version 2>&1 || true)
    if echo "$VERSION_OUT" | grep -q "$SHAIRPORT_VERSION"; then
        echo "Valid shairport-sync cache found ($SHAIRPORT_VERSION)."
        exit 0
    else
        echo "Found different version: $VERSION_OUT. Rebuilding..."
    fi
else
    echo "No valid binary found. Building..."
fi

rm -rf "$TARGET_DIR/src"
mkdir -p "$TARGET_DIR"

echo "Cloning shairport-sync $SHAIRPORT_VERSION..."
git clone --depth 1 --branch "$SHAIRPORT_VERSION" https://github.com/mikebrady/shairport-sync "$TARGET_DIR/src"

cd "$TARGET_DIR/src"

echo "Running autoreconf..."
autoreconf -fi

echo "Running configure..."
./configure --with-ssl=openssl \
            --with-soxr \
            --with-avahi \
            --with-airplay-2 \
            --with-pipe \
            --with-stdout \
            --with-metadata \
            --with-apple-alac \
            --prefix="$TARGET_DIR" \
            --sysconfdir=/tmp/shairport-sync-test

echo "Building..."
make -j$(nproc)

echo "Installing..."
make install

echo "Verifying..."
"$BIN_PATH" --version

echo "Done!"
