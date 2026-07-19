#!/usr/bin/env python3
from __future__ import annotations

import argparse
import copy
import difflib
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import tomllib
import urllib.parse
import urllib.request
from collections import Counter
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
LOCKFILE = ROOT / "Cargo.lock"
DENY_CONFIG = ROOT / "deny.toml"
V8_MANIFEST = ROOT / "vendor" / "v8" / "provenance.json"
REPORT_DIR = ROOT / "release" / "supply-chain"
REPORT_NAMES = (
    "sbom.spdx.json",
    "licenses.json",
    "dependencies.json",
    "v8-provenance.json",
    "manifest.json",
)
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
GIT_REVISION_RE = re.compile(r"^[0-9a-f]{40}$")
VERSION_RE = re.compile(r"^[0-9]+\.[0-9]+\.[0-9]+$")
FIXED_SPDX_CREATED = "1970-01-01T00:00:00Z"


class PolicyError(RuntimeError):
    pass


def canonical_json(value: Any) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(canonical_json(value))


def read_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text())
    except FileNotFoundError as error:
        raise PolicyError(f"missing required file: {path.relative_to(ROOT)}") from error
    except json.JSONDecodeError as error:
        raise PolicyError(f"invalid JSON in {path.relative_to(ROOT)}: {error}") from error


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def relative_path(path: str | Path) -> str:
    candidate = Path(path)
    try:
        return str(candidate.resolve().relative_to(ROOT))
    except (OSError, ValueError):
        return str(candidate)


def require(condition: bool, message: str) -> None:
    if not condition:
        raise PolicyError(message)


def expected_cache_key(version: str, artifact: dict[str, Any]) -> str:
    return (
        f"rusty-v8/v{version}/{artifact['profile']}/{artifact['target']}/"
        f"{artifact['sha256'][:16]}/{artifact['filename']}"
    )


