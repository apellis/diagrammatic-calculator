"""Source metadata contract; run with Python 3.11+ from any directory."""
import pathlib
import tomllib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[3]


class PackagingTests(unittest.TestCase):
    def test_python_distribution_uses_workspace_version_and_existing_module(self):
        manifest = ROOT / "pyproject.toml"
        self.assertTrue(manifest.is_file(), "root Python distribution metadata is missing")
        config = tomllib.loads(manifest.read_text())
        self.assertEqual(config["build-system"]["build-backend"], "maturin")
        self.assertEqual(config["project"]["name"], "diagcalc")
        self.assertEqual(config["project"]["dynamic"], ["version"])
        self.assertEqual(config["tool"]["maturin"]["manifest-path"], "crates/py/Cargo.toml")
        self.assertEqual(config["tool"]["maturin"]["module-name"], "diagcalc")
        self.assertEqual(config["project"]["requires-python"], ">=3.9")
        self.assertEqual(config["tool"]["maturin"]["sbom"],
                         {"rust": False, "auditwheel": False})
        self.assertIn("maturin>=1.15,<2", config["build-system"]["requires"])
        includes = config["tool"]["maturin"]["include"]
        for pattern in ("presentations/*.pres.toml", "spec/ops/*.schema.json"):
            self.assertIn({"path": pattern, "format": "sdist"}, includes)


if __name__ == "__main__":
    unittest.main()
