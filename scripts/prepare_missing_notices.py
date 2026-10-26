"""Augment locally staged dependency notices from registry files/exact Git commits.

Build-time packaging helper only. Existing staged LICENSE-METADATA.json records
identify packages. Local packaged notices are preferred. --fetch-upstream fills
missing repository-root notices at .cargo_vcs_info.json's exact commit, with
source URLs and SHA256 provenance. It never substitutes a floating branch.
"""
import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import urllib.request


def notice(name):
    return name.lower().startswith(("license", "licence", "copying", "notice", "copyright"))


def request(url):
    query = urllib.request.Request(url, headers={"User-Agent": "Hejjetrac-license-audit"})
    with urllib.request.urlopen(query, timeout=30) as response:
        return response.read()


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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--registry", type=Path, required=True)
    parser.add_argument("--staged", type=Path, default=Path("third-party-source"))
    parser.add_argument("--fetch-upstream", action="store_true")
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
        source = args.registry / target.name
        if not source.is_dir():
            raise RuntimeError(f"staged package has no local registry source: {source}")
        provenance_path = target / "LICENSE-UPSTREAM-PROVENANCE.json"
        provenance = json.loads(provenance_path.read_text()) if provenance_path.exists() else []

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
            font_notice = target.name.startswith("epaint_default_fonts-") and relative.parts[0] == "fonts" and path.suffix == ".txt"
            if notice(path.name) or font_notice:
                # Prefix supplementary packaged notices so the existing archive
                # collector retains them regardless of their upstream filename.
                saved = relative if notice(path.name) else Path("LICENSE-FONT-" + path.name)
                save(saved, path.read_bytes(), {"kind": "registry_package", "file": str(path)})
        umbrella = [p for p in target.iterdir() if p.is_file() and notice(p.name)
                    and p.name not in ("LICENSE-METADATA.json", "LICENSE-UPSTREAM-PROVENANCE.json")
                    and not p.name.startswith("LICENSE-FONT-")]
        umbrella.extend(p for p in (target / "LICENSE-UPSTREAM").rglob("*") if p.is_file())
        if not umbrella and args.fetch_upstream:
            vcs_path = source / ".cargo_vcs_info.json"
            if not vcs_path.is_file():
                raise RuntimeError(f"missing exact source revision for {target.name}")
            vcs = json.loads(vcs_path.read_text())
            commit = vcs.get("git", {}).get("sha1", "")
            if not re.fullmatch(r"[0-9a-f]{40}", commit):
                raise RuntimeError(f"invalid exact source commit for {target.name}")
            repository = metadata.get("repository")
            if target.name == "v4l2-sys-mit-0.3.0" and repository is None:
                # Published subcrate omits repository. The exact Cargo VCS commit
                # and v4l2-sys directory exist in the author's libv4l-rs repo.
                repository = "https://github.com/raymanfx/libv4l-rs"
            match = re.match(r"https://github.com/([^/]+/[^/]+)", repository or "")
            if not match:
                raise RuntimeError(f"no supported exact GitHub source for {target.name}")
            repo = match[1].removesuffix(".git")
            key = (repo, commit)
            if key not in trees:
                url = f"https://api.github.com/repos/{repo}/git/trees/{commit}?recursive=1"
                trees[key] = json.loads(request(url))
            tree = trees[key]
            if tree.get("truncated"):
                raise RuntimeError(f"incomplete source tree for {repo}@{commit}")
            crate_path = PurePosixPath(vcs.get("path_in_vcs", ""))
            ancestors = {PurePosixPath("."), crate_path, *crate_path.parents}
            found = [item["path"] for item in tree["tree"] if item["type"] == "blob"
                     and PurePosixPath(item["path"]).parent in ancestors
                     and notice(PurePosixPath(item["path"]).name)]
            if not found and target.name == "hexf-parse-0.2.1":
                # This exact public-domain source has no standalone license
                # file. Retain its README dedication and the versioned CC0 text.
                url = f"https://raw.githubusercontent.com/{repo}/{commit}/README.md"
                save(Path("LICENSE-UPSTREAM-README.md"), request(url),
                     {"kind": "exact_git_commit", "repository": repository, "commit": commit, "url": url})
                url = "https://creativecommons.org/publicdomain/zero/1.0/legalcode.txt"
                save(Path("LICENSE-CC0-1.0.txt"), request(url), {"kind": "versioned_standard_license", "url": url})
            elif not found:
                raise RuntimeError(f"no exact source notices found for {target.name}")
            for relative in found:
                url = f"https://raw.githubusercontent.com/{repo}/{commit}/{relative}"
                if url not in downloads:
                    downloads[url] = request(url)
                # Include the parent repository prefix for unusual nested files.
                saved = Path("LICENSE-UPSTREAM") / relative
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