def validate_v8_manifest(manifest: dict[str, Any]) -> None:
    require(manifest.get("schema_version") == 1, "V8 manifest schema_version must be 1")
    policy = manifest.get("policy", {})
    require(policy.get("status") == "pinned", "V8 policy status must be pinned")
    require(policy.get("network_default") == "deny", "V8 network default must be deny")
    require(policy.get("mutable_references") == "deny", "mutable V8 references must be denied")

    binding = manifest.get("binding", {})
    version = binding.get("version", "")
    require(binding.get("crate") == "v8", "binding crate must be v8")
    require(bool(VERSION_RE.fullmatch(version)), "binding version must be a full numeric semver")
    require(binding.get("tag") == f"v{version}", "binding tag must exactly match binding version")
    require(
        binding.get("repository") == "https://github.com/denoland/rusty_v8",
        "binding repository must be the canonical rusty_v8 repository",
    )
    require(bool(GIT_REVISION_RE.fullmatch(binding.get("revision", ""))), "binding revision must be a 40-hex commit")
    require(binding.get("license") == "MIT", "rusty_v8 license must be recorded as MIT")

    engine = manifest.get("engine_source", {})
    require(
        engine.get("repository") == "https://github.com/denoland/v8.git",
        "engine source must use the rusty_v8 V8 fork selected by its submodule",
    )
    require(
        engine.get("upstream_repository") == "https://chromium.googlesource.com/v8/v8",
        "upstream V8 repository is not canonical",
    )
    require(bool(GIT_REVISION_RE.fullmatch(engine.get("revision", ""))), "engine revision must be a 40-hex commit")
    require(engine.get("license") == "BSD-3-Clause", "V8 license must be recorded as BSD-3-Clause")

    build = manifest.get("build_policy", {})
    require(build.get("default_mode") == "prebuilt-static", "default V8 mode must be prebuilt-static")
    require(build.get("network_during_release_build") == "deny", "release builds must not fetch V8")
    require(build.get("source_fallback_requires_new_checksum") is True, "source fallback must require a new checksum")

    cache = manifest.get("cache_policy", {})
    require(cache.get("immutable") is True, "V8 cache must be immutable")
    require(cache.get("key_algorithm") == "sha256", "V8 cache must be keyed by SHA-256")
    require(cache.get("on_checksum_mismatch") == "delete-and-fail", "bad cache entries must be deleted and rejected")

    artifacts = manifest.get("artifacts")
    require(isinstance(artifacts, list) and artifacts, "at least one V8 artifact must be pinned")
    seen_targets: set[str] = set()
    expected_prefix = f"https://github.com/denoland/rusty_v8/releases/download/v{version}/"
    for artifact in artifacts:
        target = artifact.get("target", "")
        require(target not in seen_targets, f"duplicate V8 target: {target}")
        seen_targets.add(target)
        require(artifact.get("profile") == "release", f"{target}: only release static archives are accepted")
        checksum = artifact.get("sha256", "")
        require(bool(SHA256_RE.fullmatch(checksum)), f"{target}: sha256 must be 64 lowercase hex characters")
        require(isinstance(artifact.get("size_bytes"), int) and artifact["size_bytes"] > 1_000_000, f"{target}: invalid archive size")
        filename = artifact.get("filename", "")
        require(filename == Path(artifact.get("url", "")).name, f"{target}: filename does not match URL")
        require(artifact.get("url", "").startswith(expected_prefix), f"{target}: artifact URL is not release-pinned")
        require("latest" not in artifact.get("url", ""), f"{target}: mutable latest URL is forbidden")
        require(artifact.get("cache_key") == expected_cache_key(version, artifact), f"{target}: cache key drift")
    require("x86_64-unknown-linux-gnu" in seen_targets, "the primary Linux x86_64 V8 archive must be pinned")

    evidence = manifest.get("license_evidence")
    require(isinstance(evidence, list) and len(evidence) >= 2, "license evidence for rusty_v8 and V8 is required")
    expected_licenses = {"rusty_v8": "MIT", "v8": "BSD-3-Clause"}
    revisions = {"rusty_v8": binding["revision"], "v8": engine["revision"]}
    seen_components: set[str] = set()
    for item in evidence:
        component = item.get("component", "")
        require(component in expected_licenses, f"unknown license evidence component: {component}")
        require(component not in seen_components, f"duplicate license evidence: {component}")
        seen_components.add(component)
        require(item.get("expression") == expected_licenses[component], f"{component}: license expression drift")
        require(bool(SHA256_RE.fullmatch(item.get("sha256", ""))), f"{component}: license hash must be SHA-256")
        require(revisions[component] in item.get("url", ""), f"{component}: license URL is not commit-pinned")
        require(item.get("url", "").startswith("https://raw.githubusercontent.com/"), f"{component}: license evidence must use HTTPS")
    require(seen_components == set(expected_licenses), "license evidence is incomplete")


def cargo_metadata() -> dict[str, Any]:
    try:
        output = subprocess.check_output(
            ["cargo", "metadata", "--format-version", "1", "--locked"],
            cwd=ROOT,
            text=True,
        )
    except subprocess.CalledProcessError as error:
        raise PolicyError(f"cargo metadata failed with exit code {error.returncode}") from error
    return json.loads(output)


def lock_packages() -> dict[tuple[str, str, str | None], dict[str, Any]]:
    data = tomllib.loads(LOCKFILE.read_text())
    result: dict[tuple[str, str, str | None], dict[str, Any]] = {}
    for package in data.get("package", []):
        key = (package["name"], package["version"], package.get("source"))
        result[key] = package
    return result


def normalized_license(expression: str | None) -> str:
    if not expression:
        return "NOASSERTION"
    aliases = {
        "MIT/Apache-2.0": "MIT OR Apache-2.0",
        "Apache-2.0/MIT": "Apache-2.0 OR MIT",
        "Apache-2.0 / MIT": "Apache-2.0 OR MIT",
        "Unlicense/MIT": "Unlicense OR MIT",
    }
    return aliases.get(expression, expression)


