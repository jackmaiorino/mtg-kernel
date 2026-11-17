"""Experiment only, not for merge: starve a hosted Linux runner and keep its last readings.

`hog` takes memory at a fixed rate up to a size, then touches random pages of it for a while and exits.
With --fill third each page is one third random bytes and two thirds zeros, so it compresses about three
to one, as a compiler's heap roughly does; --fill random cannot be compressed.

`watch` samples memory, swap, pressure, swap and disk traffic and kernel messages every 2 seconds and
keeps a comment on the pull request up to date with the newest lines, more often when memory is short,
so that the readings outlive a runner that stops answering (its log and annotations do not). The job's
token is read from a file so that it never sits on a command line. Standard library only.
"""
from __future__ import annotations

import argparse
import ctypes
import http.client
import json
import os
import random
import re
import sys
import time

CHUNK = 64 * 1024 * 1024
PAGE = 4096


def hog(args) -> int:
    chunks: list[bytearray] = []
    started = time.monotonic()
    part = PAGE // 3

    def new_chunk() -> bytearray:
        if args.fill == "random":
            return bytearray(os.urandom(CHUNK))
        rnd = os.urandom((CHUNK // PAGE) * part)
        buf = bytearray(CHUNK)
        for i in range(CHUNK // PAGE):
            buf[i * PAGE:i * PAGE + part] = rnd[i * part:(i + 1) * part]
        return buf

    def held() -> float:
        return len(chunks) * CHUNK / 1048576

    while held() < args.hold_mib:
        chunks.append(new_chunk())
        for _ in range(64):  # keep earlier memory in use, as a compiler does
            chunk = random.choice(chunks)
            chunk[random.randrange(0, CHUNK, PAGE)] ^= 1
        ahead = held() / args.rate_mib - (time.monotonic() - started)
        if ahead > 0:
            time.sleep(ahead)
        if len(chunks) % 32 == 0:
            print(f"hog {os.getpid()}: holding {held():.0f} MiB after {time.monotonic() - started:.0f} s", flush=True)
    print(f"hog {os.getpid()}: holding {held():.0f} MiB, touching random pages for {args.thrash_seconds:.0f} s", flush=True)
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


def read(path: str) -> str:
    try:
        with open(path, encoding="utf-8", errors="replace") as handle:
            return handle.read()
    except OSError:
        return ""


def meminfo() -> dict[str, int]:
    out = {}
    for line in read("/proc/meminfo").splitlines():
        name, _, rest = line.partition(":")
        value = rest.split()
        if value and value[0].isdigit():
            out[name] = int(value[0]) // 1024
    return out


def pressure(kind: str) -> tuple[float, float]:
    some = full = 0.0
    for line in read(f"/proc/pressure/{kind}").splitlines():
        words = line.split()
        if len(words) > 1 and words[1].startswith("avg10="):
            if words[0] == "some":
                some = float(words[1][6:])
            elif words[0] == "full":
                full = float(words[1][6:])
    return some, full


def vmstat() -> dict[str, int]:
    out = {}
    for line in read("/proc/vmstat").splitlines():
        name, _, value = line.partition(" ")
        if name in ("pswpin", "pswpout", "pgmajfault", "pgscan_direct", "pgscan_kswapd", "allocstall_normal", "allocstall_movable", "oom_kill"):
            out[name] = int(value)
    return out


WHOLE_DISK = re.compile(r"^(sd[a-z]+|nvme[0-9]+n[0-9]+|xvd[a-z]+|vd[a-z]+)$")


def disk() -> tuple[int, int, int, int]:
    """(reads, writes, in flight, ms doing I/O) summed over the whole disks (sda on Azure, nvme0n1 on AWS)."""
    total = [0, 0, 0, 0]
    for line in read("/proc/diskstats").splitlines():
        words = line.split()
        if len(words) > 13 and WHOLE_DISK.match(words[2]):
            for i, col in enumerate((3, 7, 11, 12)):
                total[i] += int(words[col])
    return total[0], total[1], total[2], total[3]


def swaps() -> str:
    parts = []
    for line in read("/proc/swaps").splitlines()[1:]:
        words = line.split()
        if len(words) >= 4:
            parts.append(f"{os.path.basename(words[0])}:{int(words[3]) // 1024}")
    return ",".join(parts)


def zram() -> str:
    words = read("/sys/block/zram0/mm_stat").split()
    if len(words) < 3:
        return ""
    return f" zram orig={int(words[0]) >> 20} compr={int(words[1]) >> 20} used={int(words[2]) >> 20}"


def top() -> str:
    rows = []
    for entry in os.listdir("/proc"):
        if not entry.isdigit():
            continue
        name, rss = "", 0
        for line in read(f"/proc/{entry}/status").splitlines():
            if line.startswith("Name:"):
                name = line.split()[1] if len(line.split()) > 1 else "?"
            elif line.startswith("VmRSS:"):
                rss = int(line.split()[1]) // 1024
        if rss:
            rows.append((rss, name))
    rows.sort(reverse=True)
    return ",".join(f"{name}:{rss}" for rss, name in rows[:3])


class Comment:
    """One pull request comment, created once and then edited in place."""

    def __init__(self, token_file: str, repo: str, pr: str) -> None:
        self.repo = repo
        self.pr = pr
        self.token = read(token_file).strip() if token_file else ""
        self.id = None
        self.conn = None
        self.sent = 0
        self.errors = 0

    def call(self, method: str, path: str, body: dict) -> dict | None:
        data = json.dumps(body)
        headers = {"Authorization": "Bearer " + self.token, "Accept": "application/vnd.github+json", "User-Agent": "starve-probe",
                   "Content-Type": "application/json", "X-GitHub-Api-Version": "2022-11-28"}
        for _ in range(2):
            try:
                if self.conn is None:
                    self.conn = http.client.HTTPSConnection("api.github.com", timeout=8)
                self.conn.request(method, path, body=data, headers=headers)
                response = self.conn.getresponse()
                text = response.read()
                if response.status >= 300:
                    self.errors += 1
                    return {"status": response.status, "text": text[:200].decode("utf-8", "replace")}
                return json.loads(text)
            except (OSError, http.client.HTTPException, ValueError):
                self.errors += 1
                try:
                    self.conn.close()
                except Exception:  # noqa: BLE001
                    pass
                self.conn = None
        return None

    def put(self, body: str) -> None:
        if not (self.repo and self.pr and self.token):
            return
        self.sent += 1
        if self.id is None:
            answer = self.call("POST", f"/repos/{self.repo}/issues/{self.pr}/comments", {"body": body})
            if answer and "id" in answer:
                self.id = answer["id"]
            elif answer:
                print("comment not created:", answer, flush=True)
        else:
            self.call("PATCH", f"/repos/{self.repo}/issues/comments/{self.id}", {"body": body})


def watch(args) -> int:
    try:
        ctypes.CDLL(None, use_errno=True).mlockall(3)
    except (OSError, AttributeError):
        pass
    try:
        with open("/proc/self/oom_score_adj", "w") as handle:
            handle.write("-1000")
    except OSError:
        pass
    try:
        os.sched_setscheduler(0, os.SCHED_FIFO, os.sched_param(10))
    except (OSError, AttributeError):
        pass
    kmsg = None
    try:
        kmsg = os.open("/dev/kmsg", os.O_RDONLY | os.O_NONBLOCK)
        os.lseek(kmsg, 0, os.SEEK_END)
    except OSError:
        kmsg = None
    comment = Comment(args.token_file, args.repo, args.pr)
    run = args.run
    head = (f"starve probe `{args.label}` run {run} on {os.uname().nodename}, started {time.strftime('%H:%M:%S', time.gmtime())} UTC. "
            f"Machine: {args.note}. "
            "Columns: seconds, MemAvailable and Dirty+Writeback in MiB, swap used per device in MiB, memory and io pressure "
            "(some/full avg10), per second: pages swapped out and in, major faults, direct-reclaim stalls; sda writes and reads per "
            "second, requests in flight, percent busy (all whole disks); largest processes (MiB).")
    lines: list[str] = []
    log = open(args.log, "a", encoding="utf-8")
    started = time.monotonic()
    last_put, last_v, last_d, last_t = 0.0, vmstat(), disk(), started
    while True:
        now = time.monotonic()
        mem = meminfo()
        v, d = vmstat(), disk()
        dt = max(now - last_t, 0.001)
        rate = {k: (v.get(k, 0) - last_v.get(k, 0)) / dt for k in v}
        stalls = rate.get("allocstall_normal", 0) + rate.get("allocstall_movable", 0)
        busy = 100.0 * (d[3] - last_d[3]) / (dt * 1000.0)
        ms, mf = pressure("memory")
        ios, iof = pressure("io")
        line = (f"{now - started:6.0f}s avail={mem.get('MemAvailable', 0):5d} dirty={mem.get('Dirty', 0) + mem.get('Writeback', 0):4d} "
                f"swap[{swaps()}] mem={ms:.0f}/{mf:.0f} io={ios:.0f}/{iof:.0f} out={rate.get('pswpout', 0):.0f} in={rate.get('pswpin', 0):.0f} "
                f"maj={rate.get('pgmajfault', 0):.0f} stall={stalls:.0f} disk w={(d[1] - last_d[1]) / dt:.0f} r={(d[0] - last_d[0]) / dt:.0f} "
                f"q={d[2]} busy={busy:.0f}%{zram()} oom={v.get('oom_kill', 0)} top={top()}")
        if kmsg is not None:
            try:
                while True:
                    record = os.read(kmsg, 4096).decode("utf-8", "replace")
                    text = record.split(";", 1)[-1].strip().splitlines()[0][:150]
                    lines.append("   kernel: " + text)
                    log.write("   kernel: " + text + "\n")
            except OSError:
                pass
        lines.append(line)
        log.write(line + "\n")
        log.flush()
        last_v, last_d, last_t = v, d, now
        short = mem.get("MemAvailable", 99999) < 1500 or mf > 5 or stalls > 0
        every = args.short_every if short else args.calm_every
        if now - last_put >= every and comment.sent < args.max_puts:
            body = head + "\n\n```\n" + "\n".join(lines[-args.keep:]) + "\n```\n" + f"(update {comment.sent + 1}, api errors {comment.errors})"
            comment.put(body)
            last_put = time.monotonic()
        time.sleep(2.0)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    h = commands.add_parser("hog")
    h.add_argument("--fill", choices=("random", "third"), default="third")
    h.add_argument("--rate-mib", type=float, default=150.0)
    h.add_argument("--hold-mib", type=float, default=12288.0)
    h.add_argument("--thrash-seconds", type=float, default=240.0)
    w = commands.add_parser("watch")
    w.add_argument("--label", default="probe")
    w.add_argument("--log", default="/tmp/starve-watch.log")
    w.add_argument("--calm-every", type=float, default=30.0)
    w.add_argument("--short-every", type=float, default=6.0)
    w.add_argument("--max-puts", type=int, default=80)
    w.add_argument("--keep", type=int, default=45)
    w.add_argument("--token-file", default="")
    w.add_argument("--repo", default="")
    w.add_argument("--pr", default="")
    w.add_argument("--run", default="?")
    w.add_argument("--note", default="")
    args = parser.parse_args(argv)
    return hog(args) if args.command == "hog" else watch(args)


if __name__ == "__main__":
    sys.exit(main())
