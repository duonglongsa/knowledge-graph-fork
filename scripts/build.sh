#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
RELEASE_DIR="$PROJECT_ROOT/release"

PLATFORM=${PLATFORM:-$(uname -s)}
PLATFORM=$(echo "$PLATFORM" | tr '[:upper:]' '[:lower:]')

ARCH=${ARCH:-$(uname -m)}
if [ "$ARCH" = "arm64" ]; then
    ARCH="aarch64"
fi
CARGO_PARAMS=${CARGO_PARAMS:-"--locked --release"}
BIN_NAME=${BIN_NAME:-gkg}

echo "Building for $PLATFORM $ARCH (bin: $BIN_NAME)"

case "$PLATFORM" in
    darwin)
        case "$ARCH" in
            aarch64|arm64)
                TARGET="aarch64-apple-darwin"
                ;;
            x86_64)
                TARGET="x86_64-apple-darwin"
                ;;
        esac
        ;;
    linux)
        case "$ARCH" in
            aarch64)
                TARGET="aarch64-unknown-linux-gnu"
                ;;
            x86_64)
                TARGET="x86_64-unknown-linux-gnu"
                ;;
        esac
        ;;
esac

if [ -z "$TARGET" ]; then
    echo "unknown arch '$ARCH' or platform '$PLATFORM'"
    exit 1
fi

build_bin() {
    if [ "$BIN_NAME" = "gkg" ]; then
        cargo build $CARGO_PARAMS --bin gkg --target $TARGET
        tar -czvf gkg-${PLATFORM}-${ARCH}.tar.gz -C target/${TARGET}/release gkg
        echo "created gkg-${PLATFORM}-${ARCH}.tar.gz"
        return 0
    fi

    if [ "$BIN_NAME" = "http-server-deployed" ]; then
        if [ "$PLATFORM" != "linux" ]; then
            echo "http-server-deployed packaging is supported only on linux"
            exit 1
        fi
        cargo build $CARGO_PARAMS --bin http-server-deployed --target $TARGET
        PKG_DIR=$(mktemp -d)
        cp target/${TARGET}/release/http-server-deployed ${PKG_DIR}/gkg-server
        tar -czvf gkg-server-${ARCH}.tar.gz -C ${PKG_DIR} gkg-server
        rm -rf ${PKG_DIR}
        echo "created gkg-server-${ARCH}.tar.gz"
        return 0
    fi

    echo "Unknown BIN_NAME '$BIN_NAME'. Supported: gkg, http-server-deployed"
    exit 1
}

build_bin
