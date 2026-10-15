#!/usr/bin/env bash
# Sign the APK Tauri just built with a key made on the spot, so it installs
# on a phone (Android refuses unsigned APKs). The key is not kept: each
# release is signed differently, which means updating is uninstall and
# install again. A kept key can replace this when the app is published.
#
# usage: scripts/sign-apk.sh <out dir> [file name]
set -euo pipefail
out="${1:-out}"
name="${2:-graphite-android-arm64.apk}"
mkdir -p "$out"
apk=$(find apps/desktop/src-tauri/gen/android/app/build/outputs/apk -name '*.apk' | grep -v debug | head -1)
if [ -z "$apk" ]; then
  echo "no APK found" >&2
  exit 1
fi
tools=$(ls -d "$ANDROID_HOME"/build-tools/* | sort -V | tail -1)
keystore=$(mktemp -d)/ci.jks
keytool -genkeypair -keystore "$keystore" -alias graphite -storepass graphite -keypass graphite \
  -dname "CN=Graphite build" -keyalg RSA -keysize 2048 -validity 3650 >/dev/null
"$tools/zipalign" -p -f 4 "$apk" "$out/aligned.apk"
"$tools/apksigner" sign --ks "$keystore" --ks-pass pass:graphite --key-pass pass:graphite --out "$out/$name" "$out/aligned.apk"
rm -f "$out/aligned.apk" "$out/aligned.apk.idsig"
ls -l "$out"
