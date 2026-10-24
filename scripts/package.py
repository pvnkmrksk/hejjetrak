"""Build-time archive assembly; never used by the Rust application."""
import json
import os
from pathlib import Path
import platform
import plistlib
import shutil
import subprocess

root = Path(__file__).resolve().parent.parent
system = platform.system()
arch = platform.machine().lower()
label = {'Darwin': 'macos', 'Windows': 'windows', 'Linux': 'linux'}[system]
name = f'hejjetrak-{label}-{arch}'
stage = root / 'dist' / name
stage.mkdir(parents=True, exist_ok=False)
suffix = '.exe' if system == 'Windows' else ''
if system == 'Darwin':
    app = stage / 'hejjetrak.app' / 'Contents'
    binary = app / 'MacOS'
    resource = app / 'Resources'
    binary.mkdir(parents=True)
    resource.mkdir()
    info = plistlib.loads((root / 'apps/fictrac-desktop/Info.plist').read_bytes())
    info['CFBundleExecutable'] = 'hejjetrak'
    (app / 'Info.plist').write_bytes(plistlib.dumps(info))
else:
    binary = resource = stage
for source, target in [('fictrac-desktop', 'hejjetrak'), ('fictrac', 'hejjetrak-cli')]:
    shutil.copy2(root / 'target/release' / (source + suffix), binary / (target + suffix))
for directory in ['examples', 'docs']:
    shutil.copytree(root / directory, resource / directory)
for pattern in ['VR*.txt']:
    for config in root.glob(pattern):
        shutil.copy2(config, resource / config.name)
for filename in ['README.md', 'LICENSE', 'ATTRIBUTION.md', 'THIRD_PARTY.md']:
    shutil.copy2(root / filename, resource / filename)
licenses = resource / 'DependencyLicenses'
licenses.mkdir()
manifest = []
for package in sorted((root / 'third-party-source').iterdir()):
    if not package.is_dir():
        continue
    names = []
    for path in package.rglob('*'):
        if path.is_file() and (path.name.lower().startswith(('license', 'licence', 'copying', 'notice', 'copyright'))):
            relative = path.relative_to(package)
            destination = licenses / package.name / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(path, destination)
            names.append(str(relative))
    manifest.append({'package': package.name, 'notice_files': names})
(licenses / 'manifest.json').write_text(json.dumps(manifest, indent=2))
if system == 'Darwin':
    subprocess.run(['codesign', '--force', '--deep', '--sign', '-', str(stage / 'hejjetrak.app')], check=True)
    subprocess.run(['codesign', '--verify', '--deep', '--strict', str(stage / 'hejjetrak.app')], check=True)
archive = shutil.make_archive(str(root / 'dist' / name), 'zip' if system in ('Darwin', 'Windows') else 'gztar', root_dir=stage.parent, base_dir=stage.name)
print(archive)
