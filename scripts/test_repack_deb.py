"""Debian header-normalization fixtures; no extraction or external programs."""
import gzip
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("repack", Path(__file__).with_name("repack_deb.py"))
repack = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(repack)
SOURCE = "99d4f5711584dd211f8b8c16e52bbbd357119a2f"
VERSION = "0.3.1"


def archive(entries):
    raw = io.BytesIO()
    with tarfile.open(fileobj=raw, mode="w", format=tarfile.PAX_FORMAT) as stream:
        for name, data, kind in entries:
            info = tarfile.TarInfo(name)
            info.uid, info.gid, info.uname, info.gname = 123, 456, "original-user", "original-group"
            info.mode, info.mtime = 0o751, 1234567890.75
            if kind == "symlink":
                info.type, info.linkname, info.mode = tarfile.SYMTYPE, data, 0o777
                stream.addfile(info)
            elif kind == "directory":
                info.type, info.mode = tarfile.DIRTYPE, 0o2755
                stream.addfile(info)
            elif kind == "device":
                info.type = tarfile.CHRTYPE
                stream.addfile(info)
            else:
                info.size = len(data)
                stream.addfile(info, io.BytesIO(data))
    return gzip.compress(raw.getvalue(), mtime=123)


def fixture(source=SOURCE, version=VERSION, control_version=VERSION):
    binary = b"original compiled executable bytes\x00\xff"
    build = {"source_commit": source, "version": version, "platform": "linux",
             "binaries": [{"name": "hejjetrac", "compiled_sha256": hashlib.sha256(binary).hexdigest()}]}
    long_name = "usr/lib/hejjetrak/DependencyLicenses/" + "long-dependency-name-" * 12 + "/LICENSE-original.txt"
    members = {"debian-binary": b"2.0\n",
               "control.tar.gz": archive([("control", f"Package: hejjetrak\nVersion: {control_version}\n".encode(), "file")]),
               "data.tar.gz": archive([
                   ("usr", b"", "directory"),
                   ("usr/lib/hejjetrak/BUILD.json", json.dumps(build).encode(), "file"),
                   ("usr/lib/hejjetrak/hejjetrac", binary, "file"),
                   (long_name, b"full original notice\nCopyright original author\n", "file"),
                   ("usr/bin/hejjetrac", "../lib/hejjetrak/hejjetrac", "symlink"),
               ])}
    return repack.write_ar(members), long_name


