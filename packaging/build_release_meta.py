# SPDX-License-Identifier: MIT
"""Generate packaging/_release_meta.py for release launcher builds.

The launcher (packaging/launcher.py) imports an optional ``_release_meta``
module providing VERSION, SRC_ZIP_URL and SRC_ZIP_SHA256. CI generates that
module right before running PyInstaller so the frozen launcher knows which
source zip to download and how to verify it.

Usage:
    python packaging/build_release_meta.py \
        --version 1.2.3 \
        --url https://github.com/plynte-labs/LiveAudio/releases/download/v1.2.3/liveaudio-src-1.2.3.zip \
        --sha256 <hex digest>
"""

import argparse
import os

TEMPLATE = '''\
# generated at release build time - do not commit
VERSION = {version!r}
SRC_ZIP_URL = {url!r}
SRC_ZIP_SHA256 = {sha256!r}
'''


def main(argv=None):
    parser = argparse.ArgumentParser(
        description="Write packaging/_release_meta.py with release constants."
    )
    parser.add_argument("--version", required=True, help="release version, e.g. 1.2.3")
    parser.add_argument("--url", required=True, help="source zip download URL")
    parser.add_argument("--sha256", required=True, help="SHA256 hex digest of the source zip")
    args = parser.parse_args(argv)

    target = os.path.join(os.path.dirname(os.path.abspath(__file__)), "_release_meta.py")
    with open(target, "w", encoding="utf-8", newline="\n") as fh:
        fh.write(TEMPLATE.format(version=args.version, url=args.url, sha256=args.sha256))
    print("Wrote %s (version %s)" % (target, args.version))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
