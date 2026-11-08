#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

if [ -x target/release/Legenda ]; then
  BIN_SRC="target/release/Legenda"
elif [ -x dist/Legenda/Legenda ]; then
  BIN_SRC="dist/Legenda/Legenda"
else
  echo "Legenda release binary not found. Run 'cargo build --release' first." >&2
  exit 1
fi

rm -rf build/Legenda.AppDir
mkdir -p build/Legenda.AppDir/usr/bin
cp -a "$BIN_SRC" build/Legenda.AppDir/usr/bin/Legenda
chmod +x build/Legenda.AppDir/usr/bin/Legenda

APPIMAGE_VERSION=${APPIMAGE_VERSION:-$(git describe --tags --always 2>/dev/null || printf '0.0.0')}
APPIMAGE_VERSION=${APPIMAGE_VERSION#v}
APPIMAGE_ARCH=${APPIMAGE_ARCH:-x86_64}
sed -e "s|@APPIMAGE_VERSION@|${APPIMAGE_VERSION}|" \
    -e "s|@APPIMAGE_ARCH@|${APPIMAGE_ARCH}|" \
    packaging/linux/legenda.desktop > build/Legenda.AppDir/legenda.desktop
cp packaging/linux/legenda.png build/Legenda.AppDir/
cp packaging/linux/legenda.png build/Legenda.AppDir/.DirIcon
test -s build/Legenda.AppDir/legenda.png
test -s build/Legenda.AppDir/.DirIcon

cat > build/Legenda.AppDir/AppRun <<'RUN'
#!/bin/sh
HERE="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
exec "$HERE/usr/bin/Legenda" "$@"
RUN
chmod +x build/Legenda.AppDir/AppRun

curl --fail --location --retry 3 https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage -o build/appimagetool.AppImage
chmod +x build/appimagetool.AppImage
UPDATE_INFO=${APPIMAGE_UPDATE_INFO:-"zsync|https://github.com/desenrolado-s/Subtitle-Downloader/releases/latest/download/Legenda-linux-x64.AppImage.zsync"}
ARCH=x86_64 APPIMAGE_EXTRACT_AND_RUN=1 build/appimagetool.AppImage -u "$UPDATE_INFO" build/Legenda.AppDir Legenda-linux-x64.AppImage
rm -f Legenda-linux-x64.AppImage.zsync
