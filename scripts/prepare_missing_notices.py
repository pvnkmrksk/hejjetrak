"""Augment locally staged dependency notices from registry files/exact Git commits.

Build-time packaging helper only. Existing staged LICENSE-METADATA.json records
identify packages. Local packaged notices are preferred. --fetch-upstream fills
missing repository-root notices at .cargo_vcs_info.json's exact commit, with
source URLs and SHA256 provenance. It never substitutes a floating branch.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import tarfile
import urllib.request
from urllib.parse import urlsplit


def notice(name):
    return name.lower().startswith(("license", "licence", "copying", "notice", "copyright"))


MAX_RESPONSE_BYTES = 16 * 1024 * 1024
BLOCK_CRATE_SHA256 = "0d8c1fef690941d3e7788d328517591fecc684c084084702d6ff1641e993699a"
BLOCK_COMMIT = "47178790cfc9d4a8b092051d8b413b78bd31254a"
BLOCK_BLOBS = {
    "Cargo.toml": "f414e7ef2458b35fd72cebc3014ab4a977808aa3",
    "README.md": "78c65d087e262592d2ac9e10868bbbab4b046afc",
    "src/lib.rs": "b9c5d1d77479727e0c5fa3ddbe018e060684f8f9",
    "src/test_utils.rs": "940a3d22720618ca42752f890a00fd2b9ce8d2ee",
}
MIT_COMMIT = "279b7e997a87c09be00dd6bdda189519ade502cf"
MIT_SHA256 = "b05785f9f18e6716bab63424b11454513b9943a222595b70411009202fc592b5"


def github_api(url):
    parsed = urlsplit(url)
    return parsed.scheme == "https" and parsed.netloc.lower() == "api.github.com"


class SafeRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        redirected = super().redirect_request(req, fp, code, msg, headers, newurl)
        if redirected is not None and not github_api(newurl):
            redirected.remove_header("Authorization")
        return redirected


def request(url):
    headers = {"User-Agent": "Hejjetrac-license-audit"}
    token = os.environ.get("GH_TOKEN")
    if token and github_api(url):
        headers["Authorization"] = "Bearer " + token
    query = urllib.request.Request(url, headers=headers)
    opener = urllib.request.build_opener(SafeRedirect())
    with opener.open(query, timeout=30) as response:
        data = response.read(MAX_RESPONSE_BYTES + 1)
    if len(data) > MAX_RESPONSE_BYTES:
        raise RuntimeError("upstream notice response exceeds the bounded download limit")
    return data


def package_metadata(path):
    # Cargo's normalized package manifests use JSON-compatible quoted scalars.
    # Avoid a Python3.11-only TOML dependency on Ubuntu22.04's Python3.10.
    text = path.read_text()
    section = re.search(r"(?ms)^\[package\]\s*\n(.*?)(?=^\[|\Z)", text)
    if not section:
        raise RuntimeError(f"no package metadata in {path}")

    def scalar(key):
        match = re.search(rf'(?m)^{re.escape(key)}\s*=\s*("(?:[^"\\]|\\.)*")', section[1])
        return json.loads(match[1]) if match else None

    return {"name": scalar("name"), "version": scalar("version"),
            "license": scalar("license"), "repository": scalar("repository"),
            "source": "registry+https://github.com/rust-lang/crates.io-index", "notice_files": []}



def registry_source(registry, staged, metadata):
    """Resolve Cargo's directory aliases through the actual package identity."""
    identity = f"{metadata['name']}-{metadata['version']}"
    cache = Path(os.environ.get("CARGO_HOME", str(Path.home() / ".cargo"))) / "registry"
    caches = sorted((cache / "src").glob("*"))
    roots = caches + [registry] if registry.resolve() == staged.resolve() else [registry] + caches
    for root in roots:
        for candidate in (root / identity, root / metadata["name"]):
            manifest = candidate / "Cargo.toml"
            if not manifest.is_file():
                continue
            actual = package_metadata(manifest)
            if (actual["name"], actual["version"]) == (metadata["name"], metadata["version"]):
                return candidate, cache
    raise RuntimeError(f"no exact local registry source for {identity}")


