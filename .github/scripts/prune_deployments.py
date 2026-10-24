#!/usr/bin/env python3
"""Prune obsolete diagcalc Pages deployments; dry run unless --delete is given.

Only the fixed project's deployments strictly older than production at selection
time are candidates. Read all pages before deleting, stop on a detected production
change, and read the list back afterward. This is not atomic with dashboard rollback:
cancel pruning and confirm it has stopped before rolling back. Cloudflare protects
the active production deployment. Credentials are supplied only through the environment.
"""
import argparse
from datetime import datetime
import json
import os
import re
import sys
import time
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen

# Transient failures of a read are retried; a write (DELETE) is never repeated blindly.
READ_ATTEMPTS = 3
# Cloudflare's listing can lag a delete by a few seconds: read it back this many times.
READBACK_ATTEMPTS = 6
READBACK_DELAY = 10
sleep = time.sleep


class Cloudflare:
    def __init__(self, account, token):
        if not re.fullmatch(r"[a-fA-F0-9]{32}", account):
            raise ValueError("CLOUDFLARE_ACCOUNT_ID must be a 32-digit hexadecimal ID")
        if not token:
            raise ValueError("CLOUDFLARE_API_TOKEN is required")
        self.url = f"https://api.cloudflare.com/client/v4/accounts/{account}/pages/projects/diagcalc"
        self.token = token

    def request(self, method, path=""):
        attempts = READ_ATTEMPTS if method == "GET" else 1
        for attempt in range(1, attempts + 1):
            try:
                return self._request(method, path)
            except (HTTPError, URLError, TimeoutError) as error:
                transient = not isinstance(error, HTTPError) or error.code == 429 or error.code >= 500
                if not transient or attempt == attempts:
                    raise RuntimeError(f"Cloudflare {method} {path or '/project'} failed: "
                                       f"{describe(error)}") from error
                sleep(5 * attempt)

    def _request(self, method, path):
        request = Request(self.url + path, method=method,
                          headers={"Authorization": f"Bearer {self.token}"})
        with urlopen(request, timeout=30) as response:
            body = json.load(response)
        if body.get("success") is not True or "result" not in body:
            raise RuntimeError(f"Cloudflare rejected {method} {path or '/project'}")
        return body["result"]


def describe(error):
    """The HTTP status and Cloudflare's error codes and messages; never request headers."""
    if not isinstance(error, HTTPError):
        return f"{type(error).__name__}: {getattr(error, 'reason', error)}"
    try:
        errors = json.loads(error.read()).get("errors") or []
        detail = "; ".join(f"{e.get('code')} {e.get('message')}" for e in errors if isinstance(e, dict))
    except Exception:
        detail = ""
    return f"HTTP {error.code}" + (f" ({detail})" if detail else "")


def identity(item):
    if not isinstance(item, dict):
        raise ValueError("missing deployment")
    name, timestamp = item.get("id"), item.get("created_on")
    if not isinstance(name, str) or not re.fullmatch(r"[A-Za-z0-9-]+", name):
        raise ValueError("invalid deployment ID")
    if not isinstance(timestamp, str):
        raise ValueError("missing deployment timestamp")
    try:
        created = datetime.fromisoformat(timestamp.replace("Z", "+00:00"))
    except ValueError as error:
        raise ValueError("invalid deployment timestamp") from error
    if created.tzinfo is None:
        raise ValueError("deployment timestamp has no timezone")
    return name, created


def deployments(api):
    items = {}
    # A truncated listing is an error, never permission to prune a partial view.
    for page in range(1, 201):
        batch = api.request("GET", f"/deployments?page={page}&per_page=25")
        if not isinstance(batch, list):
            raise ValueError("invalid deployment listing")
        if not batch:
            return items
        for item in batch:
            name, _ = identity(item)
            if name in items:
                raise RuntimeError("deployment pagination repeated an ID; retry from a fresh listing")
            items[name] = item
    raise RuntimeError("deployment pagination exceeded 200 pages; no complete listing")


def prune(api, *, delete=False, expected_commit=""):
    production = api.request("GET").get("canonical_deployment")
    production_id, production_time = identity(production)
    result = {"production": production_id, "candidates": [], "deleted": [], "skipped": False}
    if expected_commit and production.get("deployment_trigger", {}).get("metadata", {}).get("commit_hash") != expected_commit:
        result["skipped"] = True
        return result  # A newer deployment or rollback superseded the triggering run.
    items = deployments(api)
    if production_id not in items:
        raise RuntimeError("production missing from deployment listing")
    candidates = sorted(
        name for name, item in items.items()
        if name != production_id and identity(item)[1] < production_time
    )
    result["candidates"] = candidates
    if not delete:
        return result
    for name in candidates:
        # Best-effort drift detection, not a rollback lock: the API does not offer
        # an atomic production predicate for DELETE. See the operator rule above.
        current = api.request("GET").get("canonical_deployment")
        if identity(current) != (production_id, production_time):
            raise RuntimeError("production changed during pruning; stopped")
        # Cloudflare also protects the active production deployment. Force only
        # permits removing aliases on obsolete, non-current deployments.
        api.request("DELETE", f"/deployments/{name}?force=true")
        result["deleted"].append(name)
    # The DELETE acknowledgements are not proof: read the listing back until it agrees.
    for attempt in range(1, READBACK_ATTEMPTS + 1):
        survivors = sorted(set(candidates) & deployments(api).keys())
        if not survivors:
            return result
        if attempt < READBACK_ATTEMPTS:
            sleep(READBACK_DELAY)
    raise RuntimeError("deleted deployments still present: " + ", ".join(survivors))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--delete", action="store_true", help="delete candidates (default: dry run)")
    args = parser.parse_args()
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    try:
        api = Cloudflare(os.environ.get("CLOUDFLARE_ACCOUNT_ID", ""),
                         os.environ.get("CLOUDFLARE_API_TOKEN", ""))
        result = prune(api, delete=args.delete,
                       expected_commit=os.environ.get("EXPECTED_COMMIT_HASH", ""))
    except Exception as error:
        # An annotation, so the cause is readable without the job log.
        message = f"Pruning failed: {type(error).__name__}: {error}"
        print(f"::error::{message}")
        if summary:
            with open(summary, "a", encoding="utf-8") as output:
                output.write(message + "\n")
        return 1
    print(json.dumps(result, sort_keys=True))
    if summary:
        with open(summary, "a", encoding="utf-8") as output:
            if result["skipped"]:
                output.write("Pruning skipped: current production is not the triggering commit.\n")
            else:
                output.write(f"Pages pruning: {len(result['candidates'])} obsolete deployments, "
                             f"{len(result['deleted'])} deleted and verified absent. "
                             f"Production `{result['production']}` retained.\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
