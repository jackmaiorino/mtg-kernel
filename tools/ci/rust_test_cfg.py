"""Which mtg-kernel library tests a feature set compiles, from the source, for the release-trainer checks.

Walks the module tree from mtg-kernel/src/lib.rs into the modules the trainer filters select, and records each
`#[test]` function's full libtest name and its effective cfg predicate (its own `#[cfg]` attributes and those of
every enclosing module and module declaration). A predicate is evaluated for a Linux release test build
(`test` on, `debug_assertions` off) with a feature set and its Cargo implications.

The trainer-tests job compares the names predicted for its feature set with the compiled inventory and fails on
any difference, so the prediction is checked against the compiler on every run. An extra feature set then runs
exactly the tests it compiles that the default build does not.

Usage:
  rust_test_cfg.py predict FEATURES [linux|windows]       names the build compiles under the trainer filters
  rust_test_cfg.py feature-only FEATURES [linux|windows]  names FEATURES compiles that the default build does not
  rust_test_cfg.py uncovered                              selected tests no release-trainer row compiles
FEATURES is a comma-separated list ("" for default features).
"""
from __future__ import annotations

import re
import sys
from functools import lru_cache
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SRC = ROOT / "mtg-kernel" / "src"
sys.path.insert(0, str(Path(__file__).resolve().parent))
import ci_scope  # noqa: E402

TOKEN = re.compile(r"""
    (?P<ws>\s+)
  | (?P<line>//[^\n]*)
  | (?P<block>/\*)
  | (?P<raw>b?r(?P<hashes>\#*)")
  | (?P<str>b?"(?:\\.|[^"\\])*")
  | (?P<char>b?'(?:\\(?:x[0-9a-fA-F]{2}|u\{[0-9a-fA-F]+\}|.)|[^'\\\n])')
  | (?P<life>'[A-Za-z_][A-Za-z0-9_]*)
  | (?P<ident>[A-Za-z_][A-Za-z0-9_]*)
  | (?P<num>[0-9][0-9A-Za-z_.]*)
  | (?P<punct>.)
""", re.VERBOSE | re.DOTALL)


def tokens(text: str) -> list[str]:
    out, i = [], 0
    while i < len(text):
        m = TOKEN.match(text, i)
        kind = m.lastgroup if m.lastgroup != "hashes" else "raw"
        if m.group("raw") is not None:
            end = text.index('"' + m.group("hashes"), m.end())
            out.append('""')
            i = end + 1 + len(m.group("hashes"))
            continue
        if kind == "block":
            depth, j = 1, m.end()
            while depth:
                nxt_open, nxt_close = text.find("/*", j), text.find("*/", j)
                if nxt_open != -1 and nxt_open < nxt_close:
                    depth, j = depth + 1, nxt_open + 2
                else:
                    depth, j = depth - 1, nxt_close + 2
            i = j
            continue
        if kind in ("str", "char"):
            out.append(m.group(kind) if kind == "str" else "''")
        elif kind in ("ident", "punct", "num", "life"):
            out.append(m.group(kind))
        i = m.end()
    return out


def parse_cfg(toks: list[str], i: int = 0):
    """Parse one cfg predicate starting at toks[i]; return (tree, next index)."""
    name = toks[i]
    if toks[i + 1] == "(" and name in ("all", "any", "not"):
        args, j = [], i + 2
        while toks[j] != ")":
            arg, j = parse_cfg(toks, j)
            args.append(arg)
            if toks[j] == ",":
                j += 1
        return (name, args), j + 1
    if toks[i + 1] == "=":
        return ("kv", name, toks[i + 2]), i + 3
    return ("flag", name), i + 1


class Unknown(Exception):
    pass


TARGETS = {
    "linux": {"flags": {"unix": True, "windows": False},
              "values": {"target_os": '"linux"', "target_family": '"unix"', "target_env": '"gnu"'}},
    "windows": {"flags": {"unix": False, "windows": True},
                "values": {"target_os": '"windows"', "target_family": '"windows"', "target_env": '"msvc"'}},
}


def evaluate(tree, features: frozenset[str], target: str = "linux") -> bool:
    kind = tree[0]
    if kind == "all":
        return all(evaluate(t, features, target) for t in tree[1])
    if kind == "any":
        return any(evaluate(t, features, target) for t in tree[1])
    if kind == "not":
        return not evaluate(tree[1][0], features, target)
    if kind == "flag":
        flags = {"test": True, "debug_assertions": False, "miri": False, **TARGETS[target]["flags"]}
        if tree[1] not in flags:
            raise Unknown(tree)
        return flags[tree[1]]
    key, value = tree[1], tree[2]
    values = {"target_arch": '"x86_64"', "target_pointer_width": '"64"', "target_endian": '"little"',
              **TARGETS[target]["values"]}
    if key == "feature":
        return value.strip('"') in features
    if key not in values:
        raise Unknown(tree)
    return value == values[key]


class Module:
    def __init__(self, path: str, file: Path, cfgs: list):
        self.path, self.file, self.cfgs = path, file, cfgs


def attribute_groups(toks: list[str], i: int):
    """toks[i] == '#'; return (attribute tokens without #[ ], next index, inner)."""
    j = i + 1
    inner = toks[j] == "!"
    if inner:
        j += 1
    assert toks[j] == "[", (toks[i:i + 5])
    depth, k = 0, j
    while True:
        if toks[k] == "[":
            depth += 1
        elif toks[k] == "]":
            depth -= 1
            if depth == 0:
                return toks[j + 1:k], k + 1, inner
        k += 1


