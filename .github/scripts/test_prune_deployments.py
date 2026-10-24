"""Exercise the production pruning predicate without Cloudflare credentials."""
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import unittest
from unittest.mock import patch
from urllib.error import HTTPError

SPEC = importlib.util.spec_from_file_location(
    "prune_deployments", Path(__file__).with_name("prune_deployments.py")
)
assert SPEC is not None and SPEC.loader is not None
prune = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(prune)


def deployment(name, day, commit="current"):
    return {
        "id": name,
        "created_on": f"2026-10-{day:02}T12:00:00.000000Z",
        "deployment_trigger": {"metadata": {"commit_hash": commit}},
    }


class FakeAPI:
    def __init__(self):
        self.production: dict | None = deployment("production", 23)
        self.items = [deployment("old", 20), self.production, deployment("new", 24)]
        self.deleted = []
        self.project_reads = 0
        self.change_on_read: int | None = None
        self.keep_deleted = False

    def request(self, method, path=""):
        if not path:
            self.project_reads += 1
            if self.project_reads == self.change_on_read:
                self.production = deployment("other-production", 19)
            return {"canonical_deployment": self.production}
        if method == "DELETE":
            name = path.split("/")[-1].split("?")[0]
            self.deleted.append(name)
            if not self.keep_deleted:
                self.items = [item for item in self.items if item["id"] != name]
            return None
        page = int(path.split("page=")[1].split("&")[0])
        return self.items if page == 1 else []


