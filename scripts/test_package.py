"""Build-time Debian payload/format checks; no installed package or root required."""
import importlib.util
import io
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location('hejjetrak_package', Path(__file__).with_name('package.py'))
PACKAGE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PACKAGE)


def members(path):
    result = {}
    with path.open('rb') as source:
        if source.read(8) != b'!<arch>\n':
            raise ValueError('Not an ar archive')
        while True:
            header = source.read(60)
            if not header:
                break
            if len(header) != 60 or header[58:] != b'`\n':
                raise ValueError('Invalid ar member header')
            name = header[:16].decode('ascii').strip().rstrip('/')
            length = int(header[48:58])
            value = source.read(length)
            if len(value) != length or name in result:
                raise ValueError('Truncated/duplicate ar member')
            result[name] = value
            if length % 2 and source.read(1) != b'\n':
                raise ValueError('Invalid ar padding')
    return result


class DebianPackageTests(unittest.TestCase):
    def test_complete_root_owned_payload_and_independent_ar_reader(self):
        with tempfile.TemporaryDirectory(prefix='hejjetrak-deb-test-') as directory:
            directory = Path(directory)
            stage = directory / 'stage'
            stage.mkdir()
            for name in ('hejjetrak', 'hejjetrak-cli', 'LICENSE'):
                (stage / name).write_text('test fixture ' + name)
            (stage / 'examples').mkdir()
            (stage / 'examples/sample.mp4').write_bytes(b'video fixture')
            assets = Path(__file__).resolve().parent.parent / 'assets'
            output = PACKAGE.build_deb(stage, directory / 'test.deb', '0.1.3', 'libc6 (>= 2.39), libudev1', assets)
            archive = members(output)
            self.assertEqual(list(archive), ['debian-binary', 'control.tar.gz', 'data.tar.gz'])
            self.assertEqual(archive['debian-binary'], b'2.0\n')
            with tarfile.open(fileobj=io.BytesIO(archive['control.tar.gz'])) as control:
                text = control.extractfile('control').read().decode()
                for line in ('Package: hejjetrak', 'Version: 0.1.3', 'Architecture: amd64',
                             'Depends: libc6 (>= 2.39), libudev1'):
                    self.assertIn(line + '\n', text)
            with tarfile.open(fileobj=io.BytesIO(archive['data.tar.gz'])) as data:
                for entry in data.getmembers():
                    self.assertEqual((entry.uid, entry.gid), (0, 0))
                    self.assertFalse(Path(entry.name).is_absolute())
                    self.assertNotIn('..', Path(entry.name).parts)
                self.assertEqual(data.getmember('usr/lib/hejjetrak/hejjetrak').mode, 0o755)
                self.assertEqual(data.getmember('usr/bin/hejjetrak-cli').linkname, '../lib/hejjetrak/hejjetrak-cli')
                self.assertEqual(data.extractfile('usr/lib/hejjetrak/examples/sample.mp4').read(), b'video fixture')
                self.assertIn(b'Exec=hejjetrak\n', data.extractfile('usr/share/applications/hejjetrak.desktop').read())
                self.assertTrue(data.getmember('usr/share/icons/hicolor/scalable/apps/hejjetrak.svg').isfile())
                self.assertTrue(data.getmember('usr/share/doc/hejjetrak/copyright').isfile())
                # Explicit extraction verifies real symlink targets without installing.
                extracted = directory / 'extracted'
                for entry in data.getmembers():
                    if entry.issym():
                        self.assertTrue(entry.linkname.startswith('../lib/hejjetrak/'))
                data.extractall(extracted)
                self.assertEqual((extracted / 'usr/bin/hejjetrak-cli').read_text(), 'test fixture hejjetrak-cli')
            # A different ar implementation must understand the actual package too.
            listing = subprocess.check_output(['ar', 't', str(output)], text=True)
            self.assertIn('control.tar.gz', listing)
            with self.assertRaises(FileExistsError):
                PACKAGE.build_deb(stage, output, '0.1.3', 'libc6', assets)

    def test_workspace_version_is_read_from_workspace_package_only(self):
        with tempfile.TemporaryDirectory() as directory:
            manifest = Path(directory) / 'Cargo.toml'
            manifest.write_text('[package]\nversion="9.9.9"\n[workspace.package]\nversion = "0.1.3-rc1"\n[dependencies]\n')
            self.assertEqual(PACKAGE.workspace_version(manifest), '0.1.3~rc1')


if __name__ == '__main__':
    unittest.main()
