#!/usr/bin/env python3
"""Assemble latest.json from downloaded Tauri updater artifacts.

Each updater artifact ships with a `.sig` sidecar written by the Tauri bundler.
This script adds the SHA-256 digest and size that Ferriterm verifies before it
accepts an update, so a truncated or tampered download is rejected even when
the signature file is missing from a mirror.
"""

import argparse
import datetime
import hashlib
import json
import pathlib
import sys
from urllib.parse import quote


def classify(name: str) -> str | None:
    if name.endswith(".app.tar.gz"):
        return "darwin-aarch64" if "aarch64" in name else "darwin-x86_64"
    if name.endswith(".AppImage.tar.gz") or name.endswith(".AppImage"):
        return "linux-x86_64"
    if name.endswith(".nsis.zip"):
        return "windows-x86_64"
    if name.endswith(".msi.zip"):
        return "windows-x86_64"
    return None


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--dir", required=True, help="directory with release assets")
    parser.add_argument("--version", required=True)
    parser.add_argument("--base-url", required=True, help="release download URL prefix")
    parser.add_argument("--notes", default="")
    parser.add_argument("--output", required=True)
    args = parser.parse_args()

    assets = pathlib.Path(args.dir)
    platforms: dict[str, dict] = {}
    for sig in sorted(assets.glob("*.sig")):
        artifact = sig.with_suffix("")
        if not artifact.is_file():
            continue
        platform = classify(artifact.name)
        if platform is None:
            continue
        signature = sig.read_text(encoding="utf-8").strip()
        if not signature:
            print(f"skipping {artifact.name}: empty signature", file=sys.stderr)
            continue
        # Prefer the NSIS updater bundle over MSI when both are published.
        if (
            platform == "windows-x86_64"
            and "nsis" not in artifact.name
            and platform in platforms
            and "nsis" in platforms[platform]["url"]
        ):
            continue
        digest = hashlib.sha256(artifact.read_bytes()).hexdigest()
        platforms[platform] = {
            "url": f"{args.base_url}/{quote(artifact.name)}",
            "signature": signature,
            "sha256": digest,
            "size": artifact.stat().st_size,
        }

    if not platforms:
        print("no updater artifacts found", file=sys.stderr)
        return 1

    manifest = {
        "version": args.version,
        "notes": args.notes,
        "pubDate": datetime.datetime.now(datetime.timezone.utc)
        .replace(microsecond=0)
        .isoformat()
        .replace("+00:00", "Z"),
        "platforms": platforms,
    }
    pathlib.Path(args.output).write_text(
        json.dumps(manifest, indent=2) + "\n", encoding="utf-8"
    )
    print(f"wrote {args.output} with {len(platforms)} platform(s)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
