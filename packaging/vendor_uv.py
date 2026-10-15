# SPDX-License-Identifier: MIT
"""Download the pinned uv binary into packaging/vendor/ (used by CI and release)."""

import io
import os
import sys
import tarfile
import urllib.request
import zipfile

version = os.environ["UV_VERSION"]
win = sys.platform == "win32"
asset = "uv-x86_64-pc-windows-msvc.zip" if win else "uv-x86_64-unknown-linux-gnu.tar.gz"
url = f"https://github.com/astral-sh/uv/releases/download/{version}/{asset}"
data = urllib.request.urlopen(url).read()
exe = "uv.exe" if win else "uv"
os.makedirs(os.path.join("packaging", "vendor"), exist_ok=True)
target = os.path.join("packaging", "vendor", exe)
if win:
    with zipfile.ZipFile(io.BytesIO(data)) as zf:
        member = next(n for n in zf.namelist() if n.endswith(exe))
        with zf.open(member) as src, open(target, "wb") as dst:
            dst.write(src.read())
else:
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as tf:
        member = next(m for m in tf.getmembers() if m.name == "uv" or m.name.endswith("/uv"))
        with open(target, "wb") as dst:
            dst.write(tf.extractfile(member).read())
    os.chmod(target, 0o755)
print(f"Vendored {target} (uv {version})")
