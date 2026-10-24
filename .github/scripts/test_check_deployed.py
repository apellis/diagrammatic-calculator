"""Offline tests of the post-deploy check; run from the repository root or anywhere."""
from html import escape
import json
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import check_deployed as check  # noqa: E402

SITE = "https://site.example"
COMMIT = "0123456789abcdef0123456789abcdef01234567"
POLICY = "# Privacy policy\n\nFirst  paragraph\nwraps & continues.\n\nSecond <one>.\n"


def page(markdown, extra="<p><a href=\"/\">home</a></p>"):
    blocks = check.policy_blocks(markdown)
    body = [f"<h1>{escape(blocks[0])}</h1>"] + [f"<p>{escape(b)}</p>" for b in blocks[1:]]
    return "<!doctype html><html><head><title>t</title></head><body>\n" + "\n".join(body) + extra


class Site:
    def __init__(self, commit=COMMIT, privacy=POLICY, failures=0):
        self.commit, self.privacy, self.failures, self.urls = commit, privacy, failures, []

    def get(self, url):
        self.urls.append(url)
        if self.failures:
            self.failures -= 1
            raise OSError("connection refused")
        if url.startswith(f"{SITE}/build-info.json?"):
            return json.dumps({"commit": self.commit}) + "\n"
        if url.startswith(f"{SITE}/privacy?"):
            return page(self.privacy)
        raise AssertionError(url)


def run(site, attempts=3):
    sleeps = []
    found = check.check(SITE, COMMIT, check.policy_blocks(POLICY), attempts=attempts, delay=7,
                        get=site.get, sleep=sleeps.append)
    return found, sleeps


class CheckDeployedTests(unittest.TestCase):
    def test_policy_blocks_collapse_whitespace_and_strip_the_title_marker(self):
        self.assertEqual(check.policy_blocks(POLICY),
                         ["Privacy policy", "First paragraph wraps & continues.", "Second <one>."])

    def test_served_blocks_unescape_and_skip_markup_outside_paragraphs(self):
        self.assertEqual(check.served_blocks(page(POLICY)),
                         check.policy_blocks(POLICY) + ["home"])

    def test_matching_site_passes_at_once_and_busts_caches(self):
        site = Site()
        self.assertEqual(run(site), ([], []))
        self.assertTrue(all(f"check={COMMIT}" in url for url in site.urls))

    def test_old_commit_fails_after_every_retry(self):
        found, sleeps = run(Site(commit="f" * 40))
        self.assertEqual(sleeps, [7, 7])
        self.assertEqual(len(found), 1)
        self.assertIn("build-info.json names commit", found[0])

    def test_changed_policy_text_fails(self):
        found, _ = run(Site(privacy=POLICY.replace("Second", "Third")))
        self.assertEqual(found, ["/privacy differs from PRIVACY.md at block 3"])

    def test_missing_policy_paragraph_fails(self):
        found, _ = run(Site(privacy="# Privacy policy\n\nFirst paragraph wraps & continues.\n"))
        self.assertEqual(found, ["/privacy differs from PRIVACY.md at block 3"])

    def test_html_instead_of_build_info_fails(self):
        site = Site()
        site.get = lambda url: "<!doctype html><p>app</p>"
        found, _ = run(site, attempts=1)
        self.assertTrue(found[0].startswith("build-info.json unreadable"))
        self.assertTrue(found[1].startswith("/privacy differs"))

    def test_transient_errors_are_retried(self):
        found, sleeps = run(Site(failures=2))
        self.assertEqual((found, sleeps), ([], [7]))

    def test_repository_policy_round_trips_through_the_page_shape(self):
        markdown = (Path(__file__).resolve().parents[2] / "PRIVACY.md").read_text(encoding="utf-8")
        blocks = check.policy_blocks(markdown)
        self.assertGreater(len(blocks), 1)
        self.assertEqual(check.served_blocks(page(markdown))[:len(blocks)], blocks)


if __name__ == "__main__":
    unittest.main()