def package_checksum(package: dict[str, Any], locked: dict[tuple[str, str, str | None], dict[str, Any]]) -> str | None:
    entry = locked.get((package["name"], package["version"], package.get("source")))
    return entry.get("checksum") if entry else None


def package_kind(package: dict[str, Any], workspace_members: set[str]) -> str:
    if package["id"] in workspace_members:
        return "workspace"
    source = package.get("source") or ""
    if source.startswith("registry+"):
        return "registry"
    if source.startswith("git+"):
        return "git"
    return "path"


def stable_manifest_path(package: dict[str, Any], workspace_members: set[str]) -> str:
    if package["id"] in workspace_members:
        return relative_path(package["manifest_path"])
    kind = package_kind(package, workspace_members)
    return f"external/{kind}/{package['name']}/{package['version']}/Cargo.toml"


def stable_package_id(package: dict[str, Any], workspace_members: set[str]) -> str:
    if package["id"] in workspace_members:
        manifest = stable_manifest_path(package, workspace_members)
        return f"workspace:{Path(manifest).parent.as_posix()}#{package['name']}@{package['version']}"
    source = package.get("source") or package_kind(package, workspace_members)
    return f"{source}#{package['name']}@{package['version']}"


def spdx_id(package_id: str) -> str:
    return f"SPDXRef-Package-{sha256_bytes(package_id.encode())[:20]}"


def package_purpose(package: dict[str, Any]) -> str:
    kinds = {kind for target in package.get("targets", []) for kind in target.get("kind", [])}
    return "APPLICATION" if "bin" in kinds else "LIBRARY"


def package_download_location(package: dict[str, Any]) -> str:
    source = package.get("source") or ""
    if source.startswith("registry+"):
        return f"https://crates.io/api/v1/crates/{package['name']}/{package['version']}/download"
    if source.startswith("git+"):
        return source.removeprefix("git+").split("#", 1)[0]
    return "NOASSERTION"


def package_record(
    package: dict[str, Any],
    locked: dict[tuple[str, str, str | None], dict[str, Any]],
    workspace_members: set[str],
) -> dict[str, Any]:
    checksum = package_checksum(package, locked)
    return {
        "checksum_sha256": checksum,
        "id": stable_package_id(package, workspace_members),
        "kind": package_kind(package, workspace_members),
        "license": normalized_license(package.get("license")),
        "manifest_path": stable_manifest_path(package, workspace_members),
        "name": package["name"],
        "repository": package.get("repository"),
        "source": package.get("source"),
        "version": package["version"],
        "workspace_member": package["id"] in workspace_members,
    }


def dependency_edges(metadata: dict[str, Any]) -> list[dict[str, Any]]:
    edges: list[dict[str, Any]] = []
    workspace_members = set(metadata["workspace_members"])
    packages_by_id = {package["id"]: package for package in metadata["packages"]}
    stable_ids = {
        package_id: stable_package_id(package, workspace_members)
        for package_id, package in packages_by_id.items()
    }
    resolve = metadata.get("resolve") or {}
    for node in resolve.get("nodes", []):
        if node["id"] not in stable_ids:
            continue
        for dependency in node.get("deps", []):
            if dependency["pkg"] not in stable_ids:
                continue
            kinds = sorted(
                {
                    f"{item.get('kind') or 'normal'}:{item.get('target') or '*'}"
                    for item in dependency.get("dep_kinds", [])
                }
            )
            edges.append(
                {
                    "from": stable_ids[node["id"]],
                    "kinds": kinds,
                    "name": dependency["name"],
                    "to": stable_ids[dependency["pkg"]],
                }
            )
    return sorted(edges, key=lambda item: (item["from"], item["to"], item["name"], item["kinds"]))


