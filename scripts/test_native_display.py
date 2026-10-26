#!/usr/bin/env python3
"""Verify native display-rate independence, raw recording and actual UI pixels.

The optional X11 inspector capture uses xwininfo/xwd as independent development
inspection tools. Neither tool is part of the application/runtime/package.
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

from test_native_rigs import NativeBridge, field, frame_progress


def capture_inspector(directory, rig_name):
    listing = subprocess.run(['xwininfo', '-name', f'HejjéTrac · {rig_name} configuration'], text=True,
                             capture_output=True, check=True, timeout=10).stdout
    match = re.search(r'Window id: (0x[0-9a-fA-F]+)', listing)
    if match is None:
        raise RuntimeError('native configuration window has no X11 identifier')
    path = directory / 'native-inspector.xwd'
    subprocess.run(['xwd', '-silent', '-id', match.group(1), '-out', str(path)], check=True, timeout=10)
    data = path.read_bytes()
    header = struct.unpack('>25I', data[:100])
    width, height, bits, stride = header[4], header[5], header[11], header[12]
    if not (1 <= width <= 4096 and 1 <= height <= 4096 and bits in (24, 32)):
        raise RuntimeError(f'unsupported X11 framebuffer layout: {width}x{height}/{bits}')
    offset = header[0] + 12 * header[19]
    pixel_bytes = bits // 8
    if len(data) < offset + height * stride:
        raise RuntimeError('X11 framebuffer capture is truncated')
    masks = header[14:17]
    if not all(masks):
        raise RuntimeError('X11 framebuffer has no true-color channel masks')
    shifts = [(mask & -mask).bit_length() - 1 for mask in masks]
    ranges = [mask >> shift for mask, shift in zip(masks, shifts)]
    byteorder = 'little' if header[7] == 0 else 'big'
    pixels = bytearray()
    for y in range(height):
        pixels.append(0)
        for x in range(width):
            pos = offset + y * stride + x * pixel_bytes
            value = int.from_bytes(data[pos:pos + pixel_bytes], byteorder)
            pixels.extend(((value & mask) >> shift) * 255 // maximum
                          for mask, shift, maximum in zip(masks, shifts, ranges))
    def chunk(kind, payload):
        return struct.pack('>I', len(payload)) + kind + payload + struct.pack('>I', zlib.crc32(kind + payload))
    png = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>2I5B', width, height, 8, 2, 0, 0, 0))
    png += chunk(b'IDAT', zlib.compress(pixels, 3)) + chunk(b'IEND', b'')
    (directory / 'native-inspector.png').write_bytes(png)
    return {'width': width, 'height': height, 'capture': 'X11 native window pixels'}


def run(args):
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    text = args.config.read_text()
    for key, value in [('save_raw', 'n'), ('save_debug', 'n'), ('sock_port', '0'), ('zmq_port', '0'), ('com_port', '')]:
        text = field(text, key, value)
    config = output / 'config.txt'
    config.write_text(text)
    bridge = NativeBridge([str(args.binary.resolve()), '--config', str(config), '--control-stdio',
                           '--window-size', '640x480', '--screenshot', str(output / 'native-tracking.png'),
                           '--screenshot-after', '3'], output)
    report = {'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(),
              'working_directory': str(output),
              'config_sha256': hashlib.sha256(config.read_bytes()).hexdigest(), 'cases': []}
    try:
        bridge.request('start', 1)
        initial = bridge.wait(lambda states: states[0]['frames'] >= 5)[0]
        original = bridge.request('config', 1)['text']
        bridge.request('record_start', 1, directory=str(output / 'recording'),
                       record_options={'raw': True, 'debug': False}, recording_tuning={'encoding': 'raw'})
        bridge.wait(lambda states: states[0]['recording'])
        previous = initial
        for fps in (1, 60):
            bridge.request('view', 1, display_fps=fps, diagnostics=True, overlays=False, settings=True)
            before = bridge.wait(lambda states: states[0]['preview_target_fps'] == fps)[0]
            started = time.monotonic()
            time.sleep(3)
            after = bridge.request('status', 1)
            elapsed = time.monotonic() - started
            if after['source_fps'] != initial['source_fps'] or after['revision'] != initial['revision']:
                raise RuntimeError('display-rate change altered source settings or numerical epoch')
            if after['frames'] <= before['frames'] or after['last_row']['sequence'] <= previous['last_row']['sequence']:
                raise RuntimeError('display-rate change interrupted source/frame continuity')
            if after['last_row']['timestamp_ms'] <= previous['last_row']['timestamp_ms']:
                raise RuntimeError('display-rate change reset the emitted source clock')
            if bridge.request('config', 1)['text'] != original:
                raise RuntimeError('display or recording controls rewrote the canonical config')
            processed = (after['frames'] - before['frames']) / elapsed
            if processed < 0.8 * initial['source_fps']:
                raise RuntimeError('display throttling reduced measured-frame processing below the operational fixture gate')
            if (fps == 1 and not 0 < after['preview_fps'] <= 1.5) or (fps == 60 and after['preview_fps'] <= 10):
                raise RuntimeError('measured preview cadence did not follow the display-rate control')
            report['cases'].append({'requested_display_fps': fps, 'elapsed_seconds': elapsed,
                                    'progress': frame_progress([before], [after], elapsed),
                                    'preview_fps': after['preview_fps'], 'before': before, 'after': after})
            previous = after
        maximum = bridge.request('status', 1)['window_max_inner_size']
        rejected = bridge.request('view', 1, expect_error=True,
                                  window_size=[maximum[0] + 1, maximum[1]])
        report['oversized_window_rejected'] = rejected
        report['resizes'] = []
        for _ in range(3):
            for width, height in [(96, 96), (1300, 180), (180, 900), (640, 480)]:
                bridge.request('view', 1, window_size=[width, height])
                resized = bridge.wait(lambda states: states[0]['window_inner_size'] is not None
                    and abs(states[0]['window_inner_size'][0] - width) <= 40
                    and abs(states[0]['window_inner_size'][1] - height) <= 40)[0]
                report['resizes'].append({'requested': [width, height],
                                           'actual': resized['window_inner_size'], 'frames': resized['frames']})
        if args.capture_inspector:
            report['inspector'] = capture_inspector(output, previous['window_title'].split(' · ')[0])
        bridge.request('record_stop', 1)
        final = bridge.wait(lambda states: not states[0]['recording'])[0]
        report['final'] = final
        status = final['media_recording']
        if status is None or status['raw']['state'] != 'complete' or final['media_warnings']:
            raise RuntimeError('fast raw recording did not finalize completely')
        with (output / 'recording' / 'epoch-0000-all-frames.dat').open(newline='') as stream:
            rows = [[float(v) for v in row] for row in csv.reader(stream)]
        with (output / 'recording' / 'epoch-0000-quality.csv').open(newline='') as stream:
            quality = list(csv.DictReader(stream))
        if len(rows) != len(quality) or len(rows) != status['raw']['encoded_frames']:
            raise RuntimeError('display throttling lost numeric, quality or raw media frames')
        if any(rows[i][0] != rows[i-1][0] + 1 or rows[i][21] <= rows[i-1][21]
               for i in range(1, len(rows))):
            raise RuntimeError('display throttling broke numerical frame/clock continuity')
        report['recorded_rows'] = len(rows)
        report['all_numeric_quality_raw_frames_aligned'] = True
        report['passed'] = True
        bridge.request('close', 1, discard=True)
        bridge.process.wait(timeout=15)
        if bridge.process.returncode != 0:
            raise RuntimeError(f'native app exited {bridge.process.returncode}')
        decoded = subprocess.run(['ffprobe', '-v', 'error', '-count_frames', '-select_streams', 'v:0',
            '-show_entries', 'stream=codec_name,width,height,r_frame_rate,nb_read_frames', '-of', 'json',
            str(output / 'recording' / 'epoch-0000-media' / 'raw.avi')],
            capture_output=True, text=True, check=True, timeout=60)
        streams = json.loads(decoded.stdout)['streams']
        if decoded.stderr.strip() or len(streams) != 1 or int(streams[0]['nb_read_frames']) != len(rows):
            raise RuntimeError('independent raw-video decode differs from complete numeric/quality counts')
        report['independent_raw_full_decode'] = streams[0]
    except Exception as error:
        report['passed'] = False
        report['failure'] = str(error)
        raise
    finally:
        (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        bridge.finish([1])
    print(json.dumps({'passed': True, 'recorded_rows': report['recorded_rows'],
                      'rates': [{'display': c['requested_display_fps'], 'preview': c['preview_fps'],
                                 'processed': c['progress'][0]['processed_frames_per_second']}
                                for c in report['cases']]}, indent=2))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=Path('target/release/fictrac-desktop'))
    parser.add_argument('--config', type=Path, default=Path('VR3.txt'))
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--capture-inspector', action='store_true')
    run(parser.parse_args())
