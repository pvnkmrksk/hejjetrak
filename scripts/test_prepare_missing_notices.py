"""Packaging-only notice provenance regressions; no network in these fixtures."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import tarfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("notices", Path(__file__).with_name("prepare_missing_notices.py"))
notices = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(notices)


def manifest(name, version, repository="https://github.com/example/repo"):
    return f'[package]\nname = "{name}"\nversion = "{version}"\nlicense = "MIT"\nrepository = "{repository}"\n'


class NoticeTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.cache = self.root / "cargo" / "registry"
        self.registry = self.cache / "src" / "test-index"
        self.staged = self.root / "staged"
        self.registry.mkdir(parents=True)
        self.staged.mkdir()
        self.environment = patch.dict(os.environ, {"CARGO_HOME": str(self.root / "cargo")})
        self.environment.start()
        self.addCleanup(self.environment.stop)

    def package(self, name, version, alias=None):
        source = self.registry / f"{name}-{version}"
        target = self.staged / (alias or name)
        source.mkdir()
        target.mkdir()
        for directory in (source, target):
            (directory / "Cargo.toml").write_text(manifest(name, version))
        return source, target

    def run_helper(self, *extra):
        arguments = ["notices", "--registry", str(self.staged), "--staged", str(self.staged), *extra]
        with patch("sys.argv", arguments):
            notices.main()

    def test_unversioned_staged_name_uses_exact_cache_and_retains_font_terms(self):
        source, target = self.package("epaint_default_fonts", "0.33.3")
        (source / "LICENSE-MIT").write_text("original umbrella notice")
        (source / "fonts").mkdir()
        (source / "fonts" / "OFL.txt").write_text("exact original font terms")
        self.run_helper()
        self.assertEqual((target / "LICENSE-FONT-OFL.txt").read_text(), "exact original font terms")
        metadata = json.loads((target / "LICENSE-METADATA.json").read_text())
        self.assertEqual(metadata["name"], "epaint_default_fonts")
        self.assertIn("LICENSE-FONT-OFL.txt", metadata["notice_files"])
        provenance = json.loads((target / "LICENSE-UPSTREAM-PROVENANCE.json").read_text())
        self.assertTrue(all(item["source"]["kind"] == "registry_package" for item in provenance))
        self.assertEqual((source / "fonts" / "OFL.txt").read_text(), "exact original font terms")

    def test_conflicting_existing_notice_is_never_replaced(self):
        source, target = self.package("fixture", "1.0.0")
        (source / "LICENSE-MIT").write_text("source terms")
        (target / "LICENSE-MIT").write_text("different existing text")
        with self.assertRaisesRegex(RuntimeError, "refusing to replace"):
            self.run_helper()
        self.assertEqual((target / "LICENSE-MIT").read_text(), "different existing text")

    def test_exact_vcs_commit_and_http_repository_are_used_for_missing_notice(self):
        source, target = self.package("fixture", "1.0.0")
        for directory in (source, target):
            (directory / "Cargo.toml").write_text(manifest("fixture", "1.0.0", "http://github.com/example/repo"))
        commit = "a" * 40
        (source / ".cargo_vcs_info.json").write_text(json.dumps({"git": {"sha1": commit}}))
        calls = []
        def fetch(url):
            calls.append(url)
            if "api.github.com" in url:
                return json.dumps({"tree": [{"type": "blob", "path": "LICENSE"}]}).encode()
            return b"exact pinned original notice"
        with patch.object(notices, "request", fetch):
            self.run_helper("--fetch-upstream")
        self.assertEqual(len(calls), 2)
        self.assertTrue(all(commit in url for url in calls))
        self.assertEqual((target / "LICENSE-UPSTREAM" / "LICENSE").read_bytes(), b"exact pinned original notice")
        provenance = json.loads((target / "LICENSE-UPSTREAM-PROVENANCE.json").read_text())
        self.assertEqual(provenance[0]["source"]["commit"], commit)
        self.assertEqual(provenance[0]["sha256"], hashlib.sha256(b"exact pinned original notice").hexdigest())

    def test_changed_staged_provenance_is_rejected(self):
        source, target = self.package("fixture", "1.0.0")
        (source / "LICENSE-MIT").write_text("source terms")
        self.run_helper()
        (target / "LICENSE-MIT").write_text("corrupted staged terms")
        with self.assertRaisesRegex(RuntimeError, "provenance checksum mismatch"):
            self.run_helper()

    def test_original_r_efi_authors_license_is_retained_without_substitute(self):
        source, target = self.package("r-efi", "5.3.0")
        text = "LICENSE: MIT OR Apache-2.0 OR LGPL-2.1-or-later\nAUTHORS-MIT: Permission is hereby granted\nCOPYRIGHT: original upstream holders\n"
        (source / "AUTHORS").write_text(text)
        with patch.object(notices, "request", side_effect=AssertionError("original terms need no network")):
            self.run_helper("--fetch-upstream")
        self.assertEqual((target / "LICENSE-AUTHORS").read_text(), text)
        self.assertFalse((target / "LICENSE-DECLARED-MIT.txt").exists())

    def test_spdx_license_directory_retains_terms_in_archive_collector(self):
        source, target = self.package("fixture", "1.0.0")
        commit = "a" * 40
        (source / ".cargo_vcs_info.json").write_text(json.dumps({"git": {"sha1": commit}}))
        def fetch(url):
            if "api.github.com" in url:
                return b'{"tree":[{"type":"blob","path":"LICENSES/MIT.txt"}]}'
            return b"original SPDX-directory license terms"
        with patch.object(notices, "request", fetch):
            self.run_helper("--fetch-upstream")
        saved = target / "LICENSE-UPSTREAM" / "LICENSES" / "LICENSE-MIT.txt"
        self.assertEqual(saved.read_bytes(), b"original SPDX-directory license terms")
        metadata = json.loads((target / "LICENSE-METADATA.json").read_text())
        self.assertIn(str(saved.relative_to(target)), metadata["notice_files"])

    def block_fixture(self):
        source, target = self.package("block", "0.1.6")
        (source / "Cargo.toml").write_text(manifest("block", "0.1.6") + 'authors = ["Steven Sheldon"]\n')
        for name in ("README.md", "src/lib.rs", "src/test_utils.rs"):
            path = source / name
            path.parent.mkdir(exist_ok=True)
            path.write_text("fixture bytes for " + name)
        archive = self.cache / "cache" / "test-index" / "block-0.1.6.crate"
        archive.parent.mkdir(parents=True)
        with tarfile.open(archive, "w:gz") as stream:
            for path in source.rglob("*"):
                if path.is_file():
                    stream.add(path, arcname="block-0.1.6/" + str(path.relative_to(source)))
        checksum = hashlib.sha256(archive.read_bytes()).hexdigest()
        blobs = {}
        for name in notices.BLOCK_BLOBS:
            data = (source / name).read_bytes()
            blobs[name] = hashlib.sha1(b"blob " + str(len(data)).encode() + b"\0" + data).hexdigest()
        lock = self.root / "Cargo.lock"
        lock.write_text(f'[[package]]\nname = "block"\nversion = "0.1.6"\nchecksum = "{checksum}"\n')
        return source, target, archive, checksum, blobs, lock

    def test_legacy_block_checks_archive_lock_and_each_source_blob(self):
        source, _, archive, checksum, blobs, lock = self.block_fixture()
        original_archive = archive.read_bytes()
        with patch.object(notices, "BLOCK_CRATE_SHA256", checksum), patch.object(notices, "BLOCK_BLOBS", blobs):
            self.assertEqual(notices.verify_block(source, self.cache, lock)["git"]["sha1"], notices.BLOCK_COMMIT)
            archive.write_bytes(b"changed archive")
            with self.assertRaisesRegex(RuntimeError, "crate checksum"):
                notices.verify_block(source, self.cache, lock)
            archive.write_bytes(original_archive)
            (source / "src/lib.rs").write_text("changed source")
            with self.assertRaisesRegex(RuntimeError, "source blob mismatch"):
                notices.verify_block(source, self.cache, lock)
            lock.write_text('[[package]]\nname = "block"\nversion = "0.1.6"\nchecksum = "wrong"\n')
            with self.assertRaisesRegex(RuntimeError, "Cargo.lock checksum"):
                notices.verify_block(source, self.cache, lock)

    def test_declared_block_terms_are_labeled_and_keep_original_authorship(self):
        source, target, _, checksum, blobs, lock = self.block_fixture()
        terms = b"MIT License\nCopyright (c) <year> <copyright holders>\nfixture terms\n"
        def fetch(url):
            if "api.github.com" in url:
                return b'{"tree":[]}'
            return terms
        with patch.object(notices, "BLOCK_CRATE_SHA256", checksum), patch.object(notices, "BLOCK_BLOBS", blobs), patch.object(notices, "MIT_SHA256", hashlib.sha256(terms).hexdigest()), patch.object(notices, "request", fetch):
            self.run_helper("--fetch-upstream", "--lockfile", str(lock))
        self.assertEqual((target / "LICENSE-UPSTREAM-MANIFEST.toml").read_bytes(), (source / "Cargo.toml").read_bytes())
        declared = (target / "LICENSE-DECLARED-MIT.txt").read_text()
        self.assertIn("not an original upstream", declared)
        self.assertIn("Steven Sheldon", declared)
        self.assertIn("<year> <copyright holders>", declared)
        self.assertTrue(declared.endswith(terms.decode()))
        provenance = json.loads((target / "LICENSE-UPSTREAM-PROVENANCE.json").read_text())
        self.assertEqual(provenance[-1]["source"]["kind"], "declared_license_with_versioned_standard_terms")
        self.assertEqual(provenance[-1]["source"]["source_commit"], notices.BLOCK_COMMIT)

    def test_generic_declared_mit_requires_archive_lock_and_exact_vcs(self):
        source, target = self.package("fixture", "1.0.0")
        commit = "a" * 40
        (source / ".cargo_vcs_info.json").write_text(json.dumps({"git": {"sha1": commit}}))
        archive = self.cache / "cache" / "test-index" / "fixture-1.0.0.crate"
        archive.parent.mkdir(parents=True)
        with tarfile.open(archive, "w:gz") as stream:
            for path in source.iterdir():
                stream.add(path, arcname="fixture-1.0.0/" + path.name)
        checksum = hashlib.sha256(archive.read_bytes()).hexdigest()
        lock = self.root / "Cargo.lock"
        lock.write_text(f'[[package]]\nname = "fixture"\nversion = "1.0.0"\nchecksum = "{checksum}"\n')
        terms = b"canonical fixture MIT terms"
        def fetch(url):
            return b'{"tree":[]}' if "api.github.com" in url else terms
        with patch.object(notices, "MIT_SHA256", hashlib.sha256(terms).hexdigest()), patch.object(notices, "request", fetch):
            self.run_helper("--fetch-upstream", "--lockfile", str(lock))
        self.assertTrue((target / "LICENSE-DECLARED-MIT.txt").is_file())
        self.assertEqual(notices.verify_locked_archive(source, self.cache, notices.package_metadata(source / "Cargo.toml"), lock), checksum)
        (source / "Cargo.toml").write_text(manifest("fixture", "1.0.0") + "# altered source\n")
        with self.assertRaisesRegex(RuntimeError, "archive/source mismatch"):
            notices.verify_locked_archive(source, self.cache, notices.package_metadata(source / "Cargo.toml"), lock)

    def test_unknown_missing_vcs_and_non_mit_do_not_receive_invented_notices(self):
        source, target = self.package("fixture", "1.0.0")
        with self.assertRaisesRegex(RuntimeError, "missing exact source revision"):
            self.run_helper("--fetch-upstream")
        self.assertFalse((target / "LICENSE-DECLARED-MIT.txt").exists())
        with self.assertRaisesRegex(RuntimeError, "requires exact MIT"):
            notices.declared_mit(source, lambda *args: self.fail("unexpected notice"), {"name": "fixture", "version": "1.0.0", "license": "Apache-2.0"}, "a" * 40, "b" * 64, "https://github.com/example/repo")

    def test_api_token_is_never_sent_to_raw_http_or_redirected_hosts(self):
        queries = []
        class Response:
            def __enter__(self): return self
            def __exit__(self, *args): return False
            def read(self, limit):
                self.limit = limit
                return b"fixture"
        class Opener:
            def open(self, query, timeout):
                queries.append((query, timeout))
                return Response()
        with patch.dict(os.environ, {"GH_TOKEN": "fixture-secret"}), patch.object(notices.urllib.request, "build_opener", return_value=Opener()):
            for url in ("https://api.github.com/repos/example/repo", "https://raw.githubusercontent.com/example/repo/license", "http://api.github.com/repos/example/repo", "https://api.github.com.attacker.test/license"):
                self.assertEqual(notices.request(url), b"fixture")
        self.assertEqual(queries[0][0].get_header("Authorization"), "Bearer fixture-secret")
        self.assertTrue(all(query.get_header("Authorization") is None for query, _ in queries[1:]))
        self.assertTrue(all(timeout == 30 for _, timeout in queries))
        redirected = notices.SafeRedirect().redirect_request(queries[0][0], None, 302, "redirect", {}, "https://example.com/license")
        self.assertIsNone(redirected.get_header("Authorization"))

    def test_download_limit_is_enforced(self):
        class Response:
            def __enter__(self): return self
            def __exit__(self, *args): return False
            def read(self, limit): return b"x" * limit
        class Opener:
            def open(self, query, timeout): return Response()
        with patch.object(notices, "MAX_RESPONSE_BYTES", 4), patch.object(notices.urllib.request, "build_opener", return_value=Opener()):
            with self.assertRaisesRegex(RuntimeError, "bounded download"):
                notices.request("https://example.com/notice")


if __name__ == "__main__":
    unittest.main()