def build_dependency_report(metadata: dict[str, Any], locked: dict[tuple[str, str, str | None], dict[str, Any]]) -> dict[str, Any]:
    workspace_members = set(metadata["workspace_members"])
    packages = sorted(
        (package_record(package, locked, workspace_members) for package in metadata["packages"]),
        key=lambda item: (item["name"], item["version"], item["id"]),
    )
    source_counts = Counter(package["kind"] for package in packages)
    return {
        "cargo_lock_sha256": sha256_file(LOCKFILE),
        "edges": dependency_edges(metadata),
        "package_count": len(packages),
        "packages": packages,
        "schema_version": 1,
        "source_counts": dict(sorted(source_counts.items())),
        "workspace_members": sorted(package["name"] for package in packages if package["workspace_member"]),
    }


def build_license_report(
    metadata: dict[str, Any],
    locked: dict[tuple[str, str, str | None], dict[str, Any]],
) -> dict[str, Any]:
    workspace_members = set(metadata["workspace_members"])
    packages = sorted(
        (package_record(package, locked, workspace_members) for package in metadata["packages"]),
        key=lambda item: (item["license"], item["name"], item["version"], item["id"]),
    )
    license_counts = Counter(package["license"] for package in packages)
    deny = tomllib.loads(DENY_CONFIG.read_text())
    unresolved = [
        {"name": package["name"], "version": package["version"], "id": package["id"]}
        for package in packages
        if package["license"] == "NOASSERTION"
    ]
    return {
        "allowed_licenses": sorted(deny.get("licenses", {}).get("allow", [])),
        "cargo_lock_sha256": sha256_file(LOCKFILE),
        "license_counts": dict(sorted(license_counts.items())),
        "package_count": len(packages),
        "packages": packages,
        "schema_version": 1,
        "unresolved": unresolved,
    }


def build_spdx(metadata: dict[str, Any], locked: dict[tuple[str, str, str | None], dict[str, Any]]) -> dict[str, Any]:
    workspace_members = set(metadata["workspace_members"])
    packages_by_id = {package["id"]: package for package in metadata["packages"]}
    stable_ids = {
        package_id: stable_package_id(package, workspace_members)
        for package_id, package in packages_by_id.items()
    }
    packages: list[dict[str, Any]] = []
    for package in sorted(metadata["packages"], key=lambda item: (item["name"], item["version"], item["id"])):
        checksum = package_checksum(package, locked)
        spdx_package: dict[str, Any] = {
            "SPDXID": spdx_id(stable_ids[package["id"]]),
            "copyrightText": "NOASSERTION",
            "downloadLocation": package_download_location(package),
            "externalRefs": [
                {
                    "referenceCategory": "PACKAGE-MANAGER",
                    "referenceLocator": f"pkg:cargo/{package['name']}@{package['version']}",
                    "referenceType": "purl",
                }
            ],
            "filesAnalyzed": False,
            "licenseConcluded": normalized_license(package.get("license")),
            "licenseDeclared": normalized_license(package.get("license")),
            "name": package["name"],
            "primaryPackagePurpose": package_purpose(package),
            "sourceInfo": f"Cargo package id: {stable_ids[package['id']]}; manifest: {stable_manifest_path(package, workspace_members)}",
            "supplier": "NOASSERTION",
            "versionInfo": package["version"],
        }
        if checksum and SHA256_RE.fullmatch(checksum):
            spdx_package["checksums"] = [{"algorithm": "SHA256", "checksumValue": checksum}]
        if package.get("homepage"):
            spdx_package["homepage"] = package["homepage"]
        packages.append(spdx_package)

    relationships: list[dict[str, str]] = []
    for member in sorted(workspace_members):
        relationships.append(
            {
                "relatedSpdxElement": spdx_id(stable_ids[member]),
                "relationshipType": "DESCRIBES",
                "spdxElementId": "SPDXRef-DOCUMENT",
            }
        )
    for edge in dependency_edges(metadata):
        relationships.append(
            {
                "relatedSpdxElement": spdx_id(edge["to"]),
                "relationshipType": "DEPENDS_ON",
                "spdxElementId": spdx_id(edge["from"]),
            }
        )
    relationships.sort(key=lambda item: (item["spdxElementId"], item["relationshipType"], item["relatedSpdxElement"]))
    lock_digest = sha256_file(LOCKFILE)
    return {
        "SPDXID": "SPDXRef-DOCUMENT",
        "creationInfo": {
            "comment": "A deterministic timestamp is used so checked-in SBOM drift is content-only.",
            "created": FIXED_SPDX_CREATED,
            "creators": ["Tool: meowengine-supply-chain/1"],
            "licenseListVersion": "3.26",
        },
        "dataLicense": "CC0-1.0",
        "documentNamespace": f"https://meowengine.invalid/spdx/cargo-lock/{lock_digest}",
        "name": "MeowEngine Cargo dependency SBOM",
        "packages": packages,
        "relationships": relationships,
        "spdxVersion": "SPDX-2.3",
    }


