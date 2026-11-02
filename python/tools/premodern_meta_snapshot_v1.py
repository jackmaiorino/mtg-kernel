"""Capture a Premodern metagame snapshot from mtgtop8.com for the eternal census.

Network tooling, run by hand: the census and the catalog tests read the
committed output and never fetch. For each archetype mtgtop8 lists for the
chosen period, the snapshot keeps its metagame share (mtgtop8 rounds it to a
whole percent and omits archetypes under 1%) and the most recent deck it links,
in the `eternal_meta_snapshot/v1` schema of the mtggoldfish snapshot.
"""

from __future__ import annotations

import argparse
import html
import json
from pathlib import Path
import re
import time
import urllib.request


BASE = "https://www.mtgtop8.com/"
ARCHETYPE = re.compile(
    r"archetype\?a=(\d+)&meta=\d+&f=PREM>([^<]+)</a></div>\s*<div>\s*<div[^>]*>(\d+) %")
DECK_LINK = re.compile(r"event\?e=(\d+)&d=(\d+)&f=PREM")
DECK_ROW = re.compile(
    r'id=(md|sb)\w+ class="deck_line[^"]*"[^>]*>(\d+) <span class=L14>([^<]+)</span>')


def fetch(path: str, delay: float) -> str:
    time.sleep(delay)
    request = urllib.request.Request(BASE + path, headers={"User-Agent": "Mozilla/5.0"})
    with urllib.request.urlopen(request, timeout=30) as response:
        return response.read().decode("utf-8", "replace")


def card_name(raw: str) -> str:
    """mtgtop8 writes split cards as "Fire / Ice"; the registries use "Fire // Ice"."""
    return html.unescape(raw).strip().replace(" / ", " // ")


def slug(name: str) -> str:
    return "premodern-" + re.sub(r"[^a-z0-9]+", "-", name.lower()).strip("-")


def capture(meta: str, delay: float) -> dict:
    page = fetch(f"format?f=PREM&meta={meta}", delay)
    decks = re.search(r"(\d+) decks</div>", page).group(1)
    archetypes = []
    for archetype_id, name, share in ARCHETYPE.findall(page):
        link = DECK_LINK.search(fetch(f"archetype?a={archetype_id}&meta={meta}&f=PREM", delay))
        if link is None:
            raise ValueError(f"no deck listed for {name!r}")
        event, deck = link.groups()
        zones = {"md": [], "sb": []}
        for zone, count, card in DECK_ROW.findall(fetch(f"event?e={event}&d={deck}&f=PREM", delay)):
            zones[zone].append([int(count), card_name(card)])
        archetypes.append({
            "archetype": slug(html.unescape(name)),
            "share": float(share),
            "mainboard": zones["md"],
            "sideboard": zones["sb"],
            "companion": None,
            "source_deck": f"{BASE}event?e={event}&d={deck}&f=PREM",
        })
    return {
        "schema": "eternal_meta_snapshot/v1",
        "source": f"mtgtop8.com Premodern metagame meta={meta} ({decks} decks), "
                  "most recent listed deck per archetype",
        "captured": time.strftime("%Y-%m-%d"),
        "formats": {"premodern": archetypes},
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("out", type=Path)
    parser.add_argument("--meta", default="261", help="mtgtop8 period id; 261 is the last two months")
    parser.add_argument("--delay", type=float, default=1.0, help="seconds between requests")
    args = parser.parse_args()
    snapshot = capture(args.meta, args.delay)
    args.out.write_text(json.dumps(snapshot, indent=1, ensure_ascii=False) + "\n", encoding="utf-8",
                        newline="\n")


if __name__ == "__main__":
    main()
