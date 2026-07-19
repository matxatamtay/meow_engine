#!/usr/bin/env python3
from __future__ import annotations

import argparse
from contextlib import contextmanager
import gzip
import hashlib
import json
import os
import platform
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DIST = ROOT / "release" / "dist"


def run(*args: str, cwd: Path = ROOT, check: bool = True) -> subprocess.CompletedProcess[str]:
    return subprocess.run(args, cwd=cwd, text=True, check=check, capture_output=False)


def version() -> str:
    for line in (ROOT / "Cargo.toml").read_text().splitlines():
        if line.startswith('version = "'):
            return line.split('"')[1]
    raise RuntimeError("workspace version not found")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


@contextmanager
def deterministic_tar(path: Path):
    path.unlink(missing_ok=True)
    with path.open("wb") as raw:
        with gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=0) as compressed:
            with tarfile.open(fileobj=compressed, mode="w", format=tarfile.PAX_FORMAT) as archive:
                yield archive
    validate_gzip(path)


def validate_gzip(path: Path) -> None:
    raw = path.read_bytes()
    decoder = zlib.decompressobj(16 + zlib.MAX_WBITS)
    decoder.decompress(raw)
    decoder.flush()
    if not decoder.eof or decoder.unused_data:
        raise RuntimeError(
            f"invalid gzip stream {path}: eof={decoder.eof} trailing={len(decoder.unused_data)}"
        )


def normalized_filter(info: tarfile.TarInfo) -> tarfile.TarInfo:
    info.uid = 0
    info.gid = 0
    info.uname = "root"
    info.gname = "root"
    info.mtime = int(os.environ.get("SOURCE_DATE_EPOCH", "0"))
    return info


def add_tree(archive: tarfile.TarFile, source: Path, arcname: str) -> None:
    archive.add(source, arcname=arcname, recursive=True, filter=normalized_filter)