def scan_file(module_path: str, file: Path, outer_cfgs: list, tests: list, inline_path_ok=True):
    toks = tokens(file.read_text(encoding="utf-8"))
    is_dir_owner = file.name in ("mod.rs", "lib.rs")
    base_dir = file.parent if is_dir_owner else file.parent / file.stem
    # stack of (module path, cfgs, brace depth at open, child directory)
    stack = [(module_path, list(outer_cfgs), 0, base_dir)]
    depth, i, pending, file_cfgs = 0, 0, [], []
    while i < len(toks):
        t = toks[i]
        if t == "#" and i + 1 < len(toks) and toks[i + 1] in ("[", "!"):
            if toks[i + 1] == "!" and (i + 2 >= len(toks) or toks[i + 2] != "["):
                i += 1
                continue
            attr, i, inner = attribute_groups(toks, i)
            if attr[:2] == ["cfg", "("]:
                tree, _ = parse_cfg(attr, 2)
                (stack[-1][1].append(tree) if inner else pending.append(tree))
            elif attr and attr[0] == "test" and len(attr) == 1:
                pending.append("TEST")
            elif attr and attr[0] == "path":
                pending.append(("PATH", attr[2].strip('"')))
            continue
        if t == "{":
            depth += 1
            i += 1
            pending = []
            continue
        if t == "}":
            if len(stack) > 1 and stack[-1][2] == depth:
                stack.pop()
            depth -= 1
            i += 1
            pending = []
            continue
        if t == ";":
            pending = []
            i += 1
            continue
        if t == "mod" and i + 2 < len(toks) and re.fullmatch(r"[A-Za-z_]\w*", toks[i + 1]):
            name, after = toks[i + 1], toks[i + 2]
            cfgs = stack[-1][1] + [p for p in pending if isinstance(p, tuple) and p[0] != "PATH"]
            child_path = f"{stack[-1][0]}::{name}" if stack[-1][0] else name
            path_attr = next((p[1] for p in pending if isinstance(p, tuple) and p[0] == "PATH"), None)
            if after == ";":
                if path_attr:
                    if len(stack) > 1:
                        raise Unknown(f"path attribute inside an inline module in {file}")
                    child = file.parent / path_attr
                else:
                    d = stack[-1][3]
                    child = d / f"{name}.rs" if (d / f"{name}.rs").exists() else d / name / "mod.rs"
                if not child.exists():
                    raise Unknown(f"module file for {child_path} not found from {file}")
                scan_file(child_path, child, cfgs, tests)
                pending = []
                i += 3
                continue
            if after == "{":
                depth += 1
                stack.append((child_path, cfgs, depth, stack[-1][3] / name))
                pending = []
                i += 3
                continue
        if t == "fn" and i + 1 < len(toks) and "TEST" in pending:
            name = toks[i + 1]
            cfgs = stack[-1][1] + [p for p in pending if isinstance(p, tuple) and p[0] != "PATH"]
            full = f"{stack[-1][0]}::{name}" if stack[-1][0] else name
            tests.append((full, cfgs, file))
            pending = []
            i += 2
            continue
        i += 1


@lru_cache(maxsize=1)
def all_tests() -> tuple:
    """Every #[test] function of the library: (libtest name, cfg predicates, file)."""
    tests: list = []
    scan_file("", SRC / "lib.rs", [], tests)
    return tuple(tests)


def cargo_features() -> dict[str, list[str]]:
    cargo = (ROOT / "mtg-kernel" / "Cargo.toml").read_text(encoding="utf-8")
    section = cargo[cargo.index("[features]"):]
    section = section[:re.search(r"^\[(?!features)", section[1:], re.MULTILINE).start() + 1]
    return {name: re.findall(r'"([a-z0-9-]+)"', deps)
            for name, deps in re.findall(r'^([a-z0-9-]+)\s*=\s*\[([^\]]*)\]', section, re.MULTILINE)}


def closure(features: str) -> frozenset[str]:
    implies = cargo_features()
    out, todo = set(), [f for f in features.replace(" ", ",").split(",") if f] + implies.get("default", [])
    while todo:
        f = todo.pop()
        if f not in out:
            out.add(f)
            todo += implies.get(f, [])
    return frozenset(out)


def selected(name: str) -> bool:
    return any(f in name for f in ci_scope.TRAINER_FILTERS)


def predict(features: str, target: str = "linux") -> list[str]:
    enabled = closure(features)
    return sorted({name for name, cfgs, _ in all_tests()
                   if selected(name) and all(evaluate(c, enabled, target) for c in cfgs)})


def feature_only(features: str, target: str = "linux") -> list[str]:
    return sorted(set(predict(features, target)) - set(predict("", target)))


def uncovered() -> list[tuple[str, str, bool]]:
    """Selected tests no release-trainer row compiles: (name, file, whether some Windows build compiles it)."""
    rows = [(closure(fs), os) for os, fs in ci_scope.TRAINER_ROWS]
    windows = [closure(fs) for fs in ci_scope.FEATURE_SETS + [ci_scope.WINDOWS_STORE_FEATURES]]
    out = []
    for name, cfgs, file in all_tests():
        if selected(name) and not any(all(evaluate(c, row, os) for c in cfgs) for row, os in rows):
            on_windows = any(all(evaluate(c, row, "windows") for c in cfgs) for row in windows)
            out.append((name, file.relative_to(SRC).as_posix(), on_windows))
    return sorted(out)


def main(argv: list[str]) -> int:
    command = argv[1] if len(argv) > 1 else ""
    if command in ("predict", "feature-only"):
        print("\n".join((predict if command == "predict" else feature_only)(argv[2] if len(argv) > 2 else "")))
        return 0
    if command == "uncovered":
        print("\n".join(f"{name}  ({file})" for name, file in uncovered()))
        return 0
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
