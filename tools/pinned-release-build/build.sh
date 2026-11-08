#!/usr/bin/env bash
# Usage: build.sh SOURCE_COMMIT /work/out
# Use the amd64 image mirror.gcr.io/library/python@sha256:3c3ba8ad4c85eb63b7568f806434f14e33370056985da7fdf2673621cb1d7e93.
# The launcher owns the image pin, target-volume admission, disk and memory monitoring, and wall limit.
set -euo pipefail
umask 022

if [[ $# != 2 || ! $1 =~ ^[0-9a-f]{40}$ || $2 != /work/out ]]; then
    printf 'Usage: build.sh SOURCE_COMMIT /work/out\n' >&2
    exit 2
fi
[[ $EUID == 0 && $(uname -m) == x86_64 ]]
grep -qx 'VERSION_CODENAME=bookworm' /etc/os-release
[[ $(realpath -e -- /work) == /work && -d /work && -w /work ]]
readonly work=/work/build
readonly repo=$work/src
readonly target=$work/target
readonly cargo_home=$work/cargo-home
readonly rust=$work/toolchain
readonly out=$2
readonly python=/usr/local/bin/python3.12
readonly commit=$1
readonly origin=https://github.com/jackmaiorino/mtg-kernel.git
readonly -a bins=(native_expanded_training_run_v1 checkpoint_container_v1)

check_stop() {
    if [[ -e /work/STOP || -L /work/STOP ]]; then
        printf 'Build STOP is present\n' >&2
        exit 75
    fi
}

check_stop
[[ -x $python ]]
mkdir -- "$work" "$out"
exec >"$out/build.log" 2>&1
export PATH=/usr/local/bin:/usr/bin:/bin LC_ALL=C.UTF-8 TZ=UTC DEBIAN_FRONTEND=noninteractive
export GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null GIT_TERMINAL_PROMPT=0

apt-get -o Acquire::Retries=1 update
check_stop
apt-get -o Acquire::Retries=1 install --yes --no-install-recommends \
    gcc binutils libc6-dev git xz-utils ca-certificates zstd
[[ $(getconf GNU_LIBC_VERSION) == 'glibc 2.36' ]]
[[ $(gcc -dumpfullversion) == 12.* ]]
ld_version=$(ld --version)
[[ ${ld_version%%$'\n'*} == 'GNU ld (GNU Binutils for Debian) 2.40' ]]
dpkg-query -W -f='${binary:Package}\t${Version}\n' >"$out/debian-packages.tsv"

check_stop
"$python" -B - "$work" "$rust" "$out" "/work/STOP" <<'PY'
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
from urllib.request import urlopen

work, prefix, out, stop = map(Path, sys.argv[1:])
components = (
    ("rustc", 78645148, "0a6b16ca476461c6238f48efcb74a7aa08a3983e61adf587f208ba48504d87e1"),
    ("cargo", 10972128, "03595624775204f80ff3a67986f69c8918e4cd311877ced635108888013a1e39"),
    ("rust-std", 29846424, "74109ee2c19098f1d7122087dd338a8f643203b3ce678c9d58169db733e17a03"),
)
verified = []
for name, size, digest in components:
    if stop.exists() or stop.is_symlink():
        sys.exit(75)
    stem = f"{name}-1.94.1-x86_64-unknown-linux-gnu"
    url = f"https://static.rust-lang.org/dist/2026-03-26/{stem}.tar.xz"
    # Install each verified package before downloading the next one.
    with tempfile.TemporaryDirectory(prefix=f"rust-{name}-", dir=work) as temporary:
        stage = Path(temporary)
        archive = stage / f"{stem}.tar.xz"
        hashed = hashlib.sha256()
        count = 0
        with urlopen(url, timeout=45) as response, archive.open("xb") as stream:
            while chunk := response.read(1024 * 1024):
                if stop.exists() or stop.is_symlink():
                    sys.exit(75)
                count += len(chunk)
                if count > size:
                    raise ValueError(f"{name} exceeds its pinned download size")
                hashed.update(chunk)
                stream.write(chunk)
        if count != size or hashed.hexdigest() != digest:
            raise ValueError(f"{name} differs from its pinned size or SHA-256")
        subprocess.run(["tar", "-xJf", archive, "-C", stage], check=True)
        extracted = stage / stem
        subprocess.run([extracted / "install.sh", f"--prefix={prefix}", "--disable-ldconfig"],
                       cwd=extracted, check=True)
    verified.append({"name": name, "url": url, "bytes": size, "sha256": digest})
(out / "rust-components.json").write_text(json.dumps(verified, indent=2) + "\n", encoding="utf-8")
PY

export PATH="$rust/bin:$PATH"
[[ $(rustc --version) == 'rustc 1.94.1 '* && $(cargo --version) == 'cargo 1.94.1 '* ]]
check_stop
git init --quiet "$repo"
git -C "$repo" config core.autocrlf false
git -C "$repo" remote add origin "$origin"
git -C "$repo" fetch --depth=1 --no-tags --recurse-submodules=no origin "$commit"
git -C "$repo" checkout --quiet --detach FETCH_HEAD

check_checkout() {
    local status
    [[ $(git -C "$repo" rev-parse HEAD) == "$commit" ]]
    status=$(git -C "$repo" status --porcelain=v1 --untracked-files=all)
    [[ -z $status ]]
}

check_checkout
"$python" -B - "$repo" <<'PY'
from pathlib import Path
import sys
import tomllib

repo = Path(sys.argv[1])
with (repo / "rust-toolchain.toml").open("rb") as stream:
    toolchain = tomllib.load(stream)["toolchain"]
with (repo / "Cargo.toml").open("rb") as stream:
    release = tomllib.load(stream)["profile"]["release"]
if toolchain["channel"] != "1.94.1" or release.get("lto") != "thin" or release.get("codegen-units") != 1:
    raise ValueError("source requires a different Rust toolchain or release profile")
PY

source_epoch=$(git -C "$repo" show -s --format=%ct HEAD)
mkdir -- "$work/home" "$work/tmp" "$target" "$cargo_home"
for bin in "${bins[@]}"; do
    check_stop
    # Build the binaries sequentially so thin LTO cannot link both at once.
    (
        cd -- "$repo"
        env -i PATH="$PATH" HOME="$work/home" TMPDIR="$work/tmp" LC_ALL="$LC_ALL" TZ="$TZ" \
            GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null GIT_TERMINAL_PROMPT=0 \
            SOURCE_DATE_EPOCH="$source_epoch" CARGO_HOME="$cargo_home" CARGO_TARGET_DIR="$target" \
            CARGO_INCREMENTAL=0 CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=/usr/bin/gcc \
            cargo build --release --locked --jobs 2 -p mtg-kernel --bin "$bin"
    )
done

check_stop
check_checkout
for bin in "${bins[@]}"; do
    install -m 0755 -- "$target/release/$bin" "$out/$bin"
    cmp -- "$target/release/$bin" "$out/$bin"
done

"$python" -B - "$repo" "$out" "$commit" "$origin" "$source_epoch" "${bins[@]}" <<'PY'
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys

repo, out = map(Path, sys.argv[1:3])
commit, origin, epoch, *binaries = sys.argv[3:]

def cmd(*args):
    return subprocess.check_output(args, text=True).strip()

def pin(path):
    with path.open("rb") as stream:
        return {"name": path.name, "bytes": path.stat().st_size,
                "sha256": hashlib.file_digest(stream, "sha256").hexdigest()}

def executable(name):
    path = out / name
    headers = cmd("readelf", "-h", "-l", path)
    libraries = cmd("ldd", path)
    if ("Advanced Micro Devices X86-64" not in headers
            or "/lib64/ld-linux-x86-64.so.2" not in headers or "not found" in libraries):
        raise ValueError(f"{name} must be a loadable Linux x86-64 ELF")
    mapped = [re.sub(r" \(0x[0-9a-f]+\)$", "", line.strip()) for line in libraries.splitlines()]
    return dict(pin(path), runtime_libraries=mapped)

ld = Path(cmd("gcc", "-print-prog-name=ld"))
if not ld.is_absolute():
    ld = Path("/usr/bin") / ld
manifest = {
    "schema": "fleet-combined-trainer-build/v1",
    "source_commit": commit,
    "source_tree": cmd("git", "-C", repo, "rev-parse", "HEAD^{tree}"),
    "source_origin": origin,
    "source_checkout_clean": True,
    "source_date_epoch": int(epoch),
    "cargo_lock": pin(repo / "Cargo.lock"),
    "build": {"profile": "release", "lto": "thin", "codegen_units": 1, "jobs": 2,
              "locked": True, "incremental": False, "binaries": binaries},
    "tools": {
        "rustc": cmd("rustc", "-vV"), "cargo": cmd("cargo", "--version"),
        "gcc": cmd("gcc", "--version").splitlines()[0],
        "linker_driver": "/usr/bin/gcc", "linker": str(ld.resolve(strict=True)),
        "linker_version": cmd(ld, "--version").splitlines()[0],
        "glibc": cmd("getconf", "GNU_LIBC_VERSION"),
        "python": cmd(sys.executable, "--version"), "zstd": cmd("zstd", "--version"),
        "rust_components": pin(out / "rust-components.json"),
        "debian_packages": pin(out / "debian-packages.tsv"),
    },
    "trainer": executable(binaries[0]),
    "checkpoint_container": executable(binaries[1]),
}
(out / "BUILD-MANIFEST.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
PY

(
    cd -- "$out"
    sha256sum -- "${bins[@]}" BUILD-MANIFEST.json rust-components.json debian-packages.tsv >SHA256SUMS
    sha256sum --check SHA256SUMS
    sync -- "${bins[@]}" BUILD-MANIFEST.json rust-components.json debian-packages.tsv SHA256SUMS build.log
)

check_stop
date -u +%FT%TZ >"$out/DONE"
sync -- "$out/DONE"