def package(args: argparse.Namespace) -> None:
    release_version = version()
    arch = platform.machine() or "unknown"
    DIST.mkdir(parents=True, exist_ok=True)
    binary_dir = Path(args.binary_dir)
    if not binary_dir.is_absolute():
        binary_dir = ROOT / binary_dir
    if not args.skip_build:
        if binary_dir != ROOT / "target" / "release":
            raise RuntimeError("custom --binary-dir requires --skip-build")
        run("cargo", "build", "--release", "-p", "meow-browser", "-p", "meow-headless")

    appdir = DIST / f"MeowEngine-{release_version}-{arch}.AppDir"
    if appdir.exists():
        shutil.rmtree(appdir)
    (appdir / "usr" / "bin").mkdir(parents=True, exist_ok=True)
    (appdir / "usr" / "share" / "applications").mkdir(parents=True, exist_ok=True)
    (appdir / "usr" / "share" / "icons" / "hicolor" / "scalable" / "apps").mkdir(parents=True, exist_ok=True)
    for binary in ("meow-browser", "meow-headless"):
        source_binary = binary_dir / binary
        if not source_binary.is_file():
            raise RuntimeError(f"missing release binary: {source_binary}")
        shutil.copy2(source_binary, appdir / "usr" / "bin" / binary)
    shutil.copy2(ROOT / "packaging" / "meowengine.desktop", appdir / "meowengine.desktop")
    shutil.copy2(ROOT / "packaging" / "meowengine.desktop", appdir / "usr" / "share" / "applications" / "meowengine.desktop")
    shutil.copy2(ROOT / "packaging" / "meowengine.svg", appdir / "meowengine.svg")
    shutil.copy2(
        ROOT / "packaging" / "meowengine.svg",
        appdir / "usr" / "share" / "icons" / "hicolor" / "scalable" / "apps" / "meowengine.svg",
    )
    apprun = appdir / "AppRun"
    apprun.write_text('#!/bin/sh\nHERE="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"\nexec "$HERE/usr/bin/meow-browser" "$@"\n')
    apprun.chmod(0o755)

    tar_path = DIST / f"meowengine-{release_version}-{arch}.tar.gz"
    with deterministic_tar(tar_path) as archive:
        add_tree(archive, appdir, appdir.name)

    with tempfile.TemporaryDirectory(prefix="meow-package-smoke-") as temporary:
        with tarfile.open(tar_path, "r:gz") as archive:
            archive.extractall(temporary, filter="data")
        extracted = Path(temporary) / appdir.name
        run(str(extracted / "usr" / "bin" / "meow-headless"), "--help", cwd=extracted)
        run(str(extracted / "usr" / "bin" / "meow-browser"), "--process-smoke-test", cwd=extracted)

    appimage = None
    appimagetool = shutil.which("appimagetool")
    if appimagetool:
        appimage = DIST / f"MeowEngine-{release_version}-{arch}.AppImage"
        env = os.environ.copy()
        env.setdefault("ARCH", arch)
        subprocess.run([appimagetool, str(appdir), str(appimage)], cwd=ROOT, env=env, check=True)

    source_path = DIST / f"meowengine-{release_version}-source.tar.gz"
    listed = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard"], cwd=ROOT, text=True
    ).splitlines()
    excluded = (
        "target/",
        "artifacts/",
        "release/dist/",
        "release/container-target/",
        "release/portable-bin/",
        "release/test-profile/",
        ".idea/",
        "scripts/__pycache__/",
    )
    with deterministic_tar(source_path) as archive:
        for relative in sorted(path for path in listed if not path.startswith(excluded)):
            source = ROOT / relative
            if source.is_file():
                archive.add(source, arcname=f"meowengine-{release_version}/{relative}", filter=normalized_filter)

    artifacts = [tar_path, source_path]
    if appimage:
        artifacts.append(appimage)
    manifest = {
        "schema_version": 1,
        "release": f"v{release_version}",
        "arch": arch,
        "build_baseline": os.environ.get("MEOW_BUILD_BASELINE", "host"),
        "appdir": str(appdir.relative_to(ROOT)),
        "appimage": str(appimage.relative_to(ROOT)) if appimage else None,
        "appimage_status": "built" if appimage else "skipped: appimagetool not installed",
        "artifacts": [
            {"path": str(path.relative_to(ROOT)), "bytes": path.stat().st_size, "sha256": sha256(path)}
            for path in artifacts
        ],
        "smoke_tests": ["meow-headless --help", "meow-browser --process-smoke-test"],
        "evidence": [
            relative
            for relative in (
                "tests/wpt/baseline.json",
                "release/wpt/report.json",
                "release/fuzz-report.json",
                "release/budget-report.json",
                "release/distro-smoke.json",
            )
            if (ROOT / relative).is_file()
        ],
    }
    write_json(DIST / "artifact-manifest.json", manifest)
    print(json.dumps(manifest, indent=2))


def diagnostics(args: argparse.Namespace) -> None:
    profile = Path(args.profile).resolve()
    output = Path(args.output).resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    headless = ROOT / "target" / "release" / "meow-headless"
    browser = ROOT / "target" / "release" / "meow-browser"
    if not (profile / "profile.json").is_file() and headless.is_file():
        profile.mkdir(parents=True, exist_ok=True)
        run(
            str(headless),
            "--profile",
            str(profile),
            "--url",
            "about:blank",
            "--output",
            str(profile / "diagnostics" / "profile-smoke.png"),
        )
    output.unlink(missing_ok=True)
    with tempfile.TemporaryDirectory(prefix="meow-diagnostics-") as temporary:
        root = Path(temporary) / "meowengine-diagnostics"
        root.mkdir()
        for relative in ("profile.json", "recovery", "crashes", "diagnostics"):
            source = profile / relative
            if source.is_dir():
                shutil.copytree(source, root / relative)
            elif source.is_file():
                shutil.copy2(source, root / relative)
        for relative in (
            "release/fuzz-report.json",
            "release/budget-report.json",
            "release/distro-smoke.json",
            "release/wpt/report.json",
            "release/wpt/dashboard.html",
            "artifacts/wpt/report.json",
            "artifacts/wpt/dashboard.html",
        ):
            source = ROOT / relative
            if source.is_file():
                destination = root / relative
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(source, destination)
        process_smoke = {
            "attempted": browser.is_file(),
            "success": False,
            "returncode": None,
            "intentional_content_crash_contained": False,
        }
        if browser.is_file():
            completed = subprocess.run(
                [str(browser), "--process-smoke-test"],
                cwd=ROOT,
                text=True,
                capture_output=True,
                check=False,
            )
            process_log = completed.stdout + completed.stderr
            (root / "process-smoke.log").write_text(process_log)
            process_smoke.update(
                {
                    "success": completed.returncode == 0,
                    "returncode": completed.returncode,
                    "intentional_content_crash_contained": '"content_crash_contained":true' in process_log,
                }
            )
        write_json(root / "crash-recovery.json", process_smoke)
        system = {
            "captured_unix": int(time.time()),
            "platform": platform.platform(),
            "python": sys.version,
            "machine": platform.machine(),
            "processor": platform.processor(),
            "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip(),
            "cargo": subprocess.check_output(["cargo", "--version"], text=True).strip(),
            "environment_keys": sorted(
                key for key in os.environ if key.startswith(("WAYLAND_", "XDG_", "DISPLAY", "RUST_", "MEOW_"))
            ),
            "privacy": "environment values and browser data are excluded; only selected key names are recorded",
        }
        write_json(root / "system.json", system)
        with deterministic_tar(output) as archive:
            add_tree(archive, root, root.name)
    manifest_path = DIST / "artifact-manifest.json"
    if manifest_path.is_file():
        manifest = json.loads(manifest_path.read_text())
        artifact = {
            "path": str(output.relative_to(ROOT)),
            "bytes": output.stat().st_size,
            "sha256": sha256(output),
        }
        manifest["artifacts"] = [
            existing
            for existing in manifest.get("artifacts", [])
            if existing.get("path") != artifact["path"]
        ]
        manifest["artifacts"].append(artifact)
        write_json(manifest_path, manifest)
    print(f"diagnostics bundle: {output} ({output.stat().st_size} bytes)")


