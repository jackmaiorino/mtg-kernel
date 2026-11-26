# Replaces the container and toolchain setup in the build.sh plumbing test.
work=$PWD
repo=$work/src
target=$work/target
out=$work/out
cargo_home=$work/cargo-home
commit=$1
origin=fixture
source_epoch=123
export LC_ALL=C TZ=UTC PATH="$work/toolchain:$PATH"
mkdir -p "$repo" "$out" "$work/toolchain"
check_stop() { :; }
check_checkout() { :; }

# env -i in build_target finds this stand-in through the same PATH as Cargo.
cat >"$work/toolchain/cargo" <<'SH'
#!/bin/sh
set -eu
IFS=$(printf '\t')
printf '%s\n' "$*" >>"$CARGO_TARGET_DIR/cargo-args"
printf '%s\n' "${CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS:-}" >>"$CARGO_TARGET_DIR/rustflags"
cp -R "$CARGO_TARGET_DIR/../prebuilt/." "$CARGO_TARGET_DIR"
SH
chmod +x "$work/toolchain/cargo"

cmp() {
    printf '%s\t%s\n' "${2#"$target"/}" "${3#"$out"/}" >>"$work/comparisons"
    command cmp "$@"
}

# The Python manifest writer has a separate fixture with stubbed ELF/tool queries.
python=fixture_manifest
fixture_manifest() {
    printf '%s\n' "$@" >"$work/manifest-args"
    touch "$out/BUILD-MANIFEST.json" "$out/rust-components.json" "$out/debian-packages.tsv" "$out/build.log"
}

sha256sum() {
    printf '%s\n' "$*" >>"$work/checksum-commands"
    command sha256sum "$@"
}

sync() { printf '%s\n' "$*" >>"$work/sync-commands"; }
