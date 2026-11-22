"""The two GitHub REST lookups of ci.yml's "Choose the group set" step, for runners without the GitHub CLI.

open-pr-head: "<number> <head sha>" of the first open, ready (not draft) pull request whose head is this
repository's branch GITHUB_REF_NAME, or " " when there is none (the gh --jq output).
pr-files N: every file name of pull request N, and each renamed file's previous name, one per line.

Reads GH_TOKEN, GITHUB_API_URL, GITHUB_REPOSITORY, GITHUB_REPOSITORY_OWNER and GITHUB_REF_NAME. Standard
library only. Any failure exits non-zero, and the step then keeps the full group set.
"""
from __future__ import annotations

import json
import os
import sys
import urllib.parse
import urllib.request


def get(path: str):
    request = urllib.request.Request(f"{os.environ.get('GITHUB_API_URL', 'https://api.github.com')}/{path}")
    request.add_header("Accept", "application/vnd.github+json")
    request.add_header("Authorization", f"Bearer {os.environ['GH_TOKEN']}")
    with urllib.request.urlopen(request, timeout=30) as response:
        return json.load(response)


def open_pr_head(pulls: list, repository: str) -> str:
    ready = [pull for pull in pulls if pull["head"]["repo"] and pull["head"]["repo"]["full_name"] == repository
             and not pull["draft"]]
    return f"{ready[0]['number']} {ready[0]['head']['sha']}" if ready else " "


def file_names(files: list) -> list:
    return [name for entry in files for name in (entry["filename"], entry.get("previous_filename")) if name]


def main(argv: list) -> int:
    repository = os.environ["GITHUB_REPOSITORY"]
    if argv[:1] == ["open-pr-head"] and len(argv) == 1:
        head = urllib.parse.quote(f"{os.environ['GITHUB_REPOSITORY_OWNER']}:{os.environ['GITHUB_REF_NAME']}", safe=":/")
        print(open_pr_head(get(f"repos/{repository}/pulls?state=open&head={head}"), repository))
        return 0
    if argv[:1] == ["pr-files"] and len(argv) == 2 and argv[1].isdigit():
        for page in range(1, 31):  # GitHub lists at most 3000 files of a pull request
            files = get(f"repos/{repository}/pulls/{argv[1]}/files?per_page=100&page={page}")
            for name in file_names(files):
                print(name)
            if len(files) < 100:
                return 0
        return 0
    print("usage: pr_lookup.py open-pr-head | pr-files <number>", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