def build_v8_report(manifest: dict[str, Any]) -> dict[str, Any]:
    report = copy.deepcopy(manifest)
    report["manifest_path"] = str(V8_MANIFEST.relative_to(ROOT))
    report["manifest_sha256"] = sha256_file(V8_MANIFEST)
    report["validation"] = {"network_used": False, "status": "pass"}
    return report


def generate_reports(destination: Path) -> None:
    manifest = read_json(V8_MANIFEST)
    validate_v8_manifest(manifest)
    metadata = cargo_metadata()
    locked = lock_packages()
    reports = {
        "dependencies.json": build_dependency_report(metadata, locked),
        "licenses.json": build_license_report(metadata, locked),
        "sbom.spdx.json": build_spdx(metadata, locked),
        "v8-provenance.json": build_v8_report(manifest),
    }
    destination.mkdir(parents=True, exist_ok=True)
    for name, value in reports.items():
        write_json(destination / name, value)
    index_entries = []
    for name in sorted(reports):
        path = destination / name
        index_entries.append({"bytes": path.stat().st_size, "path": name, "sha256": sha256_file(path)})
    write_json(
        destination / "manifest.json",
        {
            "cargo_lock_sha256": sha256_file(LOCKFILE),
            "deny_config_sha256": sha256_file(DENY_CONFIG),
            "reports": index_entries,
            "schema_version": 1,
            "v8_manifest_sha256": sha256_file(V8_MANIFEST),
        },
    )


def update_reports() -> None:
    with tempfile.TemporaryDirectory(prefix="meow-supply-chain-") as temporary:
        generated = Path(temporary)
        generate_reports(generated)
        REPORT_DIR.mkdir(parents=True, exist_ok=True)
        for name in REPORT_NAMES:
            shutil.copyfile(generated / name, REPORT_DIR / name)
    print(f"updated deterministic supply-chain reports in {REPORT_DIR.relative_to(ROOT)}")


def report_diff(expected: bytes, actual: bytes, name: str) -> str:
    try:
        expected_text = expected.decode().splitlines()
        actual_text = actual.decode().splitlines()
    except UnicodeDecodeError:
        return f"binary content differs: {name}"
    return "\n".join(
        list(
            difflib.unified_diff(
                actual_text,
                expected_text,
                fromfile=f"checked-in/{name}",
                tofile=f"generated/{name}",
                lineterm="",
            )
        )[:200]
    )


def check_reports() -> None:
    with tempfile.TemporaryDirectory(prefix="meow-supply-chain-check-") as temporary:
        generated = Path(temporary)
        generate_reports(generated)
        failures: list[str] = []
        for name in REPORT_NAMES:
            expected = (generated / name).read_bytes()
            checked_in = REPORT_DIR / name
            if not checked_in.is_file():
                failures.append(f"missing checked-in report: {checked_in.relative_to(ROOT)}")
                continue
            actual = checked_in.read_bytes()
            if actual != expected:
                failures.append(report_diff(expected, actual, name))
        if failures:
            raise PolicyError(
                "supply-chain report drift detected; run `cargo xtask supply-chain update`:\n"
                + "\n\n".join(failures)
            )
    print("supply-chain reports match Cargo.lock, deny.toml, and V8 provenance")


