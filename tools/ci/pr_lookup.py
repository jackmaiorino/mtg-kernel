"""The two GitHub REST lookups of ci.yml's "Choose the group set" step, for runners without the GitHub CLI.

open-pr-head: "<number> <head sha> <base sha>" of the first open, ready pull request for this
repository's branch GITHUB_REF_NAME, or an empty line. ready-pr selects the same tuple from JSON on stdin.
pr-files N: the changed-file records of pull request N, as an array of pages (the gh --paginate --slurp output).

Reads GH_TOKEN, GITHUB_API_URL, GITHUB_REPOSITORY, GITHUB_REPOSITORY_OWNER and GITHUB_REF_NAME. Standard
library only. Any failure exits non-zero, and the step then keeps the full group set.
"""
from __future__ import annotations

import json
import os
import re
import sys
import urllib.parse
import urllib.request


def get(path: str):
    request = urllib.request.Request(f"{os.environ.get('GITHUB_API_URL', 'https://api.github.com')}/{path}")
    request.add_header("Accept", "application/vnd.github+json")
    request.add_header("Authorization", f"Bearer {os.environ['GH_TOKEN']}")
    with urllib.request.urlopen(request, timeout=30) as response:
        return json.load(response)


def open_pr_head(pulls: list, repository: str, ref: str) -> str:
    for pull in pulls:
        head = pull["head"]
        if (pull["state"] != "open" or pull["draft"] is not False or head["ref"] != ref
                or (head["repo"] or {}).get("full_name") != repository):
            continue
        number, head_sha, base_sha = pull["number"], head["sha"], pull["base"]["sha"]
        if (type(number) is not int or number < 1
                or not all(isinstance(sha, str) and re.fullmatch(r"[0-9a-f]{40}", sha)
                           for sha in (head_sha, base_sha))):
            raise ValueError("Invalid ready pull request number or SHA")
        return f"{number} {head_sha} {base_sha}"
    return ""


def main(argv: list) -> int:
    repository = os.environ["GITHUB_REPOSITORY"]
    if argv in (["open-pr-head"], ["ready-pr"]):
        ref = os.environ["GITHUB_REF_NAME"]
        if argv[0] == "open-pr-head":
            head = urllib.parse.quote(f"{os.environ['GITHUB_REPOSITORY_OWNER']}:{ref}", safe=":/")
            pulls = get(f"repos/{repository}/pulls?state=open&head={head}")
        else:
            pulls = json.load(sys.stdin)
        print(open_pr_head(pulls, repository, ref))
        return 0
    if argv[:1] == ["pr-files"] and len(argv) == 2 and argv[1].isdigit():
        pages = []
        for page in range(1, 31):  # GitHub lists at most 3000 files of a pull request
            files = get(f"repos/{repository}/pulls/{argv[1]}/files?per_page=100&page={page}")
            if not isinstance(files, list):
                print("Expected an array from the pull request files API", file=sys.stderr)
                return 2
            pages.append(files)
            if len(files) < 100:
                break
        print(json.dumps(pages))
        return 0
    print("usage: pr_lookup.py open-pr-head | ready-pr | pr-files <number>", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
