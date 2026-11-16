"""Use memory the way a large compile does, for the memory guard's live test (never in a normal job).

`exhaust` holds more and more memory until it is killed. `thrash` holds a fixed amount, then keeps touching
random pages of it for a while and exits: with two of these holding more than the machine's memory but less
than memory plus swap, the machine pages constantly without ever running out.

The pages hold random bytes, so they cannot be shared or compressed away.
"""
from __future__ import annotations

import argparse
import os
import random
import sys
import time

CHUNK = 64 * 1024 * 1024
PAGE = 4096


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("scenario", choices=("exhaust", "thrash"))
    parser.add_argument("--rate-mib", type=float, default=150.0, help="MiB taken per second while growing")
    parser.add_argument("--hold-mib", type=float, default=13824.0, help="thrash: MiB to hold")
    parser.add_argument("--thrash-seconds", type=float, default=840.0, help="thrash: seconds of touching pages")
    args = parser.parse_args(argv)
    chunks: list[bytearray] = []
    started = time.monotonic()

    def held() -> float:
        return len(chunks) * CHUNK / 1048576

    while args.scenario == "exhaust" or held() < args.hold_mib:
        chunks.append(bytearray(os.urandom(CHUNK)))
        # Keep earlier memory in use, as a compiler does, so it cannot simply sit in swap.
        for _ in range(64):
            chunk = random.choice(chunks)
            chunk[random.randrange(0, CHUNK, PAGE)] ^= 1
        ahead = held() / args.rate_mib - (time.monotonic() - started)
        if ahead > 0:
            time.sleep(ahead)
        if len(chunks) % 16 == 0:
            print(f"hog {os.getpid()}: holding {held():.0f} MiB after {time.monotonic() - started:.0f} s", flush=True)
    print(f"hog {os.getpid()}: holding {held():.0f} MiB, touching random pages for {args.thrash_seconds:.0f} s",
          flush=True)
    until, touched, report = time.monotonic() + args.thrash_seconds, 0, time.monotonic() + 60
    while time.monotonic() < until:
        chunk = random.choice(chunks)
        chunk[random.randrange(0, CHUNK, PAGE)] ^= 1
        touched += 1
        if time.monotonic() >= report:
            print(f"hog {os.getpid()}: {touched} pages touched in the last minute", flush=True)
            touched, report = 0, time.monotonic() + 60
    print(f"hog {os.getpid()}: done", flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