def validate_local_policy() -> None:
    manifest = read_json(V8_MANIFEST)
    validate_v8_manifest(manifest)
    metadata = cargo_metadata()
    require(bool(metadata.get("packages")), "cargo metadata returned no packages")
    require(LOCKFILE.is_file(), "Cargo.lock is required")
    require(DENY_CONFIG.is_file(), "deny.toml is required")
    print(
        f"V8 provenance valid: rusty_v8 {manifest['binding']['version']} / "
        f"V8 {manifest['engine_source']['version']} / {len(manifest['artifacts'])} pinned archives"
    )


def cache_root(manifest: dict[str, Any], override: str | None) -> Path:
    if override:
        return Path(override).expanduser().resolve()
    environment = manifest["cache_policy"]["environment_variable"]
    if os.environ.get(environment):
        return Path(os.environ[environment]).expanduser().resolve()
    return (Path.home() / manifest["cache_policy"]["default_root"]).resolve()


def github_api_json(path: str) -> dict[str, Any]:
    url = f"https://api.github.com{path}"
    headers = {
        "Accept": "application/vnd.github+json",
        "User-Agent": "MeowEngine-V8-Provenance/1",
        "X-GitHub-Api-Version": "2022-11-28",
    }
    token = os.environ.get("GITHUB_TOKEN")
    if token:
        headers["Authorization"] = f"Bearer {token}"
    request = urllib.request.Request(url, headers=headers)
    with urllib.request.urlopen(request, timeout=60) as response:
        value = json.load(response)
    require(isinstance(value, dict), f"unexpected GitHub API response for {path}")
    return value


def resolve_github_tag(repo: str, tag: str) -> str:
    encoded_tag = urllib.parse.quote(tag, safe="")
    reference = github_api_json(f"/repos/{repo}/git/ref/tags/{encoded_tag}")
    target = reference.get("object", {})
    for _ in range(4):
        object_type = target.get("type")
        object_sha = target.get("sha", "")
        require(bool(GIT_REVISION_RE.fullmatch(object_sha)), f"GitHub tag {tag} returned an invalid object SHA")
        if object_type == "commit":
            return object_sha
        require(object_type == "tag", f"GitHub tag {tag} resolved to unsupported object type {object_type!r}")
        target = github_api_json(f"/repos/{repo}/git/tags/{object_sha}").get("object", {})
    raise PolicyError(f"GitHub tag {tag} has an unexpectedly deep annotated-tag chain")


def verify_remote_revisions(manifest: dict[str, Any]) -> None:
    binding = manifest["binding"]
    engine = manifest["engine_source"]
    repo = "denoland/rusty_v8"
    resolved_binding = resolve_github_tag(repo, binding["tag"])
    require(
        resolved_binding == binding["revision"],
        f"rusty_v8 tag drift: {binding['tag']} resolves to {resolved_binding}, expected {binding['revision']}",
    )

    submodule = github_api_json(f"/repos/{repo}/contents/v8?ref={binding['revision']}")
    require(submodule.get("sha") == engine["revision"], "rusty_v8 V8 submodule revision drift")
    require(
        submodule.get("submodule_git_url") == engine["repository"],
        "rusty_v8 V8 submodule repository drift",
    )

    readme_url = f"https://raw.githubusercontent.com/{repo}/{binding['revision']}/README.md"
    request = urllib.request.Request(readme_url, headers={"User-Agent": "MeowEngine-V8-Provenance/1"})
    with urllib.request.urlopen(request, timeout=60) as response:
        readme = response.read().decode("utf-8", "strict")
    version_line = rf"(?im)^V8\s+Version:\s*{re.escape(engine['version'])}\s*$"
    require(
        re.search(version_line, readme) is not None,
        f"rusty_v8 README no longer records V8 version {engine['version']}",
    )
    print(
        f"verified remote revisions: {binding['tag']} -> {binding['revision']}; "
        f"V8 submodule -> {engine['revision']} ({engine['version']})"
    )


