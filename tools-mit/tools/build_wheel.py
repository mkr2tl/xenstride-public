"""Build this fixed pure-Python package offline using only the standard library.

This is a narrow bootstrap, not a general packaging backend. Normal source builds
use setuptools through pyproject.toml. No setup scripts or downloaded code run here.
"""
# SPDX-License-Identifier: MIT
import argparse
import base64
import csv
import hashlib
import io
from pathlib import Path
import tomllib
from zipfile import ZipFile, ZipInfo, ZIP_DEFLATED

ROOT = Path(__file__).resolve().parents[1]


def build(output):
    project = tomllib.loads((ROOT / "pyproject.toml").read_text(encoding="utf-8"))["project"]
    name, version = project["name"], project["version"]
    if name != "xenstride" or version != "0.1.0" or project["dependencies"]:
        raise ValueError("bootstrap supports only the reviewed dependency-free v0.1.0 metadata")
    metadata_dir = f"{name}-{version}.dist-info"
    files = {}
    for p in sorted((ROOT / "src" / "xenstride").iterdir()):
        if p.is_file() and (p.suffix == ".py" or p.name == "py.typed"):
            files["xenstride/" + p.name] = p.read_bytes()
    for p in [ROOT / "LICENSE", *sorted((ROOT / "LICENSES").glob("*.txt"))]:
        files[metadata_dir + "/licenses/" + str(p.relative_to(ROOT)).replace("\\", "/")] = p.read_bytes()
    headers = ["Metadata-Version: 2.4", f"Name: {name}", f"Version: {version}",
               f"Summary: {project['description']}", f"Requires-Python: {project['requires-python']}",
               "License-Expression: MIT", "Description-Content-Type: text/markdown"]
    headers.extend("License-File: " + key.split("/licenses/", 1)[1] for key in files if "/licenses/" in key)
    files[metadata_dir + "/METADATA"] = ("\n".join(headers) + "\n\n" + (ROOT / "README.md").read_text(encoding="utf-8")).encode()
    files[metadata_dir + "/WHEEL"] = b"Wheel-Version: 1.0\nGenerator: xenstride-offline-bootstrap\nRoot-Is-Purelib: true\nTag: py3-none-any\n"
    records = io.StringIO(newline="")
    writer = csv.writer(records, lineterminator="\n")
    for key, value in sorted(files.items()):
        digest = base64.urlsafe_b64encode(hashlib.sha256(value).digest()).rstrip(b"=").decode("ascii")
        writer.writerow([key, "sha256=" + digest, len(value)])
    writer.writerow([metadata_dir + "/RECORD", "", ""])
    files[metadata_dir + "/RECORD"] = records.getvalue().encode()
    output = Path(output)
    output.mkdir(parents=True, exist_ok=True)
    wheel = output / f"{name}-{version}-py3-none-any.whl"
    with ZipFile(wheel, "w") as archive:
        for key, value in sorted(files.items()):
            info = ZipInfo(key, date_time=(2026, 1, 1, 0, 0, 0))
            info.compress_type = ZIP_DEFLATED
            info.external_attr = 0o100644 << 16
            archive.writestr(info, value)
    return wheel


if __name__ == "__main__":
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("output", type=Path)
    print(build(ap.parse_args().output).name)
