#!/usr/bin/env python3
"""Native daily-control pixel review on a real recording; development client only.

Uses the native JSONL bridge for actions and independent X11 window pixels for
inspection. The app never depends on Python, xwininfo, xwd or this script.
"""
import argparse
import csv
import hashlib
import json
from pathlib import Path
import re
import struct
import subprocess
import time
import zlib

from test_native_rigs import NativeBridge, field


def capture_window(directory, label, title):
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        found = subprocess.run(['xwininfo', '-name', title], text=True,
                               capture_output=True, timeout=5)
        match = re.search(r'Window id: (0x[0-9a-fA-F]+)', found.stdout)
        if found.returncode == 0 and match:
            break
        time.sleep(.1)
    else:
        raise RuntimeError(f'no X11 window named {title!r}')
    time.sleep(.3)
    raw = directory / (label + '.xwd')
    subprocess.run(['xwd', '-silent', '-id', match[1], '-out', str(raw)],
                   check=True, timeout=10)
    data = raw.read_bytes()
    header = struct.unpack('>25I', data[:100])
    width, height, bits, stride = header[4], header[5], header[11], header[12]
    if not (1 <= width <= 4096 and 1 <= height <= 4096 and bits in (24, 32)):
        raise RuntimeError(f'unsupported X11 framebuffer layout {width}x{height}/{bits}')
    offset = header[0] + 12 * header[19]
    if len(data) < offset + height * stride:
        raise RuntimeError('truncated X11 framebuffer')
    masks = header[14:17]
    if not all(masks):
        raise RuntimeError('X11 framebuffer is not true-color')
    shifts = [(mask & -mask).bit_length() - 1 for mask in masks]
    ranges = [mask >> shift for mask, shift in zip(masks, shifts)]
    byteorder = 'little' if header[7] == 0 else 'big'
    pixels = bytearray()
    for y in range(height):
        pixels.append(0)
        for x in range(width):
            pos = offset + y * stride + x * (bits // 8)
            value = int.from_bytes(data[pos:pos + bits // 8], byteorder)
            pixels.extend(((value & mask) >> shift) * 255 // maximum
                          for mask, shift, maximum in zip(masks, shifts, ranges))
    def chunk(kind, payload):
        return struct.pack('>I', len(payload)) + kind + payload + struct.pack('>I', zlib.crc32(kind + payload))
    png = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>2I5B', width, height, 8, 2, 0, 0, 0))
    png += chunk(b'IDAT', zlib.compress(pixels, 3)) + chunk(b'IEND', b'')
    destination = directory / (label + '.png')
    destination.write_bytes(png)
    return {'file': str(destination), 'title': title, 'window_id': match[1],
            'width': width, 'height': height, 'sha256': hashlib.sha256(png).hexdigest(),
            'capture': 'actual X11 native window pixels'}


def run(args):
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    config = args.config.read_text()
    for key, value in [('src_fn', str(args.video.resolve())), ('save_raw', 'n'),
                       ('save_debug', 'n'), ('sock_port', '0'), ('zmq_port', '0'),
                       ('com_port', ''), ('rust_instance_name', 'VR3')]:
        config = field(config, key, value)
    config_path = output / 'config.txt'
    config_path.write_text(config)
    bridge = NativeBridge([str(args.binary.resolve()), '--config', str(config_path),
                           '--control-stdio', '--window-size', '520x480'], output)
    report = {'passed': False, 'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(),
              'source_video': str(args.video.resolve()),
              'source_video_sha256': hashlib.sha256(args.video.read_bytes()).hexdigest(),
              'config_copy_sha256': hashlib.sha256(config_path.read_bytes()).hexdigest(),
              'scope': 'Actual native System-theme pixels; light/dark toolbar geometry and semantic-red rendering also have CPU egui regression coverage. No physical camera or throughput claim.',
              'captures': [], 'steps': []}
    def state():
        return bridge.request('status', 1)
    def wait(predicate):
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline:
            status = state()
            if status['error'] or status['command_error']:
                raise RuntimeError(f'native operation failed: {status}')
            if predicate(status):
                return status
            time.sleep(.05)
        raise RuntimeError(f'native operation did not converge: {status}')
    def advance(count):
        before = state()
        bridge.request('run', 1, frames=count)
        return wait(lambda status: status['advance_remaining'] == 0
                    and status['frames'] == before['frames'] + count)
    def capture(label, inspector=None):
        # Allow the requested native viewport resize/creation to settle before
        # matching its width-dependent title and reading presented pixels.
        time.sleep(.4)
        status = state()
        title = status['window_title'] if inspector is None else f'HejjéTrac · VR3 {inspector}'
        item = capture_window(output, label, title)
        item['status'] = status
        report['captures'].append(item)
        return status
    try:
        wait(lambda status: status['paused'])
        advance(40)
        canonical = bridge.request('config', 1)['text']
        baseline = state()
        capture('01-main-focused-record-idle')
        bridge.request('view', 1, full_image=True)
        capture('02-main-full-source')
        bridge.request('view', 1, diagnostics=True, overlays=False)
        capture('03-main-debug-no-saved-outlines')
        bridge.request('view', 1, diagnostics=False, full_image=False)
        take = output / 'recording'
        bridge.request('record_start', 1, directory=str(take),
                       record_options={'raw': True, 'debug': False},
                       recording_tuning={'encoding': 'raw'})
        wait(lambda status: status['recording'])
        advance(30)
        capture('04-main-recording-stop-square')
        bridge.request('record_stop', 1)
        done = wait(lambda status: not status['recording'])
        if done['media_recording']['raw']['state'] != 'complete' or done['media_warnings']:
            raise RuntimeError('visual record/stop did not finish the raw stream')
        report['recording_final'] = done['media_recording']
        paused_before = state()
        bridge.request('view', 1, calibration=True)
        capture('05-main-saved-calibration-overlay')
        capture('06-calibration-overview', 'calibration')
        if not state()['view']['calibration'] or not state()['calibration_summary']['visual_check_required']:
            raise RuntimeError('daily calibration overview is not explicit saved-geometry review')
        bridge.request('view', 1, calibration_step='ball', calibration_action='inspect')
        capture('07-main-ball-review')
        capture('08-calibration-ball-review', 'calibration')
        bridge.request('view', 1, calibration_action='keep')
        capture('09-calibration-exclusions-saved-review', 'calibration')
        if state()['view']['calibration_action'] != 'inspect':
            raise RuntimeError('Keep forced a redraw of the next saved layer')
        bridge.request('view', 1, calibration_action='redraw')
        capture('10-calibration-exclusions-redraw', 'calibration')
        bridge.request('view', 1, calibration_action='cancel')
        bridge.request('view', 1, calibration_step='lab', calibration_action='inspect')
        capture('11-calibration-lab-review', 'calibration')
        bridge.request('view', 1, calibration_action='finish')
        bridge.request('view', 1, calibration=False)
        checked = state()
        if checked['frames'] != paused_before['frames'] or checked['last_row'] != paused_before['last_row']:
            raise RuntimeError('saved calibration review changed the paused source row/clock')
        if checked['revision'] != baseline['revision'] or bridge.request('config', 1)['text'] != canonical:
            raise RuntimeError('visual inspection rewrote calibration or reset its epoch')
        bridge.request('view', 1, settings=True)
        capture('12-general-configuration', 'configuration')
        capture('13-main-toolbar-with-configuration')
        bridge.request('view', 1, settings=False)
        with next(take.glob('*-all-frames.dat')).open(newline='') as stream:
            rows = list(csv.reader(stream))
        with next(take.glob('*-quality.csv')).open(newline='') as stream:
            quality = list(csv.DictReader(stream))
        probe = subprocess.run(['ffprobe', '-v', 'error', '-count_frames', '-select_streams', 'v:0',
            '-show_entries', 'stream=width,height,codec_name,nb_read_frames', '-of', 'json',
            str(next(take.glob('*-media/raw.avi')))], capture_output=True, text=True, check=True)
        streams = json.loads(probe.stdout)['streams']
        if probe.stderr.strip() or len(streams) != 1 or len(rows) != len(quality) or len(rows) != int(streams[0]['nb_read_frames']):
            raise RuntimeError('native visual raw/numeric/quality counts differ')
        report['recorded_rows'] = len(rows)
        report['independent_raw_decode'] = streams[0]
        report['read_only_review_preserved_source_and_config'] = True
        report['passed'] = True
        bridge.request('close', 1, discard=True)
        bridge.process.wait(timeout=15)
        if bridge.process.returncode != 0:
            raise RuntimeError('native visual process failed during close')
    except Exception as error:
        report['failure'] = str(error)
        raise
    finally:
        (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        bridge.finish([1])
    print(json.dumps({'passed': True, 'captures': len(report['captures']),
                      'recorded_rows': report['recorded_rows'], 'binary_sha256': report['binary_sha256']}, indent=2))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=Path('target/release/fictrac-desktop'))
    parser.add_argument('--config', type=Path, default=Path('VR3.txt'))
    parser.add_argument('--video', type=Path, default=Path('/home/pkaushi/src/fictrac/VR_array/VR3/fictrac-raw-20261026_212053.avi'))
    parser.add_argument('--output', type=Path, required=True)
    run(parser.parse_args())
