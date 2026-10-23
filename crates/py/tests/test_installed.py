"""Public API smoke tests: run against an installed wheel, not a source import."""
import importlib.metadata
import json
import unittest

import diagcalc


class InstalledTests(unittest.TestCase):
    def call(self, operation, request):
        response = diagcalc.call(operation, json.dumps(request))
        self.assertIsInstance(response, str)
        return json.loads(response)

    def test_distribution_and_core_versions_agree(self):
        self.assertEqual(importlib.metadata.version("diagcalc"), diagcalc.version())

    def test_exact_nilhecke_dot_slide_is_deterministic(self):
        request = {"context": "nilhecke nu=2 ring=ZZ", "term": "x_1 psi_1"}
        result = self.call("simplify", request)
        self.assertEqual(result["status"], "ok")
        self.assertEqual(result["normal_form"], "1 + psi_1 x_2")
        self.assertEqual(result, self.call("simplify", request))

    def test_odd_distant_crossings_anticommute(self):
        result = self.call("simplify", {
            "context": "odd-nilhecke nu=4", "term": "psi_1 psi_3 + psi_3 psi_1"
        })
        self.assertEqual(result["status"], "ok")
        self.assertEqual(result["normal_form"], "0")

    def test_certified_klr_idempotent(self):
        result = self.call("check", {
            "context": "klr type=A2 nu=2i+j", "name": "idempotent",
            "terms": ["psi_1 psi_2 psi_1 e(iji)"]
        })
        self.assertEqual(result["status"], "ok")
        self.assertIs(result["holds"], True)
        self.assertEqual(result["certified_by"], "certificate")

    def test_malformed_json_returns_syntax_error(self):
        result = json.loads(diagcalc.call("simplify", "{"))
        self.assertEqual(result["status"], "error")
        self.assertEqual(result["kind"], "syntax")


if __name__ == "__main__":
    unittest.main()
