#!/bin/bash
set -euo pipefail

# ── CC Switch Web Server .deb Package Builder ──
# Produces: cc-switch-web_<version>_<architecture>.deb
#
# Prerequisites:
#   - Rust toolchain (rustc, cargo)
#   - Node.js 20+ and pnpm
#   - libgtk-3-dev, libwebkit2gtk-4.0-dev (for native-shell)
#   - dpkg-deb (included in all Debian-based systems)
#
# Usage:
#   ./build-deb.sh                          # Build .deb
#   sudo dpkg -i cc-switch-web_*.deb        # Install

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

# Ensure cargo is in PATH
if command -v ~/.cargo/bin/cargo &>/dev/null; then
  export PATH="$HOME/.cargo/bin:$PATH"
fi

# ── Configuration ──
VERSION="${CC_SWITCH_VERSION:-1.0.0}"
ARCH="${CC_SWITCH_ARCH:-$(dpkg --print-architecture)}"
PACKAGE="cc-switch-web_${VERSION}_${ARCH}"
DEB_FILE="${PACKAGE}.deb"
BUILD_DIR="$(mktemp -d "/tmp/cc-switch-deb-XXXXXX")"

# ── Detect toolchain ──
HAS_GCC10=$(command -v gcc-10 &>/dev/null && echo "yes" || echo "no")

echo "=== Building CC Switch Web Server .deb (v${VERSION}) ==="
echo ""

# ── Step 1: Build frontend ──
echo ">>> [1/5] Building frontend (Vite)..."
if [ ! -d "node_modules" ]; then
  pnpm install --frozen-lockfile || pnpm install
fi
pnpm build:renderer
echo "  ✓ Frontend built to dist/"
echo ""

# ── Step 2: Build web-server binary ──
echo ">>> [2/5] Compiling web-server binary..."
export CARGO_TERM_COLOR=always

if [ "$HAS_GCC10" = "yes" ]; then
  echo "  Using gcc-10 for release build..."
  export CC=gcc-10
  export CXX=g++-10
  cargo build --release -p web-server
  WEB_BINARY="target/release/web-server"
else
  # Ubuntu 20.04: GCC 9 has memcmp bug affecting aws-lc-sys.
  # Build with debug + strip fallback. Install gcc-10 for smaller release build.
  echo "  (gcc-10 not found — using debug profile + strip fallback)"
  AWS_LC_SYS_NO_ASM=1 cargo build -p web-server
  WEB_BINARY="target/debug/web-server"
fi

mkdir -p "$BUILD_DIR/usr/bin"
cp "$WEB_BINARY" "$BUILD_DIR/usr/bin/cc-switch-web-server"
strip "$BUILD_DIR/usr/bin/cc-switch-web-server"
echo "  ✓ web-server stripped: $(du -h "$BUILD_DIR/usr/bin/cc-switch-web-server" | cut -f1)"
echo ""

# ── Step 3: Build native-shell binary ──
echo ">>> [3/5] Compiling native-shell (native window)..."
# native-shell is excluded from the workspace, build independently
cd native-shell
cargo build --release
cd "$SCRIPT_DIR"
cp native-shell/target/release/native-shell "$BUILD_DIR/usr/bin/cc-switch-web"
strip "$BUILD_DIR/usr/bin/cc-switch-web"
echo "  ✓ native-shell stripped: $(du -h "$BUILD_DIR/usr/bin/cc-switch-web" | cut -f1)"
echo ""

# ── Step 4: Assemble package layout ──
echo ">>> [4/5] Assembling .deb package layout..."

mkdir -p "$BUILD_DIR/DEBIAN"
mkdir -p "$BUILD_DIR/usr/share/cc-switch-web/dist"
mkdir -p "$BUILD_DIR/usr/share/cc-switch-web"
mkdir -p "$BUILD_DIR/usr/share/applications"
cp -r dist/* "$BUILD_DIR/usr/share/cc-switch-web/dist/"
cp src-tauri/icons/icon.png "$BUILD_DIR/usr/share/cc-switch-web/cc-switch-web.png"
cp debian/cc-switch-web.desktop "$BUILD_DIR/usr/share/applications/"

# Install app icon
ICON_SIZE=256
ICON_DIR="$BUILD_DIR/usr/share/icons/hicolor/${ICON_SIZE}x${ICON_SIZE}/apps"
mkdir -p "$ICON_DIR"
cp src-tauri/icons/icon.png "$ICON_DIR/cc-switch-web.png"
cp debian/control "$BUILD_DIR/DEBIAN/"
sed -i "s/^Architecture: .*/Architecture: ${ARCH}/" "$BUILD_DIR/DEBIAN/control"
cp debian/copyright "$BUILD_DIR/DEBIAN/"
cp debian/postinst "$BUILD_DIR/DEBIAN/"
cp debian/postrm "$BUILD_DIR/DEBIAN/"
chmod 755 "$BUILD_DIR/DEBIAN/postinst" "$BUILD_DIR/DEBIAN/postrm"

# Compute installed size (in KB) and add to control
INSTALLED_SIZE=$(du -sk "$BUILD_DIR/usr" | cut -f1)
echo "Installed-Size: ${INSTALLED_SIZE}" >> "$BUILD_DIR/DEBIAN/control"

echo "  ✓ Package layout ready"
echo ""

# ── Step 5: Build .deb ──
echo ">>> [5/5] Building .deb package..."
dpkg-deb --build --root-owner-group "$BUILD_DIR" "$DEB_FILE"
rm -rf "$BUILD_DIR"

echo ""
echo "=== Done! ==="
echo "  Package: $DEB_FILE"
echo "  Size:    $(du -h "$DEB_FILE" | cut -f1)"
echo ""
echo "Install:"
echo "  sudo dpkg -i $DEB_FILE"
echo ""
