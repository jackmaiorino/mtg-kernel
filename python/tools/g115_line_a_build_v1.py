"""Engineering build of the line (a) native binaries from a clean detached worktree.

Builds public_feature_evaluation_v1 and public_feature_training_v1 with the
feature and profile the D3 Windows build used, at BelowNormal priority with at
most four jobs, into the lane's target directory. It records the commit, a
clean-tree check, the toolchain and every binary hash, and pins each binary
into E:/pinned-binaries/<sha256>/ (artifact law clause 4). This is not the
yardstick build: CODEX #523 C3 requires one new integrated executable, pinned
before calibration, once the other lanes' loading support is merged.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import time

import g115_line_a_guard_v1 as guard

BINARIES = ('public_feature_evaluation_v1', 'public_feature_training_v1')
FEATURE = 'experimental-burn-net8-packed-cuda-v1'
TARGET = 'D:/cargo-target/opus-line-a-launcher'
BUILD_ENVIRONMENT = 'E:/mtg-meta-recovery-20260921/public-human-bridge-tools-005/build-environment.cmd'
CUDA_BIN = 'C:/Program Files/NVIDIA GPU Computing Toolkit/CUDA/v12.8/bin'


def git(repo, *args):
    return subprocess.check_output(['git', *args], cwd=repo, text=True).strip()


def build_environment(temp):
    env = os.environ.copy()
    output = subprocess.check_output(['cmd', '/d', '/c', BUILD_ENVIRONMENT], text=True)
    for line in output.splitlines():
        if '=' in line and not line.startswith('='):
            key, value = line.split('=', 1)
            env[key] = value
    env.update(GIT_OPTIONAL_LOCKS='0', CARGO_TARGET_DIR=TARGET, TEMP=str(temp), TMP=str(temp))
    env['PATH'] = CUDA_BIN + ';' + env.get('Path', env.get('PATH', ''))
    return env


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument('--repo', type=Path, required=True, help='Clean detached worktree to build')
    parser.add_argument('--root', type=Path, required=True, help='Fresh receipt directory')
    parser.add_argument('--pinned-root', default=guard.PINNED_ROOT)
    args = parser.parse_args()
    guard.require(not git(args.repo, 'status', '--porcelain'), 'Build worktree must be clean')
    commit = git(args.repo, 'rev-parse', 'HEAD')
    args.root.mkdir(parents=True)
    temp = args.root / 'temp'
    temp.mkdir()
    env = build_environment(temp)
    command = ['cargo', '--config', 'profile.release.package.mtg-kernel.codegen-units=4', 'build', '-p', 'mtg-kernel',
               '--release', '--features', FEATURE, '-j', '4']
    for name in BINARIES:
        command += ['--bin', name]
    receipt = dict(schema='g115-line-a-engineering-build/v1', commit=commit, clean=True, command=command,
                   target=TARGET, toolchain=dict(rustc=subprocess.check_output(['rustc', '-Vv'], env=env, text=True),
                                                 cargo=subprocess.check_output(['cargo', '-V'], env=env, text=True),
                                                 vc_tools=env.get('VCToolsVersion')),
                   started_unix=time.time(), scope='engineering qualification build, not the yardstick build')
    flags = subprocess.BELOW_NORMAL_PRIORITY_CLASS | subprocess.CREATE_NO_WINDOW
    with (args.root / 'build.log').open('xb') as log:
        result = subprocess.run(command, cwd=args.repo, env=env, stdout=log, stderr=subprocess.STDOUT,
                                creationflags=flags)
    receipt.update(finished_unix=time.time(), exit_code=result.returncode)
    guard.require(not git(args.repo, 'status', '--porcelain'), 'Build changed the worktree')
    if result.returncode == 0:
        receipt['binaries'] = {}
        for name in BINARIES:
            pinned, digest = guard.pin_binary(Path(TARGET) / 'release' / (name + '.exe'), args.pinned_root)
            receipt['binaries'][name] = dict(sha256=digest, pinned=str(pinned).replace('\\', '/'))
    (args.root / 'build-receipt.json').write_text(json.dumps(receipt, indent=2) + '\n', encoding='utf-8')
    print(json.dumps({k: v for k, v in receipt.items() if k != 'toolchain'}, indent=2))
    raise SystemExit(result.returncode)


if __name__ == '__main__':
    main()
