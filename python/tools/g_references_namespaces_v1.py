"""The G(i) seed namespaces against every registered one (collab LANES/g-references-20261028/DESIGN.md, section 3).

Read from code only, the generators and the panel registry (tools/spy_reset/pilot.py); no CP7 or sealed
outcome file is opened. Kept apart from g_references_v1.py, whose import closure travels to rented pods.

CLI: python g_references_namespaces_v1.py   (JSON on stdout; exit 1 on a clash)
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

import g_references_v1 as g
import nine_deck_baseline_v1 as ndb


def registered_namespaces() -> list[str]:
    """Namespace strings of earlier generators and every registered panel namespace."""
    tools = Path(__file__).resolve().parents[2] / "tools" / "spy_reset"
    sys.path.insert(0, str(tools))
    try:
        import pilot
    finally:
        sys.path.remove(str(tools))
    names = {f"{ndb.PREFIX}/{name}" for name in ("order", "train", "dev", "confirm")}
    names |= {name for name in pilot.PANEL_NAMESPACES if not name.startswith(g.PREFIX + "/")}
    names |= {pilot.NS, f"{pilot.NS}/train", f"{pilot.NS}/qualification"}
    return sorted(names)


def namespace_check() -> dict:
    """This study's namespaces are new strings: no registered namespace shares this study's prefix.

    Read from code only (the generators and the panel registry); no CP7 or
    sealed outcome file is opened. Every key is compact JSON whose first
    element is the namespace, so distinct namespaces give distinct keys.
    """
    registered = registered_namespaces()
    clashes = [name for name in registered if name.split("/")[0] == g.PREFIX]
    return {"schema": f"{g.PREFIX}/namespace-check", "namespaces": list(g.NAMESPACES), "registered": registered,
            "clashes": clashes, "pass": not clashes and len(set(g.NAMESPACES)) == len(g.NAMESPACES)}


if __name__ == "__main__":
    outcome = namespace_check()
    print(json.dumps(outcome))
    raise SystemExit(0 if outcome["pass"] else 1)
