#!/bin/sh
# Variant f bundles from target/Wisp.app (run scripts/bundle.sh first); launch them with WISP_MENU=a.
#   WispF26.app  UIDesignRequiresCompatibility=YES, LC_BUILD_VERSION sdk rewritten to 26.0
#   WispF27.app  the key on the unchanged sdk-27 binary (Apple: ignored for macOS 27 SDK builds)
#   WispS26.app  sdk 26.0 without the key, to separate the SDK's own effect from the key's
set -eu
cd "$(dirname "$0")/../.."
make() { # <name> <sdk|-> <key yes|no>
    APP=target/$1.app
    rm -rf "$APP"; cp -R target/Wisp.app "$APP"
    if [ "$2" != - ]; then
        vtool -set-build-version macos 11.0 "$2" -replace -output "$APP/Contents/MacOS/wisp.new" "$APP/Contents/MacOS/wisp"
        mv "$APP/Contents/MacOS/wisp.new" "$APP/Contents/MacOS/wisp"
    fi
    [ "$3" = yes ] && plutil -insert UIDesignRequiresCompatibility -bool YES "$APP/Contents/Info.plist"
    codesign --force --sign - "$APP" 2>/dev/null
    printf '%s: ' "$APP"; vtool -show-build "$APP/Contents/MacOS/wisp" | awk '/sdk/ { printf "sdk %s ", $2 }'
    plutil -extract UIDesignRequiresCompatibility raw "$APP/Contents/Info.plist" 2>/dev/null || echo "(no key)"
}
make WispF26 26.0 yes
make WispF27 - yes
make WispS26 26.0 no
