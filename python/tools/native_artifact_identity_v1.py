"""Exact portable identities for a pinned native artifact dependency graph.

Read-only successor to comparisons that omit trajectory hashes. Raw pins remain
authoritative for storage. This identity replaces each verified dependency pin
with its recursively normalized content identity, retaining every other field.
It never edits files or changes the frozen G(i) qualification judgement.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
from pathlib import Path
import re

import checkpoint_container_v1 as checkpoint


RULE = "mtg-kernel-native-artifact-content/v1"
CAP = 1024 ** 3
HEX = re.compile(r"[0-9a-f]{64}\Z")
ATTEMPT = re.compile(r"(b[0-9]+)-a[0-9]+\Z")
REFERENCES = {"checkpoint_sha256", "play_import_sha256", "trajectory_sha256"}


def unique(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON member: {key}")
        result[key] = value
    return result


def nonfinite(value):
    raise ValueError(f"non-finite JSON number: {value}")


def canonical(value):
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"),
                      allow_nan=False).encode("utf-8")


def digest(value):
    return hashlib.sha256(RULE.encode("ascii") + b"\0" + canonical(value)).hexdigest()


def check_pin(pin):
    if (not isinstance(pin, dict) or set(pin) != {"path", "sha256"}
            or not isinstance(pin["path"], str) or not pin["path"]
            or not isinstance(pin["sha256"], str) or not HEX.fullmatch(pin["sha256"])):
        raise ValueError("expected an exact {path, sha256} pin")


class Identity:
    """One immutable read session. Supply retained pins for hash-only references.

    Roots map stable logical names to absolute physical directories. Only those
    prefixes and bNNN-aK components beneath them are normalized. Roots outside
    the mapping keep their absolute spelling, preventing accidental equivalence.
    """
    def __init__(self, roots: dict[str, str], dependencies=(), *, max_bytes=CAP):
        if type(max_bytes) is not int or max_bytes <= 0:
            raise ValueError("byte cap must be a positive integer")
        self.roots = []
        for name, root in roots.items():
            if not re.fullmatch(r"[A-Za-z][A-Za-z0-9_-]*", name):
                raise ValueError("root labels must be simple identifiers")
            path = Path(root)
            if not path.is_absolute():
                raise ValueError("root directories must be absolute")
            self.roots.append((str(path.resolve()).replace("\\", "/").rstrip("/"), name))
        if len({root for root, _ in self.roots}) != len(self.roots):
            raise ValueError("one physical root cannot have two labels")
        self.roots.sort(key=lambda row: len(row[0]), reverse=True)
        self.catalog = {}
        for pin in dependencies:
            self.register(pin)
        self.max_bytes = max_bytes
        self.cache = {}
        self.active = set()
        self.verified = {}

    def register(self, pin):
        check_pin(pin)
        # A raw hash can have copies at several paths. Each referenced copy is
        # still checked on access; hash-only references use the declared copy.
        self.catalog.setdefault(pin["sha256"], pin)

    def portable_path(self, path, *, raw_sha256=None, content_sha256=None):
        if not isinstance(path, str):
            raise ValueError("path must be a string")
        candidate = path.replace("\\", "/")
        for root, name in self.roots:
            if candidate == root or candidate.startswith(root + "/"):
                relative = candidate[len(root):].lstrip("/")
                if any(part in (".", "..", "") for part in relative.split("/") if relative):
                    raise ValueError("non-canonical path under a declared root")
                parts = relative.split("/") if relative else []
                normalized = []
                for index, part in enumerate(parts):
                    attempt = ATTEMPT.fullmatch(part)
                    if attempt:
                        normalized.append({"block": attempt[1], "attempt": "normalized"})
                    elif (raw_sha256 is not None and part == raw_sha256 and index >= 2
                          and parts[index - 2:index] == ["inputs", "objects"]):
                        # The cloud stager names the descriptor's directory by
                        # its raw hash, which changes when its root changes.
                        # Only this verified pin's exact content address may be
                        # replaced, never arbitrary hash-looking path segments.
                        normalized.append({"content_sha256": content_sha256})
                    else:
                        normalized.append(part)
                return {"root": name, "components": normalized}
        return {"absolute_or_external": candidate}

    @staticmethod
    def is_pin(value):
        return isinstance(value, dict) and set(value) == {"path", "sha256"}

    def discover(self, value):
        if self.is_pin(value):
            self.register(value)
        elif isinstance(value, dict):
            for member in value.values():
                self.discover(member)
        elif isinstance(value, list):
            for member in value:
                self.discover(member)

    def normalize(self, value, key=""):
        if self.is_pin(value):
            content = self.artifact(value)["content_sha256"]
            path = self.portable_path(value["path"], raw_sha256=value["sha256"], content_sha256=content)
            return ["pin", path, content]
        if isinstance(value, dict):
            return ["object", [[k, self.normalize(v, k)] for k, v in sorted(value.items())]]
        if isinstance(value, list):
            return ["array", [self.normalize(v, key) for v in value]]
        if isinstance(value, str):
            if key == "path" or key.endswith(("_path", "_directory")):
                return ["path", self.portable_path(value)]
            if key in REFERENCES or key.endswith("_checkpoint_sha256"):
                if value not in self.catalog:
                    raise ValueError(f"missing retained dependency for {key}: {value}")
                return ["dependency", self.artifact(self.catalog[value])["content_sha256"]]
            return ["string", value]
        # Type tags prevent an ordinary JSON object/array from imitating a
        # normalized path or dependency. Preserve floating-point signed zero.
        if value is None:
            return ["null"]
        if isinstance(value, bool):
            return ["boolean", value]
        if isinstance(value, int):
            return ["integer", value]
        if isinstance(value, float):
            if not math.isfinite(value):
                raise ValueError("non-finite JSON number")
            return ["float", value.hex()]
        raise ValueError("unsupported JSON value")

    def artifact(self, pin):
        check_pin(pin)
        path = Path(pin["path"])
        if not path.is_absolute():
            raise ValueError("artifact pins must use absolute paths")
        key = (str(path), pin["sha256"])
        if key in self.cache:
            return self.cache[key]
        if key in self.active:
            raise ValueError("cyclic artifact dependency")
        if path.stat().st_size > self.max_bytes:
            raise ValueError("artifact exceeds byte cap")
        with path.open("rb") as stream:
            data = stream.read(self.max_bytes + 1)
        if len(data) > self.max_bytes:
            raise ValueError("artifact exceeds byte cap")
        if hashlib.sha256(data).hexdigest() != pin["sha256"]:
            raise ValueError(f"artifact pin mismatch: {path}")
        self.register(pin)
        self.active.add(key)
        try:
            if data.startswith(b"\x28\xb5\x2f\xfd"):
                raise ValueError("compressed record envelope is not supported by identity v1")
            if checkpoint.is_container(data) or data.lstrip().startswith((b"{", b"[")):
                document = checkpoint.document_from_bytes(
                    data, object_pairs_hook=unique, parse_constant=nonfinite)
                self.discover(document)
                content = {"format": "native-json-document", "document": self.normalize(document)}
            else:
                # Opaque leaves retain every byte. A malformed JSON-looking
                # record never falls back to this case.
                content = {"format": "opaque-bytes", "sha256": pin["sha256"]}
            result = {"rule": RULE, "file_sha256": pin["sha256"],
                      "content_sha256": digest(content)}
            self.cache[key] = result
            self.verified[key] = {"path": str(path), **result}
            return result
        finally:
            self.active.remove(key)

    def report(self, pin):
        result = self.artifact(pin)
        return {**result, "verified_artifacts": sorted(self.verified.values(), key=lambda x: x["path"])}


def read_manifest(path):
    value = json.loads(Path(path).read_bytes(), object_pairs_hook=unique, parse_constant=nonfinite)
    if (not isinstance(value, dict) or set(value) != {"schema", "roots", "artifact", "dependencies"}
            or value["schema"] != "mtg-kernel-native-artifact-identity-request/v1"
            or not isinstance(value["roots"], dict) or not isinstance(value["dependencies"], list)):
        raise ValueError("invalid native artifact identity request")
    return Identity(value["roots"], value["dependencies"]).report(value["artifact"])


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("request", help="JSON request naming roots, artifact pin and retained dependency pins")
    args = parser.parse_args(argv)
    print(json.dumps(read_manifest(args.request), indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
