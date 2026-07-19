from __future__ import annotations

import copy
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name("supply_chain.py")
SPEC = importlib.util.spec_from_file_location("meow_supply_chain", SCRIPT)
assert SPEC and SPEC.loader
supply_chain = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(supply_chain)


class SupplyChainPolicyTests(unittest.TestCase):
    def setUp(self) -> None:
        self.manifest = supply_chain.read_json(supply_chain.V8_MANIFEST)

    def test_checked_in_v8_manifest_is_valid(self) -> None:
        supply_chain.validate_v8_manifest(self.manifest)

    def test_cache_key_drift_is_rejected(self) -> None:
        manifest = copy.deepcopy(self.manifest)
        manifest["artifacts"][0]["cache_key"] = "mutable/latest/archive.gz"
        with self.assertRaisesRegex(supply_chain.PolicyError, "cache key drift"):
            supply_chain.validate_v8_manifest(manifest)

    def test_mutable_release_url_is_rejected(self) -> None:
        manifest = copy.deepcopy(self.manifest)
        manifest["artifacts"][0]["url"] = (
            "https://github.com/denoland/rusty_v8/releases/latest/download/"
            + manifest["artifacts"][0]["filename"]
        )
        with self.assertRaises(supply_chain.PolicyError):
            supply_chain.validate_v8_manifest(manifest)

    def test_license_aliases_are_normalized(self) -> None:
        self.assertEqual(supply_chain.normalized_license("MIT/Apache-2.0"), "MIT OR Apache-2.0")
        self.assertEqual(supply_chain.normalized_license(None), "NOASSERTION")

    def test_report_generation_is_deterministic_and_portable(self) -> None:
        with tempfile.TemporaryDirectory() as first, tempfile.TemporaryDirectory() as second:
            first_path = Path(first)
            second_path = Path(second)
            supply_chain.generate_reports(first_path)
            supply_chain.generate_reports(second_path)
            for name in supply_chain.REPORT_NAMES:
                self.assertEqual((first_path / name).read_bytes(), (second_path / name).read_bytes(), name)
            dependencies = json.loads((first_path / "dependencies.json").read_text())
            encoded = json.dumps(dependencies)
            self.assertNotIn(str(Path.home()), encoded)
            self.assertNotIn("file:///", encoded)

    def test_poisoned_cache_entry_is_deleted(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary) / "archive.gz"
            destination.write_bytes(b"not-the-pinned-archive")
            with self.assertRaisesRegex(supply_chain.PolicyError, "poisoned cache"):
                supply_chain.download_and_verify(
                    "https://example.invalid/archive.gz",
                    "0" * 64,
                    100,
                    destination,
                )
            self.assertFalse(destination.exists())


if __name__ == "__main__":
    unittest.main()
