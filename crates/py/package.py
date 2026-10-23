"""Build and audit local Python distributions without publishing (Python 3.9+)."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
from urllib.parse import quote
import zipfile


def private_roots(extra=()):
    roots = [Path.cwd(), Path.home(), Path(tempfile.gettempdir())]
    roots += [Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo"))]
    roots += [Path(p) for p in extra]
    return sorted({str(p.resolve()) for p in roots}, key=len)


def audit(artifact, roots):
    """Scan decompressed names, headers and every payload, including native code/SBOMs."""
    needles = set()
    for root in roots:
        for text in (root, root.replace("\\", "/"), quote(root, safe=""),
                     quote(root, safe="/"), json.dumps(root)[1:-1]):
            needles.add(text.encode())
            needles.add(text.encode("utf-16le"))
    findings = []

    def check(name, data):
        if any(n in data for n in needles):
            findings.append(name)

    if zipfile.is_zipfile(artifact):
        with zipfile.ZipFile(artifact) as archive:
            check("<archive-comment>", archive.comment)
            for item in archive.infolist():
                check(item.filename, item.filename.encode() + item.extra + item.comment
                      + archive.read(item))
    else:
        with tarfile.open(artifact, "r:gz") as archive:
            check("<archive-headers>", json.dumps(archive.pax_headers).encode())
            for item in archive:
                stream = archive.extractfile(item) if item.isfile() else None
                data = stream.read() if stream else b""
                check(item.name, json.dumps([item.name, item.linkname, item.uname,
                                            item.gname, item.pax_headers]).encode() + data)
    return findings


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    build = commands.add_parser("build")
    build.add_argument("--out", required=True, type=Path)
    build.add_argument("--target-dir", required=True, type=Path)
    scan = commands.add_parser("audit")
    scan.add_argument("--forbid-root", action="append", default=[])
    scan.add_argument("artifacts", nargs="+", type=Path)
    args = parser.parse_args()
    if args.command == "audit":
        report = {str(p): audit(p, private_roots(args.forbid_root)) for p in args.artifacts}
        print(json.dumps(report, indent=2))
        return int(any(report.values()))

    # Cargo's encoded form preserves whitespace in paths and applies to dependencies,
    # unlike maturin's rustc-args (which apply only to the final extension crate).
    env = os.environ.copy()
    if env.get("RUSTFLAGS") or env.get("CARGO_ENCODED_RUSTFLAGS"):
        parser.error("unset Rust flags before the safe build; do not override path remapping")
    roots = private_roots([args.target_dir, args.out])
    env["CARGO_ENCODED_RUSTFLAGS"] = "\x1f".join(
        "--remap-path-prefix=" + root + "=/build-source" for root in roots
    )
    subprocess.run([sys.executable, "-m", "maturin", "build", "--release", "--locked",
                    "--jobs", "2", "--out", str(args.out.resolve()),
                    "--target-dir", str(args.target_dir.resolve())], env=env, check=True)
    wheels = list(args.out.glob("*.whl"))
    if not wheels:
        raise RuntimeError("build produced no wheels")
    report = {str(p): audit(p, roots) for p in wheels}
    print(json.dumps(report, indent=2))
    return int(any(report.values()))


if __name__ == "__main__":
    sys.exit(main())