def verify_block(source, cache, lockfile):
    """Old block predates Cargo VCS metadata; verify every published source blob."""
    locked = lockfile.read_text()
    blocks = re.split(r"(?m)^\[\[package\]\]\s*$", locked)
    expected = next((part for part in blocks
                     if re.search(r'(?m)^name = "block"$', part)
                     and re.search(r'(?m)^version = "0\.1\.6"$', part)), "")
    if f'checksum = "{BLOCK_CRATE_SHA256}"' not in expected:
        raise RuntimeError("block 0.1.6 does not match its pinned Cargo.lock checksum")
    archives = sorted((cache / "cache").glob("*/block-0.1.6.crate"))
    if not archives or not any(hashlib.sha256(path.read_bytes()).hexdigest() == BLOCK_CRATE_SHA256 for path in archives):
        raise RuntimeError("block 0.1.6 cached crate checksum does not match the exact published archive")
    for relative, expected in BLOCK_BLOBS.items():
        data = (source / relative).read_bytes()
        actual = hashlib.sha1(b"blob " + str(len(data)).encode() + b"\0" + data).hexdigest()
        if actual != expected:
            raise RuntimeError(f"block 0.1.6 source blob mismatch: {relative}")
    return {"git": {"sha1": BLOCK_COMMIT}, "path_in_vcs": ""}


def verify_locked_archive(source, cache, metadata, lockfile):
    identity = (metadata["name"], metadata["version"])
    parts = re.split(r"(?m)^\[\[package\]\]\s*$", lockfile.read_text())
    selected = next((part for part in parts
                     if re.search(rf'(?m)^name = "{re.escape(identity[0])}"$', part)
                     and re.search(rf'(?m)^version = "{re.escape(identity[1])}"$', part)), "")
    checksum = re.search(r'(?m)^checksum = "([0-9a-f]{64})"$', selected)
    if not checksum:
        raise RuntimeError(f"missing locked registry checksum for {identity}")
    checksum = checksum[1]
    archives = sorted((cache / "cache").glob(f"*/{identity[0]}-{identity[1]}.crate"))
    archive = next((path for path in archives if hashlib.sha256(path.read_bytes()).hexdigest() == checksum), None)
    if archive is None:
        raise RuntimeError(f"cached crate checksum mismatch for {identity}")
    total = 0
    with tarfile.open(archive) as stream:
        for member in stream:
            if not member.isfile():
                continue
            relative = PurePosixPath(member.name).relative_to(f"{identity[0]}-{identity[1]}")
            if relative.is_absolute() or ".." in relative.parts or member.size > MAX_RESPONSE_BYTES:
                raise RuntimeError(f"invalid archive member for {identity}")
            total += member.size
            if total > MAX_RESPONSE_BYTES:
                raise RuntimeError(f"legacy notice source exceeds the byte bound for {identity}")
            content = stream.extractfile(member).read(MAX_RESPONSE_BYTES + 1)
            if (source / Path(relative)).read_bytes() != content:
                raise RuntimeError(f"published archive/source mismatch for {identity}: {relative}")
    return checksum


def legacy_malloc(source, cache, lockfile):
    # The published manifest adds only exclude=[".gitignore"] after tag 0.0.6.
    # Pin its actual archive/manifest, not a claim of byte-identical tag metadata.
    metadata = package_metadata(source / "Cargo.toml")
    checksum = verify_locked_archive(source, cache, metadata, lockfile)
    if checksum != "62bb907fe88d54d8d9ce32a3cceab4218ed2f6b7d35617cafe9adf84e43919cb":
        raise RuntimeError("malloc_buf 0.0.6 differs from its pinned published archive")
    for relative, expected in {
        "Cargo.toml": "cfb0c082d87488dc110624a324636deeac8e2677",
        "src/lib.rs": "24066b69d418d50a507fb91c0db3d4f607590680",
    }.items():
        data = (source / relative).read_bytes()
        if hashlib.sha1(b"blob " + str(len(data)).encode() + b"\0" + data).hexdigest() != expected:
            raise RuntimeError(f"malloc_buf published source blob mismatch: {relative}")
    return {"git": {"sha1": "a7811e5f4c6f9685dd57ad9bfe34e9bf6b0ba4f9"}, "path_in_vcs": ""}


