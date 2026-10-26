"""Build-time Debian payload/format checks; no installed package or root required."""
import importlib.util
import io
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest
from unittest import mock
import zipfile

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
            for name in ('hejjetrac', 'hejjetrac-cli', 'LICENSE'):
                (stage / name).write_text('test fixture ' + name)
            for name in ('hejjetrak', 'hejjetrak-cli'):
                (stage / name).symlink_to(name.replace('hejjetrak', 'hejjetrac'))
            (stage / 'examples').mkdir()
            (stage / 'examples/sample.mp4').write_bytes(b'video fixture')
            assets = Path(__file__).resolve().parent.parent / 'assets'
            output = PACKAGE.build_deb(stage, directory / 'test.deb', '0.1.4', 'libc6 (>= 2.35), libudev1', assets)
            archive = members(output)
            self.assertEqual(list(archive), ['debian-binary', 'control.tar.gz', 'data.tar.gz'])
            self.assertEqual(archive['debian-binary'], b'2.0\n')
            with tarfile.open(fileobj=io.BytesIO(archive['control.tar.gz'])) as control:
                text = control.extractfile('control').read().decode()
                for line in ('Package: hejjetrak', 'Version: 0.1.4', 'Architecture: amd64',
                             'Depends: libc6 (>= 2.35), libudev1'):
                    self.assertIn(line + '\n', text)
            with tarfile.open(fileobj=io.BytesIO(archive['data.tar.gz'])) as data:
                for entry in data.getmembers():
                    self.assertEqual((entry.uid, entry.gid), (0, 0))
                    self.assertFalse(Path(entry.name).is_absolute())
                    self.assertNotIn('..', Path(entry.name).parts)
                self.assertEqual(data.getmember('usr/lib/hejjetrak/hejjetrac').mode, 0o755)
                self.assertEqual(data.getmember('usr/lib/hejjetrak/hejjetrak').linkname, 'hejjetrac')
                self.assertEqual(data.getmember('usr/bin/hejjetrak-cli').linkname, '../lib/hejjetrak/hejjetrak-cli')
                self.assertEqual(data.extractfile('usr/lib/hejjetrak/examples/sample.mp4').read(), b'video fixture')
                self.assertIn(b'Exec=hejjetrac\n', data.extractfile('usr/share/applications/hejjetrak.desktop').read())
                self.assertTrue(data.getmember('usr/share/icons/hicolor/scalable/apps/hejjetrak.svg').isfile())
                self.assertTrue(data.getmember('usr/share/doc/hejjetrak/copyright').isfile())
                # Explicit extraction verifies real symlink targets without installing.
                extracted = directory / 'extracted'
                for entry in data.getmembers():
                    if entry.issym():
                        self.assertTrue(entry.linkname.startswith('../lib/hejjetrak/')
                                        or entry.linkname in ('hejjetrac', 'hejjetrac-cli'))
                data.extractall(extracted)
                for name in ('hejjetrac-cli', 'hejjetrak-cli'):
                    self.assertEqual((extracted / 'usr/bin' / name).read_text(), 'test fixture hejjetrac-cli')
            # A different ar implementation must understand the actual package too.
            listing = subprocess.check_output(['ar', 't', str(output)], text=True)
            self.assertIn('control.tar.gz', listing)
            with self.assertRaises(FileExistsError):
                PACKAGE.build_deb(stage, output, '0.1.4', 'libc6', assets)

    def test_workspace_version_is_read_from_workspace_package_only(self):
        with tempfile.TemporaryDirectory() as directory:
            manifest = Path(directory) / 'Cargo.toml'
            manifest.write_text('[package]\nversion="9.9.9"\n[workspace.package]\nversion = "0.1.3-rc1"\n[dependencies]\n')
            self.assertEqual(PACKAGE.workspace_version(manifest), '0.1.3-rc1')
            self.assertEqual(PACKAGE.workspace_version(manifest, debian=True), '0.1.3~rc1')


