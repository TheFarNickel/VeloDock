#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="$(grep -m1 '^version' "$ROOT_DIR/Cargo.toml" | sed -E 's/.*"([^"]+)".*/\1/')"
ARCH="$(uname -m)"
PKG_DIR="velodock-${VERSION}-${ARCH}"
DIST_DIR="$ROOT_DIR/dist"
STAGE="$DIST_DIR/$PKG_DIR"
TARBALL="$DIST_DIR/${PKG_DIR}.tar.gz"

echo "==> Building VeloDock v${VERSION} (${ARCH}) release binary..."
cd "$ROOT_DIR"
cargo build --release

rm -rf "$STAGE" "$TARBALL"
mkdir -p "$STAGE/icons"

cp "$ROOT_DIR/target/release/velodock" "$STAGE/"
cp "$ROOT_DIR/packaging/velodock.desktop" "$STAGE/"
cp "$ROOT_DIR/packaging/velodock-settings.desktop" "$STAGE/"
cp "$ROOT_DIR/assets/icons/"*.svg "$STAGE/icons/"
cp "$ROOT_DIR/assets/icons/"*.png "$STAGE/icons/"
cp "$ROOT_DIR/LICENSE" "$STAGE/"
cp "$ROOT_DIR/README.md" "$STAGE/"

tar -C "$DIST_DIR" -czf "$TARBALL" "$PKG_DIR"
rm -rf "$STAGE"

SHA256="$(sha256sum "$TARBALL" | awk '{print $1}')"
echo ""
echo "==> Built precompiled release tarball:"
echo "    $TARBALL"
echo "==> SHA256:"
echo "    $SHA256"