class RepackTests(unittest.TestCase):
    def test_pax_long_names_metadata_and_symlinks_survive_gnu_rewrite(self):
        original, long_name = fixture()
        converted, report = repack.convert(original, SOURCE, VERSION)
        self.assertTrue(report["application_payload_unchanged"])
        self.assertEqual(report["data"]["regular_files"], 3)
        self.assertEqual(report["data"]["symlinks"], 1)
        self.assertTrue(report["data"]["input_pax_fields"]["path"])
        self.assertFalse(report["data"]["output_pax_fields"])
        self.assertEqual(report["data"]["input_payload_manifest_sha256"], report["data"]["output_payload_manifest_sha256"])
        self.assertEqual(report["verified_build_binary_hashes"], ["hejjetrac"])
        members = repack.read_ar(converted)
        for name in ("control.tar.gz", "data.tar.gz"):
            self.assertEqual(members[name][4:8], b"\0\0\0\0")
        records, contents, _, pax, fractions = repack.inspect_tar(members["data.tar.gz"], repack.MAX_DATA_TAR_BYTES)
        self.assertIn(long_name, contents)
        self.assertEqual(records["usr/bin/hejjetrac"]["linkname"], "../lib/hejjetrak/hejjetrac")
        self.assertEqual(records[long_name]["mode"], 0o751)
        self.assertEqual(records[long_name]["uid"], 123)
        self.assertEqual(records[long_name]["gid"], 456)
        self.assertEqual(records[long_name]["uname"], "original-user")
        self.assertEqual(records["usr"]["mode"], 0o2755)
        self.assertFalse(pax or fractions)
        self.assertEqual(converted, repack.convert(original, SOURCE, VERSION)[0])
        self.assertEqual(converted, repack.convert(converted, SOURCE, VERSION)[0])

    def test_source_version_control_and_binary_tampering_are_rejected_atomically(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for index, data in enumerate((fixture(source="a" * 40)[0], fixture(version="9.0.0")[0], fixture(control_version="9.0.0")[0])):
                source, output, report = root / f"input-{index}.deb", root / f"output-{index}.deb", root / f"report-{index}.json"
                source.write_bytes(data)
                with self.assertRaisesRegex(ValueError, "does not match"):
                    repack.run(source, output, report, SOURCE, VERSION)
                self.assertFalse(output.exists() or report.exists())
            data, _ = fixture()
            members = repack.read_ar(data)
            raw = gzip.decompress(members["data.tar.gz"]).replace(b"original compiled executable bytes", b"tampered compiled executable bytes")
            members["data.tar.gz"] = gzip.compress(raw)
            with self.assertRaises(ValueError):
                repack.convert(repack.write_ar(members), SOURCE, VERSION)

    def test_existing_output_or_report_is_never_overwritten(self):
        data, _ = fixture()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, output, report = root / "input.deb", root / "output.deb", root / "report.json"
            source.write_bytes(data)
            output.write_bytes(b"preserved package")
            with self.assertRaises(FileExistsError):
                repack.run(source, output, report, SOURCE, VERSION)
            self.assertEqual(output.read_bytes(), b"preserved package")
            self.assertFalse(report.exists())
            output.unlink()
            report.write_bytes(b"preserved report")
            with self.assertRaises(FileExistsError):
                repack.run(source, output, report, SOURCE, VERSION)
            self.assertEqual(report.read_bytes(), b"preserved report")
            self.assertFalse(output.exists())
            report.unlink()
            result = repack.run(source, output, report, SOURCE, VERSION)
            self.assertEqual(result, json.loads(report.read_text(encoding="utf-8")))
            self.assertEqual(repack.digest(output.read_bytes()), result["output_sha256"])
            self.assertFalse(list(root.glob(".repack-deb-*")))

    def test_unexpected_ar_tar_members_and_bounds_are_rejected(self):
        data, _ = fixture()
        with self.assertRaises(ValueError):
            repack.convert(data + b"unexpected member", SOURCE, VERSION)
        with self.assertRaises(ValueError):
            repack.convert(data.replace(b"debian-binary/", b"wrong-member/", 1), SOURCE, VERSION)
        members = repack.read_ar(data)
        members["data.tar.gz"] = archive([("unsafe-device", b"", "device")])
        with self.assertRaisesRegex(ValueError, "Unsupported tar member type"):
            repack.convert(repack.write_ar(members), SOURCE, VERSION)
        members["data.tar.gz"] = archive([("../escape", b"data", "file")])
        with self.assertRaisesRegex(ValueError, "Unsafe tar"):
            repack.convert(repack.write_ar(members), SOURCE, VERSION)
        with patch.object(repack, "MAX_DATA_TAR_BYTES", 1024):
            with self.assertRaisesRegex(ValueError, "Uncompressed tar"):
                repack.convert(data, SOURCE, VERSION)
        with patch.object(repack, "MAX_MEMBERS", 1):
            with self.assertRaisesRegex(ValueError, "member count"):
                repack.convert(data, SOURCE, VERSION)
        with patch.object(repack, "MAX_DEB_BYTES", 10):
            with self.assertRaisesRegex(ValueError, "bounded Debian"):
                repack.convert(data, SOURCE, VERSION)

    def test_unknown_pax_extensions_fail_instead_of_discarding_metadata(self):
        raw = io.BytesIO()
        with tarfile.open(fileobj=raw, mode="w", format=tarfile.PAX_FORMAT) as stream:
            info = tarfile.TarInfo("file")
            info.pax_headers = {"SCHILY.xattr.user.example": "retain me"}
            stream.addfile(info)
        with self.assertRaisesRegex(ValueError, "Unsupported extended"):
            repack.normalize_tar(gzip.compress(raw.getvalue()), repack.MAX_DATA_TAR_BYTES)


if __name__ == "__main__":
    unittest.main()
