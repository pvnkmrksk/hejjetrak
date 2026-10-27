#!/usr/bin/env python3
"""Normalize Debian tar headers to GNU format without extracting or rebuilding files."""
import argparse
import copy
import gzip
import hashlib
import io
import json
import math
import os
from pathlib import Path, PurePosixPath
import re
import tarfile
import tempfile

MAX_DEB_BYTES = 128 * 1024 * 1024
MAX_DATA_TAR_BYTES = 512 * 1024 * 1024
MAX_CONTROL_TAR_BYTES = 8 * 1024 * 1024
MAX_FILE_BYTES = 128 * 1024 * 1024
MAX_MEMBERS = 32768
MAX_PATH_BYTES = 4096
AR_NAMES = ("debian-binary", "control.tar.gz", "data.tar.gz")
PAX_FIELDS = {"path", "linkpath", "mtime", "uid", "gid", "uname", "gname", "size"}
BUILD_PATH = "usr/lib/hejjetrak/BUILD.json"


def digest(data):
    return hashlib.sha256(data).hexdigest()


def read_ar(data):
    if len(data) > MAX_DEB_BYTES or not data.startswith(b"!<arch>\n"):
        raise ValueError("Input must be a bounded Debian ar archive")
    position, members = 8, {}
    for expected in AR_NAMES:
        header = data[position:position + 60]
        if len(header) != 60 or header[58:] != b"`\n":
            raise ValueError("Invalid or truncated Debian ar header")
        try:
            name = header[:16].decode("ascii").strip().removesuffix("/")
            size_text = header[48:58].decode("ascii").strip()
            if not size_text.isdecimal():
                raise ValueError("Invalid ar member length")
            size = int(size_text)
            for field in (header[16:28], header[28:34], header[34:40]):
                if not field.decode("ascii").strip().isdecimal():
                    raise ValueError("Invalid ar numeric metadata")
            int(header[40:48].decode("ascii").strip(), 8)
        except (UnicodeDecodeError, ValueError) as error:
            raise ValueError("Invalid Debian ar metadata") from error
        if name != expected:
            raise ValueError(f"Unexpected Debian ar member {name!r}; expected {expected}")
        position += 60
        if size > MAX_DEB_BYTES or position + size > len(data):
            raise ValueError("Truncated or oversized Debian ar member")
        members[name] = data[position:position + size]
        position += size
        if size % 2:
            if data[position:position + 1] != b"\n":
                raise ValueError("Invalid Debian ar padding")
            position += 1
    if position != len(data) or members["debian-binary"] != b"2.0\n":
        raise ValueError("Unexpected Debian format version or extra ar members")
    return members


def write_ar(members):
    output = io.BytesIO()
    output.write(b"!<arch>\n")
    for name in AR_NAMES:
        data = members[name]
        header = (f'{name + "/":<16}{0:<12}{0:<6}{0:<6}{"100644":<8}{len(data):<10}`\n').encode("ascii")
        if len(header) != 60:
            raise ValueError("Output ar member exceeds format limits")
        output.write(header)
        output.write(data)
        if len(data) % 2:
            output.write(b"\n")
    return output.getvalue()


def normalized_name(name):
    if not isinstance(name, str) or "\0" in name or len(name.encode("utf-8")) > MAX_PATH_BYTES:
        raise ValueError("Invalid or oversized tar path")
    path = PurePosixPath(name)
    if path.is_absolute() or ".." in path.parts:
        raise ValueError(f"Unsafe tar member path: {name!r}")
    return str(path)


def tar_bytes(compressed, maximum):
    if len(compressed) > MAX_DEB_BYTES or compressed[:2] != b"\x1f\x8b":
        raise ValueError("Debian tar members must use bounded gzip streams")
    with gzip.GzipFile(fileobj=io.BytesIO(compressed), mode="rb") as stream:
        data = stream.read(maximum + 1)
    if len(data) > maximum:
        raise ValueError("Uncompressed tar exceeds the byte limit")
    if len(data) % 512 or len(data) < 1024:
        raise ValueError("Invalid tar record length")
    return data