def download_and_verify(url: str, expected_sha256: str, expected_size: int | None, destination: Path) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    if destination.is_file():
        actual_sha256 = sha256_file(destination)
        actual_size = destination.stat().st_size
        if actual_sha256 == expected_sha256 and (expected_size is None or actual_size == expected_size):
            print(f"cache hit: {destination} ({actual_sha256})")
            return
        destination.unlink()
        raise PolicyError(f"deleted poisoned cache entry after checksum/size mismatch: {destination}")

    partial = destination.with_name(destination.name + ".partial")
    partial.unlink(missing_ok=True)
    request = urllib.request.Request(url, headers={"User-Agent": "MeowEngine-V8-Provenance/1"})
    digest = hashlib.sha256()
    size = 0
    try:
        with urllib.request.urlopen(request, timeout=120) as response, partial.open("wb") as output:
            while chunk := response.read(1024 * 1024):
                digest.update(chunk)
                size += len(chunk)
                output.write(chunk)
    except Exception:
        partial.unlink(missing_ok=True)
        raise
    actual_sha256 = digest.hexdigest()
    if actual_sha256 != expected_sha256 or (expected_size is not None and size != expected_size):
        partial.unlink(missing_ok=True)
        raise PolicyError(
            f"download verification failed for {url}: sha256={actual_sha256}, bytes={size}; "
            f"expected sha256={expected_sha256}, bytes={expected_size}"
        )
    partial.replace(destination)
    print(f"verified and cached: {destination} ({actual_sha256}, {size} bytes)")


def verify_v8(args: argparse.Namespace) -> None:
    manifest = read_json(V8_MANIFEST)
    validate_v8_manifest(manifest)
    targets = [artifact["target"] for artifact in manifest["artifacts"]]
    requested = targets if args.all_targets else [args.target]
    unknown = sorted(set(requested) - set(targets))
    require(not unknown, f"un-pinned V8 target(s): {', '.join(unknown)}")
    verify_remote_revisions(manifest)
    root = cache_root(manifest, args.cache_dir)
    for artifact in manifest["artifacts"]:
        if artifact["target"] in requested:
            destination = root / artifact["cache_key"]
            download_and_verify(artifact["url"], artifact["sha256"], artifact["size_bytes"], destination)
    evidence_root = root / "license-evidence"
    for item in manifest["license_evidence"]:
        filename = f"{item['component']}-{item['sha256'][:16]}.LICENSE"
        download_and_verify(item["url"], item["sha256"], None, evidence_root / filename)
    print("remote V8 archive and license evidence verification passed")


def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser(description="MeowEngine supply-chain and V8 provenance tooling")
    commands = root.add_subparsers(dest="command", required=True)
    commands.add_parser("validate", help="validate local policy without network access").set_defaults(function=lambda _args: validate_local_policy())
    commands.add_parser("update", help="regenerate checked-in SBOM/license/provenance reports").set_defaults(function=lambda _args: update_reports())
    commands.add_parser("check", help="fail if checked-in reports drift").set_defaults(function=lambda _args: check_reports())
    verify = commands.add_parser("verify-v8", help="explicitly download and verify pinned V8 evidence")
    verify.add_argument("--target", default="x86_64-unknown-linux-gnu")
    verify.add_argument("--all-targets", action="store_true")
    verify.add_argument("--cache-dir")
    verify.set_defaults(function=verify_v8)
    return root


def main() -> int:
    args = parser().parse_args()
    try:
        args.function(args)
    except PolicyError as error:
        print(f"supply-chain policy error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
