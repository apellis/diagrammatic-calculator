#!/usr/bin/env python3
"""Check that the production site serves the deployment just made.

Reads https://diagcalc.pages.dev/build-info.json back and requires its commit to be the
deployed one, and reads /privacy back and requires its title and paragraphs to be the
text of PRIVACY.md at that commit. Retries for propagation; a lasting mismatch exits
nonzero. Needs no credentials: it reads only what any visitor sees.
"""
import argparse
from html.parser import HTMLParser
import json
import os
from pathlib import Path
import re
import sys
import time
from urllib.request import Request, urlopen

SITE = "https://diagcalc.pages.dev"


def collapse(text):
    return re.sub(r"\s+", " ", text).strip()


def policy_blocks(markdown):
    """The title and paragraphs of PRIVACY.md, as the site's /privacy page renders them."""
    blocks = [collapse(b) for b in re.split(r"\n\s*\n", markdown.strip())]
    if blocks and blocks[0].startswith("# "):
        blocks[0] = blocks[0][2:]
    return blocks


class _Blocks(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.blocks, self.depth, self.text = [], 0, []

    def handle_starttag(self, tag, attrs):
        if tag in ("h1", "p"):
            if self.depth == 0:
                self.text = []
            self.depth += 1

    def handle_endtag(self, tag):
        if tag in ("h1", "p") and self.depth:
            self.depth -= 1
            if self.depth == 0:
                self.blocks.append(collapse("".join(self.text)))

    def handle_data(self, data):
        if self.depth:
            self.text.append(data)


def served_blocks(page):
    """The text of each heading and paragraph of an HTML page, in order."""
    parser = _Blocks()
    parser.feed(page)
    parser.close()
    return parser.blocks


def fetch(url):
    request = Request(url, headers={"Cache-Control": "no-cache", "Pragma": "no-cache",
                                    "User-Agent": "diagcalc-deploy-check"})
    with urlopen(request, timeout=30) as response:
        return response.read().decode("utf-8")


def problems(site, commit, policy, get=fetch):
    """What the site serves differently from the deployment; empty when it matches."""
    found = []
    try:
        info = json.loads(get(f"{site}/build-info.json?check={commit}"))
        served = info.get("commit") if isinstance(info, dict) else None
        if served != commit:
            found.append(f"build-info.json names commit {served!r}, deployed {commit!r}")
    except Exception as error:  # Network, HTTP, or JSON errors: all mean "not yet served".
        found.append(f"build-info.json unreadable: {error}")
    try:
        page = served_blocks(get(f"{site}/privacy?check={commit}"))
        if page[:len(policy)] != policy:
            mismatch = next((i for i, (a, b) in enumerate(zip(page, policy)) if a != b),
                            min(len(page), len(policy)))
            found.append(f"/privacy differs from PRIVACY.md at block {mismatch + 1}")
    except Exception as error:
        found.append(f"/privacy unreadable: {error}")
    return found


def check(site, commit, policy, *, attempts, delay, get=fetch, sleep=time.sleep):
    for attempt in range(1, attempts + 1):
        found = problems(site, commit, policy, get)
        if not found:
            return []
        print(f"attempt {attempt}/{attempts}: " + "; ".join(found), flush=True)
        if attempt < attempts:
            sleep(delay)
    return found


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--commit", required=True, help="the full hash of the deployed commit")
    parser.add_argument("--privacy", type=Path, default=Path("PRIVACY.md"))
    parser.add_argument("--site", default=SITE)
    parser.add_argument("--attempts", type=int, default=12)
    parser.add_argument("--delay", type=float, default=15)
    args = parser.parse_args()
    if not re.fullmatch(r"[0-9a-f]{40}", args.commit):
        parser.error("--commit must be a full 40-digit commit hash")
    policy = policy_blocks(args.privacy.read_text(encoding="utf-8"))
    found = check(args.site, args.commit, policy, attempts=args.attempts, delay=args.delay)
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if found:
        for problem in found:
            print(f"::error::{args.site}: {problem}")
    elif summary:
        with open(summary, "a", encoding="utf-8") as output:
            output.write(f"{args.site} serves commit `{args.commit[:7]}` and the current "
                         f"privacy policy ({len(policy)} blocks).\n")
    print("site matches the deployment" if not found else "site does not match the deployment")
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main())
