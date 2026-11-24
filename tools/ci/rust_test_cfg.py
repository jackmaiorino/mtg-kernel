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

import ast
import re
import shlex
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


RAW = "\x00raw:"  # prefix of a raw string token, followed by its exact contents
PUSH, POP = "\x00push", "\x00pop"  # bounds of an included file's tokens


def string_value(tok: str) -> str:
    """The value of a string literal token (raw or ordinary, escapes decoded)."""
    if tok.startswith(RAW):
        return tok[len(RAW):]
    if tok.startswith('"'):
        return ast.literal_eval(tok)
    raise Unknown(f"not a string literal: {tok!r}")


def tokens(text: str) -> list[str]:
    out, i = [], 0
    while i < len(text):
        m = TOKEN.match(text, i)
        kind = m.lastgroup if m.lastgroup != "hashes" else "raw"
        if m.group("raw") is not None:
            end = text.index('"' + m.group("hashes"), m.end())
            out.append(RAW + text[m.end():end])
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


def expanded_tokens(file: Path) -> list[str]:
    """Tokens of a file with each `include!("path")` replaced by the included file's tokens (path relative to
    the including file) between PUSH and POP markers, so the scanner applies the attributes on the invocation
    to every included item, as the compiler does."""
    toks, out, i = tokens(file.read_text(encoding="utf-8")), [], 0
    while i < len(toks):
        if toks[i] == "include" and toks[i + 1:i + 3] == ["!", "("]:
            if toks[i + 3:i + 8] == ["concat", "!", "(", "env", "!"] and '"OUT_DIR"' in toks[i + 8:i + 11]:
                # Build-script output: not in the source tree. Generated tests would show up as an inventory
                # difference in run_trainer_row.py, never as a silent gap.
                depth, j = 0, i + 2
                while True:
                    depth += {"(": 1, ")": -1}.get(toks[j], 0)
                    j += 1
                    if depth == 0:
                        break
                i = j + (1 if j < len(toks) and toks[j] == ";" else 0)
                continue
            if toks[i + 4] != ")":
                raise Unknown(f"unsupported include! form in {file}")
            out += [PUSH, *expanded_tokens(file.parent / string_value(toks[i + 3])), POP]
            i += 5
            if i < len(toks) and toks[i] == ";":
                i += 1
            continue
        out.append(toks[i])
        i += 1
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
    toks = expanded_tokens(file)
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
        if t == PUSH:
            # An included file: its items inherit the cfgs on the include! invocation.
            stack.append((stack[-1][0], stack[-1][1] + [p for p in pending if isinstance(p, tuple)
                                                        and p[0] != "PATH"], None, stack[-1][3]))
            pending = []
            i += 1
            continue
        if t == POP:
            stack.pop()
            pending = []
            i += 1
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


WORKFLOW = ROOT / ".github" / "workflows" / "ci.yml"


def full_set_feature_steps(path: Path = WORKFLOW) -> list[tuple[str, list[str]]]:
    """(features, library filters) of every full-set test command that builds with features: each
    run_rust_lib_filters.py call and plain `cargo test --lib` call, excluding the trainer rows themselves and the
    Limited-only call (it runs every Limited-only case, which the Limited row's added tests include)."""
    text = re.sub(r"\\\n\s*", " ", path.read_text(encoding="utf-8"))
    steps = []
    for line in text.splitlines():
        if "--features" not in line or "--limited-only-workflow" in line:
            continue
        if "run_rust_lib_filters.py" in line:
            words = shlex.split(line.split("run_rust_lib_filters.py", 1)[1])
            valued = {"--features", "--shard-index", "--shard-count", "--filter-timeout-seconds"}
        elif "cargo test" in line and "--lib" in line.split():
            words = shlex.split(line.split("cargo test", 1)[1].split(" -- ", 1)[0])
            valued = {"--features", "--test", "--bin", "-p", "--package"}
        else:
            continue
        features = words[words.index("--features") + 1].replace(" ", ",")
        filters, i = [], 0
        while i < len(words):
            if words[i] in valued:
                i += 2
                continue
            if not words[i].startswith("-"):
                filters.append(words[i])
            i += 1
        steps.append((features, filters))
    return steps


def full_set_tests(features: str, target: str = "linux") -> list[str]:
    """Selected tests the full set's feature commands with exactly this feature set (same Cargo closure) run."""
    enabled = closure(features)
    filters = [f for step, fs in full_set_feature_steps() if closure(step) == enabled for f in fs]
    return sorted(name for name in predict(features, target) if any(f in name for f in filters))


def row_tests(features: str, target: str = "linux") -> list[str]:
    """What a feature row runs: the selected tests its build adds over the default build, plus the selected tests
    the full set's own commands with this feature set run, so the row is never weaker than the full set."""
    return sorted(set(feature_only(features, target)) | set(full_set_tests(features, target)))


def atoms(tree) -> set[str]:
    if tree[0] in ("all", "any", "not"):
        return set().union(*(atoms(t) for t in tree[1])) if tree[1] else set()
    return {tree[2].strip('"')} if tree[0] == "kv" and tree[1] == "feature" else set()


def satisfiable_on(cfgs: list, target: str) -> bool:
    """Whether some feature selection (with Cargo implications) compiles a test with these cfgs on target."""
    names = sorted(set().union(*(atoms(c) for c in cfgs))) if cfgs else []
    if len(names) > 12:
        raise Unknown(f"too many feature atoms to enumerate: {names}")
    for mask in range(1 << len(names)):
        enabled = closure(",".join(n for i, n in enumerate(names) if mask >> i & 1))
        if all(evaluate(c, enabled, target) for c in cfgs):
            return True
    return False


def uncovered() -> list[tuple[str, str, bool]]:
    """Selected tests no release-trainer row compiles: (name, file, whether no Linux build can compile it)."""
    rows = [(closure(fs), os) for os, fs in ci_scope.TRAINER_ROWS]
    out = []
    for name, cfgs, file in all_tests():
        if selected(name) and not any(all(evaluate(c, row, os) for c in cfgs) for row, os in rows):
            out.append((name, file.relative_to(SRC).as_posix(), not satisfiable_on(cfgs, "linux")))
    return sorted(out)


def check_coverage() -> int:
    """Exit status for the trainer scope: every selected test is run by some row unless no Linux build compiles it."""
    gaps = [(name, file) for name, file, windows_only in uncovered() if not windows_only]
    for name, file in gaps:
        print(f"no release-trainer row compiles {name} ({file})")
    if gaps:
        print("Add a feature set to tools/ci/ci_scope.py FEATURE_SETS (and the trainer-tests matrix) that compiles them.")
        return 1
    exempt = sum(1 for *_, windows_only in uncovered() if windows_only)
    print(f"coverage: every selected test is run by a row, except {exempt} that no Linux build compiles "
          f"(Windows with non-default features, outside the pull request set too)")
    return 0


def main(argv: list[str]) -> int:
    command = argv[1] if len(argv) > 1 else ""
    if command in ("predict", "feature-only"):
        target = argv[3] if len(argv) > 3 else "linux"
        if target not in TARGETS:
            print(f"target must be one of {sorted(TARGETS)}", file=sys.stderr)
            return 2
        function = predict if command == "predict" else feature_only
        print("\n".join(function(argv[2] if len(argv) > 2 else "", target)))
        return 0
    if command == "uncovered":
        for name, file, windows_only in uncovered():
            print(f"{name}  ({file})" + ("  Windows only" if windows_only else ""))
        return 0
    if command == "check-coverage":
        return check_coverage()
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