def declared_mit(source, save, metadata, commit, checksum, repository):
    if metadata.get("license") != "MIT":
        raise RuntimeError(f"no original notice for {metadata['name']}; declared-license supplement requires exact MIT")
    url = f"https://raw.githubusercontent.com/spdx/license-list-data/{MIT_COMMIT}/text/MIT.txt"
    terms = request(url)
    if hashlib.sha256(terms).hexdigest() != MIT_SHA256:
        raise RuntimeError("canonical SPDX MIT text checksum mismatch")
    manifest = (source / "Cargo.toml").read_bytes()
    save(Path("LICENSE-UPSTREAM-MANIFEST.toml"), manifest,
         {"kind": "exact_published_crate", "crate_sha256": checksum,
          "repository": repository, "commit": commit, "file": "Cargo.toml"})
    explanation = (
        f"Declared license for {metadata['name']} {metadata['version']}\n\n"
        "The exact published Cargo.toml retained below declares MIT and identifies\n"
        "the package authors. The matching upstream source has no standalone\n"
        "license notice. This supplementary file is not an original upstream\n"
        "LICENSE. No copyright year or holder statement has been invented.\n\n"
        "Original published package manifest (also saved separately):\n\n"
    ).encode() + manifest + (
        "\nThe following is the unchanged canonical SPDX MIT template; its angle-\n"
        "bracket placeholders are SPDX placeholders, not supplied copyright facts.\n\n"
    ).encode()
    save(Path("LICENSE-DECLARED-MIT.txt"), explanation + terms,
         {"kind": "declared_license_with_versioned_standard_terms", "url": url,
          "standard_text_sha256": MIT_SHA256, "source_commit": commit,
          "crate_sha256": checksum})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--registry", type=Path, required=True)
    parser.add_argument("--staged", type=Path, default=Path("third-party-source"))
    parser.add_argument("--fetch-upstream", action="store_true")
    parser.add_argument("--lockfile", type=Path, default=Path("Cargo.lock"))
    args = parser.parse_args()
    trees, downloads, results = {}, {}, []
    for target in sorted(args.staged.iterdir()):
        if not target.is_dir():
            continue
        metadata_path = target / "LICENSE-METADATA.json"
        if metadata_path.is_file():
            metadata = json.loads(metadata_path.read_text())
        elif (target / "Cargo.toml").is_file():
            metadata = package_metadata(target / "Cargo.toml")
        else:
            continue
        if not metadata.get("name") or not metadata.get("version"):
            raise RuntimeError(f"staged package has no exact name/version: {target}")
        source, cache = registry_source(args.registry, args.staged, metadata)
        identity = (metadata["name"], metadata["version"])
        legacy_vcs = None
        if identity == ("block", "0.1.6"):
            legacy_vcs = verify_block(source, cache, args.lockfile)
        elif identity == ("malloc_buf", "0.0.6"):
            legacy_vcs = legacy_malloc(source, cache, args.lockfile)
        provenance_path = target / "LICENSE-UPSTREAM-PROVENANCE.json"
        provenance = json.loads(provenance_path.read_text()) if provenance_path.exists() else []
        for entry in provenance:
            relative = Path(entry["notice_file"])
            if relative.is_absolute() or ".." in relative.parts:
                raise RuntimeError(f"invalid notice provenance path in {target.name}")
            saved = target / relative
            if not saved.is_file() or hashlib.sha256(saved.read_bytes()).hexdigest() != entry["sha256"]:
                raise RuntimeError(f"notice provenance checksum mismatch: {saved}")

        def save(relative, content, origin):
            destination = target / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            if destination.exists() and destination.read_bytes() != content:
                raise RuntimeError(f"refusing to replace different notice text: {destination}")
            destination.write_bytes(content)
            entry = {"notice_file": str(relative), "source": origin,
                     "sha256": hashlib.sha256(content).hexdigest()}
            if entry not in provenance:
                provenance.append(entry)

        for path in source.rglob("*"):
            if not path.is_file():
                continue
            if path.name in ("LICENSE-METADATA.json", "LICENSE-UPSTREAM-PROVENANCE.json", "LICENSE-AUDIT.json"):
                continue
            relative = path.relative_to(source)
            # egui's packaged font licenses use names such as OFL.txt, UFL.txt,
            # Hack-Regular.txt and emoji-icon-font-mit-license.txt.
            font_notice = metadata["name"] == "epaint_default_fonts" and relative.parts[0] == "fonts" and path.suffix == ".txt"
            authors_notice = metadata["name"] == "r-efi" and path.name == "AUTHORS"
            if notice(path.name) or font_notice or authors_notice:
                # Prefix supplementary packaged notices so the existing archive
                # collector retains them regardless of their upstream filename.
                saved = relative if notice(path.name) else Path(
                    ("LICENSE-FONT-" if font_notice else "LICENSE-") + path.name
                )
                save(saved, path.read_bytes(), {"kind": "registry_package", "file": str(path)})
        umbrella = [p for p in target.iterdir() if p.is_file() and notice(p.name)
                    and p.name not in ("LICENSE-METADATA.json", "LICENSE-UPSTREAM-PROVENANCE.json")
                    and not p.name.startswith("LICENSE-FONT-")]
        umbrella.extend(p for p in (target / "LICENSE-UPSTREAM").rglob("*") if p.is_file())
        if not umbrella and args.fetch_upstream:
            vcs_path = source / ".cargo_vcs_info.json"
            if vcs_path.is_file():
                vcs = json.loads(vcs_path.read_text())
            elif legacy_vcs is not None:
                vcs = legacy_vcs
            else:
                raise RuntimeError(f"missing exact source revision for {metadata['name']} {metadata['version']}")
            commit = vcs.get("git", {}).get("sha1", "")
            if not re.fullmatch(r"[0-9a-f]{40}", commit):
                raise RuntimeError(f"invalid exact source commit for {target.name}")
            repository = metadata.get("repository")
            if identity == ("v4l2-sys-mit", "0.3.0") and repository is None:
                # Published subcrate omits repository. The exact Cargo VCS commit
                # and v4l2-sys directory exist in the author's libv4l-rs repo.
                repository = "https://github.com/raymanfx/libv4l-rs"
            match = re.fullmatch(r"https?://github.com/([^/]+/[^/#?]+)(?:/[^?#]*)?", repository or "")
            if not match:
                raise RuntimeError(f"no supported exact GitHub source for {target.name}")
            repo = match[1].removesuffix(".git")
            repository = f"https://github.com/{repo}"
            key = (repo, commit)
            if key not in trees:
                url = f"https://api.github.com/repos/{repo}/git/trees/{commit}?recursive=1"
                trees[key] = json.loads(request(url))
            tree = trees[key]
            if tree.get("truncated"):
                raise RuntimeError(f"incomplete source tree for {repo}@{commit}")
            crate_path = PurePosixPath(vcs.get("path_in_vcs", ""))
            ancestors = {PurePosixPath("."), crate_path, *crate_path.parents}
            def upstream_notice(path):
                path = PurePosixPath(path)
                if path.parent in ancestors and notice(path.name):
                    return True
                for parent in ancestors:
                    try:
                        relative = path.relative_to(parent)
                    except ValueError:
                        continue
                    if len(relative.parts) > 1 and notice(relative.parts[0]):
                        return True
                return False
            found = [item["path"] for item in tree["tree"]
                     if item["type"] == "blob" and upstream_notice(item["path"])]
            if not found and identity == ("hexf-parse", "0.2.1"):
                # This exact public-domain source has no standalone license
                # file. Retain its README dedication and the versioned CC0 text.
                url = f"https://raw.githubusercontent.com/{repo}/{commit}/README.md"
                save(Path("LICENSE-UPSTREAM-README.md"), request(url),
                     {"kind": "exact_git_commit", "repository": repository, "commit": commit, "url": url})
                url = "https://creativecommons.org/publicdomain/zero/1.0/legalcode.txt"
                save(Path("LICENSE-CC0-1.0.txt"), request(url), {"kind": "versioned_standard_license", "url": url})
            elif not found and metadata.get("license") == "MIT":
                checksum = verify_locked_archive(source, cache, metadata, args.lockfile)
                declared_mit(source, save, metadata, commit, checksum, repository)
            elif not found:
                raise RuntimeError(f"no exact source notices found for {target.name}")
            for relative in found:
                url = f"https://raw.githubusercontent.com/{repo}/{commit}/{relative}"
                if url not in downloads:
                    downloads[url] = request(url)
                # Include the parent repository prefix for unusual nested files.
                saved = Path("LICENSE-UPSTREAM") / relative
                if not notice(saved.name):
                    saved = saved.with_name("LICENSE-" + saved.name)
                save(saved, downloads[url], {"kind": "exact_git_commit", "repository": repository,
                                            "commit": commit, "url": url})
        if provenance:
            provenance_path.write_text(json.dumps(provenance, indent=2) + "\n")
        metadata["notice_files"] = sorted(str(p.relative_to(target)) for p in target.rglob("*")
                                          if p.is_file() and notice(p.name) and p.name != "LICENSE-METADATA.json")
        metadata_path.write_text(json.dumps(metadata, indent=2) + "\n")
        results.append({"package": target.name, "notice_file_count": len(metadata["notice_files"]),
                        "has_full_notice_text": any(p.is_file() and notice(p.name) and p.suffix.lower() != ".json" for p in target.rglob("*"))})
    report = {"packages": results, "all_staged_packages_have_notice_text": all(p["has_full_notice_text"] for p in results)}
    (args.staged / "LICENSE-AUDIT.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({"staged_packages": len(results), "all_staged_packages_have_notice_text": report["all_staged_packages_have_notice_text"]}))
    if not report["all_staged_packages_have_notice_text"]:
        raise RuntimeError("some staged packages still lack actual notice text; inspect LICENSE-AUDIT.json")


if __name__ == "__main__":
    main()