def inspect_tar(compressed, maximum):
    raw = tar_bytes(compressed, maximum)
    records, contents, infos, pax_counts, fractions = {}, {}, [], {}, []
    with tarfile.open(fileobj=io.BytesIO(raw), mode="r:", errorlevel=2) as archive:
        for info in archive:
            if len(infos) >= MAX_MEMBERS:
                raise ValueError("Tar member count exceeds the limit")
            name = normalized_name(info.name)
            if name in records:
                raise ValueError(f"Duplicate tar member: {name}")
            if not (info.isfile() or info.isdir() or info.issym() or info.islnk()):
                raise ValueError(f"Unsupported tar member type: {name}")
            if getattr(info, "sparse", None) is not None:
                raise ValueError("Sparse tar members are unsupported")
            if set(info.pax_headers) - PAX_FIELDS:
                raise ValueError(f"Unsupported extended tar metadata: {name}")
            for key in info.pax_headers:
                pax_counts[key] = pax_counts.get(key, 0) + 1
            if not isinstance(info.size, int) or not 0 <= info.size <= MAX_FILE_BYTES:
                raise ValueError("Tar file size exceeds the limit")
            if not info.isfile() and info.size != 0:
                raise ValueError("Non-file tar member has a payload")
            for owner in (info.uid, info.gid):
                if not isinstance(owner, int) or not 0 <= owner < 2 ** 63:
                    raise ValueError("Tar ownership is not representable in GNU format")
            if not 0 <= info.mode <= 0o7777:
                raise ValueError("Invalid tar permissions")
            for owner_name in (info.uname, info.gname):
                if "\0" in owner_name or len(owner_name.encode("utf-8")) > 32:
                    raise ValueError("Tar ownership names cannot be preserved in GNU format")
            if not isinstance(info.mtime, (int, float)) or not math.isfinite(info.mtime) or abs(info.mtime) >= 2 ** 63:
                raise ValueError("Invalid tar modification time")
            mtime = math.floor(info.mtime)
            if mtime != info.mtime:
                fractions.append({"path": name, "input_mtime": info.mtime, "output_mtime": mtime})
            if "\0" in info.linkname or len(info.linkname.encode("utf-8")) > MAX_PATH_BYTES:
                raise ValueError("Invalid tar link target")
            if info.islnk():
                normalized_name(info.linkname)
            content = b""
            if info.isfile():
                source = archive.extractfile(info)
                if source is None:
                    raise ValueError("Missing regular-file payload")
                content = source.read(info.size + 1)
                if len(content) != info.size:
                    raise ValueError("Truncated regular-file payload")
                contents[name] = content
            kind = "file" if info.isfile() else "directory" if info.isdir() else "symlink" if info.issym() else "hardlink"
            records[name] = {"type": kind, "mode": info.mode, "uid": info.uid, "gid": info.gid,
                             "uname": info.uname, "gname": info.gname, "linkname": info.linkname,
                             "size": info.size, "mtime": mtime, "content_sha256": digest(content) if info.isfile() else None}
            infos.append(info)
        # Anything after the tar end marker must be ordinary zero padding.
        if any(raw[archive.offset:]):
            raise ValueError("Unexpected bytes after the tar end marker")
    return records, contents, infos, pax_counts, fractions


