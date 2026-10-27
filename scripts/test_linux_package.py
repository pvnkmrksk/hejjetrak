#!/usr/bin/env python3
"""Exercise the same release on Ubuntu 22.04/24.04 userspace, without a camera.

CI supplies an installed .deb, its portable tar, Xvfb and Mesa software graphics.
This checks ABI/dependencies, resource discovery, full-video numerical output,
native rendering, automatic graphics fallback and no-camera fallback. Software
rendering under Xvfb does not establish physical hybrid-GPU or Wayland support.
It does not simulate V4L hardware.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import signal
import shutil
import struct
import subprocess
import tarfile
import time
import zlib


def run(arguments, directory, label, *, env=None, timeout=60, allow_failure=False):
    arguments = [str(value) for value in arguments]
    tick = time.monotonic()
    process = subprocess.Popen(arguments, cwd=directory, env=env,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                               text=True, start_new_session=True)
    try:
        stdout, stderr = process.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        os.killpg(process.pid, signal.SIGTERM)
        try:
            stdout, stderr = process.communicate(timeout=5)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            stdout, stderr = process.communicate()
        (directory / (label + '.stdout')).write_text(stdout)
        (directory / (label + '.stderr')).write_text(stderr)
        raise RuntimeError(f'{label} exceeded {timeout}s; see saved stdout/stderr')
    (directory / (label + '.stdout')).write_text(stdout)
    (directory / (label + '.stderr')).write_text(stderr)
    if process.returncode and not allow_failure:
        raise RuntimeError(f'{label} exited {process.returncode}: {stderr[-2000:]}')
    return {'command': arguments, 'seconds': time.monotonic() - tick,
            'exit_code': process.returncode}


def config(cli, source, destination, sample, directory, label, *, camera=False):
    arguments = [cli, 'config', '--config', source, '--output', destination]
    for key, value in {'src_fn': '9999' if camera else str(sample),
                       'zmq_port': '-1', 'sock_port': '-1',
                       'save_raw': 'n', 'save_debug': 'n', 'do_display': 'y'}.items():
        arguments.extend(['--set', key + '=' + value])
    return run(arguments, directory, label)


def preview_pattern(png):
    """Check the known sample's light/dark texture, not just nonempty UI chrome."""
    if png[:8] != b'\x89PNG\r\n\x1a\n':
        raise ValueError('Native screenshot is not PNG')
    offset, compressed, header = 8, [], None
    while offset + 12 <= len(png):
        length = struct.unpack('>I', png[offset:offset + 4])[0]
        kind = png[offset + 4:offset + 8]
        data = png[offset + 8:offset + 8 + length]
        if len(data) != length:
            raise ValueError('Truncated native PNG')
        if kind == b'IHDR':
            header = struct.unpack('>IIBBBBB', data)
        elif kind == b'IDAT':
            compressed.append(data)
        offset += length + 12
    if header is None:
        raise ValueError('Native PNG has no header')
    width, height, depth, color, compression, filtering, interlace = header
    if (width, height) != (640, 480) or depth != 8 or color not in (2, 6) or any(
            (compression, filtering, interlace)):
        raise ValueError('Unexpected native screenshot format: ' + str(header))
    channels = 4 if color == 6 else 3
    stride = width * channels
    expected = (stride + 1) * height
    decoder = zlib.decompressobj()
    raw = decoder.decompress(b''.join(compressed), expected + 1)
    if len(raw) != expected or not decoder.eof or decoder.unused_data:
        raise ValueError('Unexpected native PNG pixel length')
    previous = bytearray(stride)
    light = dark = total = 0
    for y in range(height):
        start = y * (stride + 1)
        mode = raw[start]
        row = bytearray(raw[start + 1:start + 1 + stride])
        if mode > 4:
            raise ValueError('Unknown PNG filter')
        for x in range(stride):
            left = row[x - channels] if x >= channels else 0
            up = previous[x]
            corner = previous[x - channels] if x >= channels else 0
            if mode == 1:
                predictor = left
            elif mode == 2:
                predictor = up
            elif mode == 3:
                predictor = (left + up) // 2
            elif mode == 4:
                p = left + up - corner
                distances = (abs(p - left), abs(p - up), abs(p - corner))
                predictor = (left, up, corner)[distances.index(min(distances))]
            else:
                predictor = 0
            row[x] = (row[x] + predictor) & 255
        if height // 4 <= y < 3 * height // 4:
            for x in range(width // 4, 3 * width // 4):
                gray = sum(row[x * channels:x * channels + 3]) / 3
                dark += gray <= 100
                light += gray >= 160
                total += 1
        previous = row
    evidence = {'dark_fraction': dark / total, 'light_fraction': light / total}
    if min(evidence.values()) < 0.08:
        raise ValueError('Sample preview is blank or hidden: ' + str(evidence))
    return evidence


def graphics_environment(runtime, overrides=None):
    env = {**os.environ, 'XDG_RUNTIME_DIR': str(runtime),
           'LIBGL_ALWAYS_SOFTWARE': '1', 'WINIT_UNIX_BACKEND': 'x11'}
    # Each baseline/automatic test exercises the actual implicit backend policy.
    # Explicit selection is enabled only for the isolated negative control.
    for key in ('WGPU_BACKEND', 'WGPU_ADAPTER_NAME', 'WGPU_POWER_PREF'):
        env.pop(key, None)
    for key, value in (overrides or {}).items():
        if value is None:
            env.pop(key, None)
        else:
            env[key] = str(value)
    return env


def graphics_environment_evidence(env):
    return {key: env.get(key) for key in (
        'WGPU_BACKEND', 'WGPU_ADAPTER_NAME', 'WGPU_POWER_PREF',
        'WINIT_UNIX_BACKEND', 'LIBGL_ALWAYS_SOFTWARE',
        '__EGL_VENDOR_LIBRARY_FILENAMES', 'VK_ICD_FILENAMES', 'VK_DRIVER_FILES')}


def graphics_adapter(stderr):
    matches = re.findall(r'^Graphics: (.+) \((Gl|Vulkan), ([^)\r\n]+)\)\s*$',
                         stderr, re.MULTILINE)
    if len(matches) != 1:
        raise RuntimeError('Expected exactly one actual Gl/Vulkan adapter initialization log')
    name, backend, device_type = matches[0]
    return {'name': name, 'backend': backend, 'device_type': device_type}


def graphics_failure(stderr):
    if 'panicked' in stderr.lower():
        raise RuntimeError('Forced-GL control panicked instead of failing cleanly')
    markers = ('no suitable graphics adapter found', 'adapter selection failed:',
               'there was no valid format for the surface at all',
               'failed to create surface for any enabled backend:')
    failures = [line for line in stderr.splitlines()
                if line.lower().startswith('hejjetrac: wgpu error: ')
                and any(marker in line.lower() for marker in markers)]
    if len(failures) != 1:
        raise RuntimeError('Forced-GL control did not report an application graphics/adapter failure')
    return failures[0]


def gui_arguments(binary, state, screenshot, config_path):
    arguments = ['xvfb-run', '-a', '-s', '-screen 0 1280x800x24', binary,
                 '--window-size', '640x480', '--exit-after', '8',
                 '--state-output', state, '--screenshot', screenshot,
                 '--screenshot-after', '3']
    if config_path is not None:
        arguments.extend(['--config', config_path])
    return arguments


def gui(binary, directory, label, *, config_path=None, expect_fallback=False,
        env_overrides=None, expected_backend='Gl'):
    state = directory / (label + '.json')
    screenshot = directory / (label + '.png')
    runtime = directory / (label + '-runtime')
    runtime.mkdir(mode=0o700)
    env = graphics_environment(runtime, env_overrides)
    arguments = gui_arguments(binary, state, screenshot, config_path)
    result = run(arguments, directory, label, env=env, timeout=45)
    adapter = graphics_adapter((directory / (label + '.stderr')).read_text())
    if adapter['backend'] != expected_backend:
        raise RuntimeError(f'{label}: expected {expected_backend}, selected {adapter}')
    value = json.loads(state.read_text())
    instances = value.get('instances', [])
    if len(instances) != 1 or instances[0]['frames'] < 3 or instances[0]['error']:
        raise RuntimeError(f'{label}: no working preview: {value}')
    if expect_fallback and not instances[0]['source'].endswith('sample.mp4'):
        raise RuntimeError(f'{label}: unavailable camera did not switch to sample: {value}')
    pixels = screenshot.read_bytes()
    if len(pixels) < 1024 or pixels[:8] != b'\x89PNG\r\n\x1a\n':
        raise RuntimeError(f'{label}: missing native rendered screenshot')
    width, height = struct.unpack('>II', pixels[16:24])
    result.update({'frames': instances[0]['frames'], 'source': instances[0]['source'],
                   'error': instances[0]['error'], 'screenshot_size': [width, height],
                   'screenshot_sha256': hashlib.sha256(pixels).hexdigest(),
                   'visible_sample_pattern': preview_pattern(pixels),
                   'graphics_adapter': adapter, 'expected_graphics_backend': expected_backend,
                   'graphics_environment': graphics_environment_evidence(env)})
    return result


def automatic_graphics_fallback(binary, directory, config_path):
    manifests = sorted(Path('/usr/share/vulkan/icd.d').glob('*lvp*.json'))
    if len(manifests) != 1:
        raise RuntimeError(f'Expected exactly one installed Lavapipe ICD: {manifests}')
    manifest = manifests[0].resolve()
    disabled_egl = directory / 'nonexistent-egl-vendor.json'
    if disabled_egl.exists():
        raise RuntimeError('The EGL-disable negative-control path unexpectedly exists')
    overrides = {'WGPU_BACKEND': None, 'WGPU_ADAPTER_NAME': None,
                 'VK_ICD_FILENAMES': str(manifest), 'VK_DRIVER_FILES': str(manifest),
                 '__EGL_VENDOR_LIBRARY_FILENAMES': str(disabled_egl.resolve())}
    # Demonstrate that EGL really is unavailable in this environment. A camera,
    # decoder, missing-library, Xvfb or panic failure cannot satisfy this control.
    label = 'graphics-forced-gl-negative'
    state, screenshot = directory / (label + '.json'), directory / (label + '.png')
    runtime = directory / (label + '-runtime')
    runtime.mkdir(mode=0o700)
    env = graphics_environment(runtime, {**overrides, 'WGPU_BACKEND': 'gl'})
    negative = run(gui_arguments(binary, state, screenshot, config_path), directory,
                   label, env=env, timeout=45, allow_failure=True)
    if negative['exit_code'] not in (1, 2):
        raise RuntimeError(f'Forced-GL control did not fail cleanly: {negative}')
    stderr = (directory / (label + '.stderr')).read_text()
    failure = graphics_failure(stderr)
    if state.exists() or screenshot.exists() or re.search(r'^Graphics:', stderr, re.MULTILINE):
        raise RuntimeError('Forced-GL control unexpectedly initialized a rendered GUI')
    negative.update({'expected_graphics_failure': failure, 'no_rendered_gui': True,
                     'graphics_environment': graphics_environment_evidence(env)})
    automatic = gui(binary, directory, 'native-automatic-vulkan-fallback',
                    config_path=config_path, env_overrides=overrides,
                    expected_backend='Vulkan')
    if any(automatic['graphics_environment'][key] is not None
           for key in ('WGPU_BACKEND', 'WGPU_ADAPTER_NAME')):
        raise RuntimeError('Automatic graphics fallback accidentally used an adapter/backend override')
    return {'scope': 'Actual Xvfb/Mesa rendering with EGL vendor discovery disabled; '
                     'automatic Vulkan selection, not a physical hybrid-GPU or Wayland test',
            'lavapipe_icd': str(manifest),
            'lavapipe_icd_sha256': hashlib.sha256(manifest.read_bytes()).hexdigest(),
            'forced_gl_negative_control': negative, 'automatic_vulkan_preview': automatic}


def check_install(root, directory):
    directory.mkdir()
    build = json.loads((root / 'BUILD.json').read_text())
    for entry in build['binaries']:
        actual = hashlib.sha256((root / entry['name']).read_bytes()).hexdigest()
        if actual != entry['compiled_sha256']:
            raise RuntimeError(f'{root}: executable differs from BUILD.json: {entry["name"]}')
    notices = root / 'DependencyLicenses'
    notice_manifest = json.loads((notices / 'manifest.json').read_text())
    verified_notices = 0
    for package in notice_manifest:
        files = [notices / package['package'] / name for name in package['notice_files']]
        if not files or any(not path.is_file() for path in files):
            raise RuntimeError(f'{root}: dependency notices missing for {package["package"]}')
        actual_text = [path for path in files if path.suffix.lower() != '.json']
        if not actual_text or any(path.stat().st_size == 0 for path in actual_text):
            raise RuntimeError(f'{root}: dependency has metadata without actual notice text: {package["package"]}')
        provenance = notices / package['package'] / 'LICENSE-UPSTREAM-PROVENANCE.json'
        if provenance.exists():
            for entry in json.loads(provenance.read_text()):
                relative = Path(entry['notice_file'])
                if relative.is_absolute() or '..' in relative.parts:
                    raise RuntimeError('Invalid dependency notice provenance path')
                path = notices / package['package'] / relative
                if not path.exists():
                    raise RuntimeError(f'{root}: provenance notice is absent from package: {path}')
                if hashlib.sha256(path.read_bytes()).hexdigest() != entry['sha256']:
                    raise RuntimeError(f'{root}: dependency notice differs from provenance: {path}')
        verified_notices += 1
    font_entries = [package for package in notice_manifest if package['package'].startswith('epaint_default_fonts-')]
    if font_entries:
        font_names = ' '.join(font_entries[0]['notice_files'])
        if any(name not in font_names for name in ('OFL', 'UFL', 'Hack-Regular', 'emoji-icon-font-mit-license')):
            raise RuntimeError(f'{root}: bundled font license notices are incomplete')
    cli = root / 'hejjetrac-cli'
    sample = root / 'examples/sample.mp4'
    reference = root / 'examples/benchmark/upstream.dat'
    configured = directory / 'config.txt'
    checks = [run([cli, '--version'], directory, 'version'),
              config(cli, root / 'examples/benchmark/config.txt', configured,
                     sample, directory, 'configure'),
              run([cli, 'inspect', sample, '--decode'], directory, 'decode'),
              run([cli, 'track', '--config', configured, '--input', sample,
                   '--output', directory / 'tracking'], directory, 'track'),
              run([cli, 'compare', '--reference', reference, '--candidate',
                   directory / 'tracking/tracking.dat', '--report',
                   directory / 'comparison.json'], directory, 'compare')]
    summary = json.loads((directory / 'tracking/summary.json').read_text())
    comparison = json.loads((directory / 'comparison.json').read_text())
    if summary['frames'] != 300 or summary['rejected'] != 0 or not comparison['passed']:
        raise RuntimeError(f'{root}: full sample failed: {summary}, {comparison}')
    checks.append(gui(root / 'hejjetrac', directory, 'native-video', config_path=configured))
    graphics_fallback = automatic_graphics_fallback(root / 'hejjetrac', directory, configured)
    return {'root': str(root), 'build': build, 'dependency_packages_with_verified_notice_text': verified_notices,
            'sample': summary, 'comparison': comparison, 'checks': checks,
            'automatic_graphics_fallback': graphics_fallback}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--archive', required=True, type=Path)
    parser.add_argument('--installed', type=Path, default=Path('/usr/lib/hejjetrak'))
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    if platform.system() != 'Linux':
        parser.error('This test requires Linux userspace, Xvfb and the installed package')
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    report = {'distribution': platform.freedesktop_os_release(), 'libc': platform.libc_ver(),
              'archive_sha256': hashlib.sha256(args.archive.read_bytes()).hexdigest(),
              'harness_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              'scope': 'Actual packaged CLI, Xvfb/Mesa GLES and automatic Vulkan fallback; '
                       'no physical V4L camera, hardware hybrid-GPU or Wayland claim',
              'status': 'running', 'checks': []}
    try:
        with tarfile.open(args.archive) as archive:
            # Release archives contain only this package root, files and relative
            # command aliases. Refuse absolute/traversal paths before extraction.
            for member in archive.getmembers():
                path = Path(member.name)
                if path.is_absolute() or '..' in path.parts:
                    raise ValueError('Unsafe path in package archive: ' + member.name)
                if not (member.isfile() or member.isdir() or member.issym()):
                    raise ValueError('Unexpected package member type: ' + member.name)
                if member.islnk() or (member.issym() and
                                     member.linkname not in ('hejjetrac', 'hejjetrac-cli')):
                    raise ValueError('Unexpected package link: ' + member.name)
            archive.extractall(output / 'extracted')
        roots = [path for path in (output / 'extracted').iterdir() if path.is_dir()]
        if len(roots) != 1:
            raise RuntimeError('Expected one portable package root')
        portable = roots[0]
        for label, root in [('installed', args.installed.resolve()), ('portable', portable)]:
            report['checks'].append(check_install(root, output / label))
        if not list(Path('/dev').glob('video*')):
            # Exercise ordinary double-click startup in a disposable copy, with
            # a known unavailable camera and wire ports disabled.
            fallback = output / 'default-startup'
            fallback.mkdir()
            app = fallback / 'app'
            shutil.copytree(portable, app, symlinks=True)
            new_config = fallback / 'VR1.txt'
            config(app / 'hejjetrac-cli', app / 'VR1.txt', new_config,
                   app / 'examples/sample.mp4', fallback, 'configure-camera', camera=True)
            new_config.replace(app / 'VR1.txt')
            report['no_camera_startup'] = gui(app / 'hejjetrac', fallback,
                                             'native-fallback', expect_fallback=True)
        else:
            report['no_camera_startup'] = {'skipped': 'Runner exposes V4L hardware'}
        report['status'] = 'passed'
    except Exception as error:
        report['status'] = 'failed'
        report['error'] = str(error)
        raise
    finally:
        (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        # The original tar/deb are already retained as build artifacts. Keep the
        # runtime evidence compact instead of uploading duplicate executables.
        for generated in (output / 'extracted', output / 'default-startup/app'):
            shutil.rmtree(generated, ignore_errors=True)
    print(json.dumps({'status': report['status'], 'output': str(output)}))


if __name__ == '__main__':
    main()