def verify(args: argparse.Namespace) -> None:
    required = [
        ROOT / "tests" / "wpt" / "baseline.json",
        ROOT / "release" / "wpt" / "report.json",
        ROOT / "release" / "wpt" / "dashboard.html",
        ROOT / "release" / "fuzz-report.json",
        ROOT / "release" / "budget-report.json",
        ROOT / "release" / "distro-smoke.json",
        ROOT / "release" / "dist" / "artifact-manifest.json",
        ROOT / "release" / "dist" / "meowengine-diagnostics.tar.gz",
        ROOT / "docs" / "privacy.md",
        ROOT / "docs" / "threat-model.md",
        ROOT / "docs" / "known-issues.md",
    ]
    missing = [str(path.relative_to(ROOT)) for path in required if not path.is_file()]
    if missing:
        raise RuntimeError("release verification missing: " + ", ".join(missing))
    budget = json.loads((ROOT / "release" / "budget-report.json").read_text())
    fuzz = json.loads((ROOT / "release" / "fuzz-report.json").read_text())
    if budget.get("violations"):
        raise RuntimeError("budget report contains violations")
    if fuzz.get("new_crashes") != 0:
        raise RuntimeError("fuzz report contains crashes")
    distro = json.loads((ROOT / "release" / "distro-smoke.json").read_text())
    if any(result.get("status") != "pass" for result in distro.get("results", [])):
        raise RuntimeError("distro smoke report contains a failure")
    manifest = json.loads((ROOT / "release" / "dist" / "artifact-manifest.json").read_text())
    diagnostics_path = "release/dist/meowengine-diagnostics.tar.gz"
    if not any(item.get("path") == diagnostics_path for item in manifest.get("artifacts", [])):
        raise RuntimeError("artifact manifest does not include diagnostics bundle")
    print("release verification passed")


def write_json(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser(description="MeowEngine public-alpha release tooling")
    commands = root.add_subparsers(dest="command", required=True)
    package_parser = commands.add_parser("package")
    package_parser.add_argument("--skip-build", action="store_true")
    package_parser.add_argument("--binary-dir", default="target/release")
    package_parser.set_defaults(function=package)
    diagnostics_parser = commands.add_parser("diagnostics")
    diagnostics_parser.add_argument("--profile", default="artifacts/profile")
    diagnostics_parser.add_argument("--output", default="release/dist/meowengine-diagnostics.tar.gz")
    diagnostics_parser.set_defaults(function=diagnostics)
    verify_parser = commands.add_parser("verify")
    verify_parser.set_defaults(function=verify)
    return root


if __name__ == "__main__":
    arguments = parser().parse_args()
    arguments.function(arguments)