def normalize_tar(compressed, maximum):
    before, contents, infos, pax, fractions = inspect_tar(compressed, maximum)
    output = io.BytesIO()
    with gzip.GzipFile(filename="", fileobj=output, mode="wb", mtime=0) as compressed_output:
        with tarfile.open(fileobj=compressed_output, mode="w|", format=tarfile.GNU_FORMAT) as archive:
            for original in infos:
                info = copy.copy(original)
                info.pax_headers = {}
                info.mtime = math.floor(info.mtime)
                archive.addfile(info, io.BytesIO(contents[normalized_name(info.name)]) if info.isfile() else None)
    result = output.getvalue()
    after, _, _, output_pax, output_fractions = inspect_tar(result, maximum)
    if before != after or output_pax or output_fractions:
        raise ValueError("GNU conversion changed package payload or preserved unsupported tar headers")
    manifest = json.dumps(before, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode("utf-8")
    report = {"members": len(before), "regular_files": len(contents),
              "directories": sum(x["type"] == "directory" for x in before.values()),
              "symlinks": sum(x["type"] == "symlink" for x in before.values()),
              "hardlinks": sum(x["type"] == "hardlink" for x in before.values()),
              "file_content_bytes": sum(len(x) for x in contents.values()),
              "input_pax_fields": pax, "output_pax_fields": output_pax,
              "fractional_mtime_normalizations": fractions,
              "input_payload_manifest_sha256": digest(manifest),
              "output_payload_manifest_sha256": digest(manifest),
              "all_file_bytes_permissions_ownership_links_equal": True}
    return result, contents, before, report


def convert(data, source_commit, version, tool_revision="uncommitted"):
    if not re.fullmatch(r"[0-9a-f]{40}", source_commit):
        raise ValueError("Expected source commit must be a full lowercase Git SHA")
    if not re.fullmatch(r"[0-9][0-9A-Za-z.+:~_-]{0,63}", version):
        raise ValueError("Expected version is invalid")
    if tool_revision != "uncommitted" and not re.fullmatch(r"[0-9a-f]{40}", tool_revision):
        raise ValueError("Packaging tool revision must be a full Git SHA or uncommitted")
    members = read_ar(data)
    control, control_files, _, control_report = normalize_tar(members["control.tar.gz"], MAX_CONTROL_TAR_BYTES)
    payload, files, records, data_report = normalize_tar(members["data.tar.gz"], MAX_DATA_TAR_BYTES)
    if BUILD_PATH not in files or len(files[BUILD_PATH]) > 1024 * 1024:
        raise ValueError("Package has no bounded application BUILD.json")
    build = json.loads(files[BUILD_PATH].decode("utf-8"))
    if build.get("source_commit") != source_commit or build.get("version") != version:
        raise ValueError("Application BUILD.json source commit or version does not match")
    if build.get("platform") != "linux":
        raise ValueError("BUILD.json does not describe a Linux package")
    control_text = control_files.get("control", b"").decode("utf-8")
    versions = re.findall(r"(?m)^Version: ([^\r\n]+)$", control_text)
    if versions != [version]:
        raise ValueError("Debian control version does not match the application version")
    checked_binaries = []
    for binary in build.get("binaries", []):
        name = binary["name"]
        if "/" in name or "\\" in name:
            raise ValueError("Invalid BUILD.json binary name")
        path = "usr/lib/hejjetrak/" + name
        if path not in records or records[path]["content_sha256"] != binary["compiled_sha256"]:
            raise ValueError("Packaged binary does not match BUILD.json")
        checked_binaries.append(name)
    output = write_ar({"debian-binary": b"2.0\n", "control.tar.gz": control, "data.tar.gz": payload})
    if len(output) > MAX_DEB_BYTES:
        raise ValueError("Corrected package exceeds the input/output byte limit")
    report = {"schema_version": 1, "application_source_commit": source_commit, "version": version,
              "packaging_tool_revision": tool_revision,
              "packaging_tool_sha256": digest(Path(__file__).read_bytes()),
              "input_sha256": digest(data), "output_sha256": digest(output),
              "input_bytes": len(data), "output_bytes": len(output),
              "input_format": "Debian ar with gzip tar streams", "output_tar_format": "GNU",
              "deterministic_gzip_mtime": 0, "deterministic_ar_metadata": True,
              "control": control_report, "data": data_report,
              "verified_build_binary_hashes": checked_binaries,
              "application_payload_unchanged": True,
              "scope": "Tar header/format normalization only; no executable, config, notice or documentation bytes rebuilt or edited."}
    return output, report


def publish(output, report_path, data, report):
    if output == report_path or output.exists() or report_path.exists():
        raise FileExistsError("Refusing to overwrite an existing package/report")
    staged, published = [], []
    try:
        for destination, content in ((output, data), (report_path, (json.dumps(report, indent=2, ensure_ascii=False) + "\n").encode("utf-8"))):
            destination.parent.mkdir(parents=True, exist_ok=True)
            with tempfile.NamedTemporaryFile(prefix=".repack-deb-", dir=destination.parent, delete=False) as stream:
                temporary = Path(stream.name)
                staged.append((temporary, destination))
                stream.write(content)
                stream.flush()
                os.fsync(stream.fileno())
            temporary.chmod(0o644)
        for temporary, destination in staged:
            os.link(temporary, destination)
            published.append((temporary, destination))
    except BaseException:
        for temporary, destination in published:
            if destination.exists() and destination.samefile(temporary):
                destination.unlink()
        raise
    finally:
        for temporary, _ in staged:
            temporary.unlink(missing_ok=True)


def run(input_path, output_path, report_path, source_commit, version, tool_revision="uncommitted"):
    if output_path.exists() or report_path.exists():
        raise FileExistsError("Refusing to overwrite an existing package/report")
    with input_path.open("rb") as stream:
        data = stream.read(MAX_DEB_BYTES + 1)
    corrected, report = convert(data, source_commit, version, tool_revision)
    publish(output_path, report_path, corrected, report)
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--source-commit", required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--tool-revision", default="uncommitted")
    args = parser.parse_args()
    report = run(args.input, args.output, args.report, args.source_commit, args.version, args.tool_revision)
    print(json.dumps({"output_sha256": report["output_sha256"], "regular_files": report["data"]["regular_files"], "application_payload_unchanged": True}))


if __name__ == "__main__":
    main()
