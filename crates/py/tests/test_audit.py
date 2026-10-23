"""Regression coverage for archive path leakage; fixtures contain no host paths."""
import importlib.util
import io
from pathlib import Path
import tarfile
import tempfile
import unittest
import zipfile

spec = importlib.util.spec_from_file_location("package", Path(__file__).parents[1] / "package.py")
assert spec is not None and spec.loader is not None
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


class AuditTests(unittest.TestCase):
    def test_detects_native_and_sbom_paths(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = str(Path(tmp).resolve())
            wheel = Path(tmp) / "bad.whl"
            with zipfile.ZipFile(wheel, "w") as out:
                out.writestr("module.so", b"\x00panic at " + root.encode() + b"/source.rs\x00")
                out.writestr("dist-info/sboms/deps.json", '{"path": "' + root + '/Cargo.toml"}')
            self.assertEqual(package.audit(wheel, [root]),
                             ["module.so", "dist-info/sboms/deps.json"])

    def test_sdist_payload_and_header(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = str(Path(tmp).resolve())
            archive = Path(tmp) / "bad.tar.gz"
            with tarfile.open(archive, "w:gz") as out:
                item = tarfile.TarInfo("source.rs")
                data = root.encode()
                item.size = len(data)
                out.addfile(item, io.BytesIO(data))
                item = tarfile.TarInfo("link")
                item.type = tarfile.SYMTYPE
                item.linkname = root
                out.addfile(item)
            self.assertEqual(package.audit(archive, [root]), ["source.rs", "link"])

    def test_clean_archive_and_each_root(self):
        with tempfile.TemporaryDirectory() as tmp:
            roots = package.private_roots([Path(tmp) / "staging"])
            wheel = Path(tmp) / "fixture.whl"
            for root in roots:
                with zipfile.ZipFile(wheel, "w") as out:
                    out.writestr("payload", root.encode())
                self.assertEqual(package.audit(wheel, roots), ["payload"])
            with zipfile.ZipFile(wheel, "w") as out:
                out.writestr("payload", b"/build-source/crates/core/src/lib.rs")
            self.assertEqual(package.audit(wheel, roots), [])


if __name__ == "__main__":
    unittest.main()