class ReleaseIdentityTests(unittest.TestCase):
    def test_old_binary_and_mismatched_release_tag_fail_before_copy(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'target/release').mkdir(parents=True)
            (root / 'target/release/fictrac-desktop').write_bytes(b'old executable')
            destination = root / 'package'
            destination.mkdir()
            old_version = subprocess.CompletedProcess([], 0, stdout='hejjetrak 0.1.3\n')
            with mock.patch.object(PACKAGE.subprocess, 'run', return_value=old_version):
                with self.assertRaisesRegex(ValueError, 'expected version 0.1.4'):
                    PACKAGE.install_binaries(root, destination, '', 'Darwin', '0.1.4')
            self.assertEqual(list(destination.iterdir()), [])
            manifest = root / 'Cargo.toml'
            manifest.write_text('[workspace.package]\nversion = "0.1.4"\n')
            PACKAGE.verify_release_tag(manifest, 'v0.1.4')
            with self.assertRaisesRegex(ValueError, 'does not match source version'):
                PACKAGE.verify_release_tag(manifest, 'v0.1.3')

    def test_zip_retains_small_unix_compatibility_aliases(self):
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            stage = directory / 'hejjetrac-macos-arm64'
            stage.mkdir()
            (stage / 'hejjetrac').write_bytes(b'executable payload')
            (stage / 'hejjetrak').symlink_to('hejjetrac')
            archive = directory / 'package.zip'
            PACKAGE.write_zip_tree(stage, archive)
            with zipfile.ZipFile(archive) as packaged:
                alias = packaged.getinfo(stage.name + '/hejjetrak')
                self.assertEqual(alias.external_attr >> 16, 0o120777)
                self.assertEqual(packaged.read(alias), b'hejjetrac')
                self.assertEqual(packaged.read(stage.name + '/hejjetrac'), b'executable payload')


class LinuxAbiTests(unittest.TestCase):
    @staticmethod
    def symbols(version):
        return subprocess.CompletedProcess([], 0, stdout=(
            '00000000 DF *UND* 00000000 (GLIBC_2.2.5) memcpy\n'
            f'00000000 DF *UND* 00000000 (GLIBC_{version}) __libc_start_main\n'))

    def test_both_executables_are_audited_and_versions_compare_numerically(self):
        with mock.patch.object(PACKAGE.subprocess, 'run', side_effect=[
                self.symbols('2.9'), self.symbols('2.35')]) as inspect:
            evidence = PACKAGE.linux_abi_requirements(Path('/package'))
        self.assertEqual([v['maximum_required_glibc'] for v in evidence['binaries']],
                         ['2.9', '2.35'])
        self.assertEqual([call.args[0][-1] for call in inspect.call_args_list],
                         ['/package/hejjetrac', '/package/hejjetrac-cli'])

    def test_either_newer_executable_rejects_the_package(self):
        for versions in [('2.39', '2.34'), ('2.34', '2.36')]:
            with self.subTest(versions=versions), mock.patch.object(
                    PACKAGE.subprocess, 'run', side_effect=[self.symbols(v) for v in versions]):
                with self.assertRaisesRegex(ValueError, 'Rebuild in the Ubuntu 22.04'):
                    PACKAGE.linux_abi_requirements(Path('/package'))

    def test_unknown_or_private_abi_is_not_treated_as_compatible(self):
        for output in ['', 'GLIBC_PRIVATE', 'GLIBC_2.34 GLIBC_ABI_DT_RELR']:
            with self.subTest(output=output), mock.patch.object(
                    PACKAGE.subprocess, 'run', return_value=subprocess.CompletedProcess([], 0, stdout=output)):
                with self.assertRaisesRegex(ValueError, 'cannot establish a public glibc ABI'):
                    PACKAGE.linux_abi_requirements(Path('/package'))


if __name__ == '__main__':
    unittest.main()