class PruneTests(unittest.TestCase):
    def setUp(self):
        self.sleeps = []
        sleeper = patch.object(prune, "sleep", self.sleeps.append)
        sleeper.start()
        self.addCleanup(sleeper.stop)

    def test_default_dry_run_changes_nothing(self):
        api = FakeAPI()
        result = prune.prune(api)
        self.assertEqual(result["candidates"], ["old"])
        self.assertEqual(api.deleted, [])

    def test_delete_keeps_current_equal_timestamp_and_newer(self):
        api = FakeAPI()
        api.items.append(deployment("same-time", 23))
        result = prune.prune(api, delete=True)
        self.assertEqual(api.deleted, ["old"])
        self.assertEqual(result["deleted"], ["old"])
        self.assertEqual({x["id"] for x in api.items}, {"production", "new", "same-time"})

    def test_stale_workflow_run_cannot_prune(self):
        api = FakeAPI()
        result = prune.prune(api, delete=True, expected_commit="superseded")
        self.assertTrue(result["skipped"])
        self.assertEqual(api.deleted, [])

    def test_missing_production_fails_closed(self):
        api = FakeAPI()
        api.production = None
        with self.assertRaises(ValueError):
            prune.prune(api, delete=True)
        self.assertEqual(api.deleted, [])

    def test_bad_timestamp_or_identifier_fails_before_deleting(self):
        for field, value in [("created_on", "yesterday"), ("id", "../other-project")]:
            with self.subTest(field=field):
                api = FakeAPI()
                api.items[0][field] = value
                with self.assertRaises(ValueError):
                    prune.prune(api, delete=True)
                self.assertEqual(api.deleted, [])

    def test_production_change_stops_deletion(self):
        api = FakeAPI()
        api.change_on_read = 2
        with self.assertRaisesRegex(RuntimeError, "production changed"):
            prune.prune(api, delete=True)
        self.assertEqual(api.deleted, [])

    def test_acknowledgement_without_deletion_is_failure_after_every_readback(self):
        api = FakeAPI()
        api.keep_deleted = True
        with self.assertRaisesRegex(RuntimeError, "still present: old"):
            prune.prune(api, delete=True)
        self.assertEqual(self.sleeps, [prune.READBACK_DELAY] * (prune.READBACK_ATTEMPTS - 1))

    def test_listing_that_lags_the_delete_is_read_back_again(self):
        api = FakeAPI()
        api.keep_deleted = True
        base = api.request
        listings = []
        def request(method, path=""):
            if method == "GET" and path.startswith("/deployments?page=1"):
                listings.append(path)
                if len(listings) == 3:  # selection, first readback (stale), second readback
                    api.items = [item for item in api.items if item["id"] not in api.deleted]
            return base(method, path)
        api.request = request
        self.assertEqual(prune.prune(api, delete=True)["deleted"], ["old"])
        self.assertEqual(self.sleeps, [prune.READBACK_DELAY])

    def test_manual_rollback_between_requests_is_not_atomic(self):
        # Document the operator constraint, not a lock the API cannot provide.
        # A manual rollback must wait until the pruning run has stopped.
        api = FakeAPI()
        api.items.append(deployment("rollback", 19))
        base = api.request
        def request(method, path=""):
            if method == "DELETE":
                api.production = deployment("rollback", 19)
                name = path.split("/")[-1].split("?")[0]
                if name == api.production["id"]:
                    raise RuntimeError("provider protects active production")
            return base(method, path)
        api.request = request
        with self.assertRaisesRegex(RuntimeError, "production changed"):
            prune.prune(api, delete=True)
        self.assertEqual(api.deleted, ["old"])
        self.assertIn("rollback", [item["id"] for item in api.items])

    def test_pagination_collects_every_page_before_delete(self):
        api = FakeAPI()
        base = api.request
        def request(method, path=""):
            if method == "GET" and path.startswith("/deployments?"):
                page = int(path.split("page=")[1].split("&")[0])
                return api.items[page - 1:page]
            return base(method, path)
        api.request = request
        self.assertEqual(prune.prune(api, delete=True)["deleted"], ["old"])

    def test_endless_pagination_fails_without_deleting(self):
        api = FakeAPI()
        base = api.request
        api.request = lambda method, path="": [deployment("old", 20)] if path else base(method, path)
        with self.assertRaisesRegex(RuntimeError, "pagination"):
            prune.prune(api, delete=True)
        self.assertEqual(api.deleted, [])

    def test_http_api_checks_success_envelope(self):
        class Response:
            def __enter__(self):
                return self
            def __exit__(self, *_):
                pass
            def read(self):
                return json.dumps({"success": False, "result": []}).encode()
        with patch.object(prune, "urlopen", return_value=Response()) as opener:
            api = prune.Cloudflare("a" * 32, "test-token")
            with self.assertRaisesRegex(RuntimeError, "rejected"):
                api.request("GET")
            request = opener.call_args.args[0]
            self.assertEqual(request.full_url, "https://api.cloudflare.com/client/v4/accounts/" + "a" * 32 + "/pages/projects/diagcalc")
            self.assertEqual(opener.call_args.kwargs["timeout"], 30)

    def test_transient_read_errors_are_retried_and_others_described(self):
        class Response:
            def __enter__(self):
                return self
            def __exit__(self, *_):
                pass
            def read(self):
                return json.dumps({"success": True, "result": {"ok": 1}}).encode()
        def failure(code):
            body = io.BytesIO(json.dumps({"errors": [{"code": 8000007, "message": "nope"}]}).encode())
            return HTTPError("https://api.example/x", code, "error", {}, body)
        api = prune.Cloudflare("a" * 32, "test-token")
        with patch.object(prune, "urlopen", side_effect=[failure(502), Response()]):
            self.assertEqual(api.request("GET"), {"ok": 1})
        with patch.object(prune, "urlopen", side_effect=[failure(400)]) as opener:
            with self.assertRaisesRegex(RuntimeError, r"GET /project failed: HTTP 400 \(8000007 nope\)"):
                api.request("GET")
            self.assertEqual(opener.call_count, 1)
        with patch.object(prune, "urlopen", side_effect=[failure(503)]) as opener:
            with self.assertRaisesRegex(RuntimeError, "HTTP 503"):
                api.request("DELETE", "/deployments/old?force=true")
            self.assertEqual(opener.call_count, 1)

    def test_failure_is_reported_as_an_annotation(self):
        out = io.StringIO()
        with patch.dict(prune.os.environ, {"CLOUDFLARE_ACCOUNT_ID": "bad", "CLOUDFLARE_API_TOKEN": "t",
                                           "GITHUB_STEP_SUMMARY": ""}), \
             patch.object(prune.sys, "argv", ["prune"]), contextlib.redirect_stdout(out):
            self.assertEqual(prune.main(), 1)
        self.assertIn("::error::Pruning failed: ValueError: CLOUDFLARE_ACCOUNT_ID", out.getvalue())


if __name__ == "__main__":
    unittest.main()
