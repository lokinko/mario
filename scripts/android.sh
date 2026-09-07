#!/usr/bin/env bash
set -euo pipefail

action="${1:-build}"
shift || true
root_dir="$(cd "$(dirname "$0")/.." && pwd)"

export JAVA_HOME="${JAVA_HOME:-/opt/homebrew/opt/openjdk@17/libexec/openjdk.jdk/Contents/Home}"
export ANDROID_HOME="${ANDROID_HOME:-/opt/homebrew/share/android-commandlinetools}"
rustup_bin="/opt/homebrew/opt/rustup/bin"
export PATH="$rustup_bin:$ANDROID_HOME/cmdline-tools/latest/bin:$ANDROID_HOME/platform-tools:$PATH"

# Cargo still compiles build scripts for the macOS host during an Android build.
# Prefer the standalone Command Line Tools when the selected full Xcode install is
# temporarily incompatible with the current macOS runtime.
clt_root="/Library/Developer/CommandLineTools"
if [[ -x "$clt_root/usr/bin/clang" && -d "$clt_root/SDKs/MacOSX.sdk" ]]; then
  export DEVELOPER_DIR="${DEVELOPER_DIR:-$clt_root}"
  export PATH="$clt_root/usr/bin:$PATH"
  export SDKROOT="${SDKROOT:-$clt_root/SDKs/MacOSX.sdk}"
  export CC="${CC:-clang}"
  export CXX="${CXX:-clang++}"
  export AR="${AR:-ar}"
  export RANLIB="${RANLIB:-ranlib}"
  export CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER="${CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER:-$clt_root/usr/bin/clang}"
fi

if [[ -z "${NDK_HOME:-}" ]]; then
  ndk_home="$(find "$ANDROID_HOME/ndk" -mindepth 1 -maxdepth 1 -type d 2>/dev/null | sort -V | tail -1)"
  if [[ -z "$ndk_home" ]]; then
    echo "Android NDK 未安装。请先通过 sdkmanager 安装 NDK (Side by side)。" >&2
    exit 1
  fi
  export NDK_HOME="$ndk_home"
fi

configure_android_manifest() {
  manifest="$root_dir/client/src-tauri/gen/android/app/src/main/AndroidManifest.xml"
  gradle_file="$root_dir/client/src-tauri/gen/android/app/build.gradle.kts"
  if [[ ! -f "$manifest" ]]; then
    echo "Android 工程尚未初始化，请先运行 npm run android:init。" >&2
    exit 1
  fi
  if grep -q 'android:usesCleartextTraffic=' "$manifest"; then
    perl -0pi -e 's/android:usesCleartextTraffic="[^"]*"/android:usesCleartextTraffic="true"/' "$manifest"
  else
    perl -0pi -e 's/<application/<application android:usesCleartextTraffic="true"/' "$manifest"
  fi
  # Tauri's generated debug profile retains hundreds of MB of native symbols.
  # The installable smoke-test APK does not need them; Cargo artifacts remain
  # available locally if native debugging is required.
  if [[ -f "$gradle_file" ]]; then
    perl -0pi -e 's/\n\s*packaging\s*\{[^{}]*jniLibs\.keepDebugSymbols[^{}]*\}//s' "$gradle_file"
  fi
}

case "$action" in
  init)
    npm run tauri --prefix "$root_dir/client" -- android init --ci --skip-targets-install "$@"
    configure_android_manifest
    ;;
  build)
    configure_android_manifest
    find "$root_dir/client/src-tauri/gen/android/app/src/main/jniLibs" \
      -type l -name 'libmario_client_lib.so' -delete 2>/dev/null || true
    read -r -a android_targets <<< "${MARIO_ANDROID_TARGETS:-aarch64}"
    npm run tauri --prefix "$root_dir/client" -- android build --debug --apk --ci --target "${android_targets[@]}" "$@"
    source_apk="$root_dir/client/src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk"
    if [[ ! -f "$source_apk" ]]; then
      echo "未找到预期的通用调试 APK: $source_apk" >&2
      exit 1
    fi
    app_version="$(node -p 'require(process.argv[1]).version' "$root_dir/client/package.json")"
    target_label="$(IFS=-; echo "${android_targets[*]}")"
    destination_apk="$root_dir/outputs/mario_${app_version}_android-${target_label}-debug.apk"
    mkdir -p "$root_dir/outputs"

    package_dir="$(mktemp -d)"
    trap 'rm -rf "$package_dir"' EXIT
    working_apk="$package_dir/mario-unaligned.apk"
    aligned_apk="$package_dir/mario-aligned.apk"
    staged_libs="$package_dir/staged"
    cp "$source_apk" "$working_apk"

    strip_bin="$NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin/llvm-strip"
    lib_entries="$(unzip -Z1 "$source_apk" | rg '^lib/[^/]+/libmario_client_lib\.so$')"
    for lib_entry in $lib_entries; do
      mkdir -p "$staged_libs/$(dirname "$lib_entry")"
      unzip -p "$source_apk" "$lib_entry" > "$staged_libs/$lib_entry"
      "$strip_bin" --strip-unneeded "$staged_libs/$lib_entry"
      zip -q -d "$working_apk" "$lib_entry"
      (cd "$staged_libs" && zip -q -0 "$working_apk" "$lib_entry")
    done

    build_tools="$(find "$ANDROID_HOME/build-tools" -mindepth 1 -maxdepth 1 -type d | sort -V | tail -1)"
    debug_keystore="${ANDROID_DEBUG_KEYSTORE:-$HOME/.android/debug.keystore}"
    "$build_tools/zipalign" -f -p 4 "$working_apk" "$aligned_apk"
    if [[ -f "$destination_apk" ]]; then
      unlink "$destination_apk"
    fi
    "$build_tools/apksigner" sign \
      --ks "$debug_keystore" \
      --ks-key-alias "${ANDROID_DEBUG_KEY_ALIAS:-androiddebugkey}" \
      --ks-pass "pass:${ANDROID_DEBUG_KEYSTORE_PASSWORD:-android}" \
      --key-pass "pass:${ANDROID_DEBUG_KEY_PASSWORD:-android}" \
      --out "$destination_apk" \
      "$aligned_apk"
    "$build_tools/apksigner" verify "$destination_apk"
    echo "Prepared APK: $destination_apk"
    ;;
  *)
    echo "用法: scripts/android.sh init|build [额外 Tauri 参数]" >&2
    exit 2
    ;;
esac
