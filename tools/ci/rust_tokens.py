"""A small Rust tokenizer (comments, strings and raw strings handled), for the trainer reference guard."""
import re

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
