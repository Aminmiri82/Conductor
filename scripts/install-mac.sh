#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BUILT="$ROOT/src-tauri/target/release/bundle/macos/Conductor.app"
DEST="/Applications/Conductor.app"
IDENTIFIER="com.yaramiri.conductor"
PLIST_BUDDY="/usr/libexec/PlistBuddy"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "This command only installs the macOS app." >&2
  exit 1
fi

cd "$ROOT"
pnpm tauri build --bundles app --config '{"bundle":{"macOS":{"signingIdentity":"-"}}}'

bundle_value() {
  "$PLIST_BUDDY" -c "Print :$2" "$1/Contents/Info.plist"
}

if [[ ! -d "$BUILT" ]] || [[ "$(bundle_value "$BUILT" CFBundleIdentifier)" != "$IDENTIFIER" ]]; then
  echo "Build did not produce the expected Conductor.app bundle." >&2
  exit 1
fi

if [[ -L "$DEST" ]]; then
  echo "$DEST is a symlink; refusing to replace it." >&2
  exit 1
fi

if [[ -e "$DEST" ]] && [[ "$(bundle_value "$DEST" CFBundleIdentifier)" != "$IDENTIFIER" ]]; then
  echo "$DEST has a different bundle identifier; refusing to replace it." >&2
  exit 1
fi

if [[ -e "$DEST" ]]; then
  EXECUTABLE="$(bundle_value "$DEST" CFBundleExecutable)"
  if lsof -t "$DEST/Contents/MacOS/$EXECUTABLE" >/dev/null 2>&1; then
    echo "Asking the installed Conductor to quit..."
    if ! osascript -e 'tell application "/Applications/Conductor.app" to quit'; then
      echo "Could not quit Conductor automatically. Quit it and rerun pnpm install:mac." >&2
      exit 1
    fi
    for _ in {1..30}; do
      if ! lsof -t "$DEST/Contents/MacOS/$EXECUTABLE" >/dev/null 2>&1; then
        break
      fi
      sleep 1
    done
    if lsof -t "$DEST/Contents/MacOS/$EXECUTABLE" >/dev/null 2>&1; then
      echo "Conductor is still running. Quit it and rerun pnpm install:mac." >&2
      exit 1
    fi
  fi
fi

STAGING="$(mktemp -d /Applications/.Conductor-install.XXXXXX)"
BACKUP="$STAGING/previous.app"
INSTALL_COMPLETE=0
restore_or_clean() {
  if [[ "$INSTALL_COMPLETE" == 1 ]]; then
    rm -rf "$STAGING"
  elif [[ -d "$BACKUP" && ! -e "$DEST" ]]; then
    mv "$BACKUP" "$DEST"
    rm -rf "$STAGING"
  elif [[ -d "$BACKUP" ]]; then
    echo "Previous app was kept at $BACKUP; restore it manually if needed." >&2
  else
    rm -rf "$STAGING"
  fi
}
trap restore_or_clean EXIT

ditto "$BUILT" "$STAGING/Conductor.app"
if [[ "$(bundle_value "$STAGING/Conductor.app" CFBundleIdentifier)" != "$IDENTIFIER" ]]; then
  echo "Copied app failed the bundle identifier check." >&2
  exit 1
fi

if [[ -e "$DEST" ]]; then
  if lsof -t "$DEST/Contents/MacOS/$EXECUTABLE" >/dev/null 2>&1; then
    echo "Conductor started again. Quit it and rerun pnpm install:mac." >&2
    exit 1
  fi
  mv "$DEST" "$BACKUP"
fi
mv "$STAGING/Conductor.app" "$DEST"
INSTALL_COMPLETE=1

echo "Installed $DEST. App data in ~/Library/Application Support/$IDENTIFIER/ was not touched."
