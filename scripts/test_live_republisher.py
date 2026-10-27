#!/usr/bin/env python3
"""Development-only exact-byte check of live Rust and existing Python publishers.

Copies VR2--VR4 configurations and records numerical evidence only. It never
binds the original UDP/987x ports, opens camera0, or terminates other processes.
"""
from __future__ import annotations

import argparse
import ast
import base64
from collections import deque
from datetime import datetime, timezone
import hashlib
import json
import math
from pathlib import Path
import socket
import threading
import time

from test_native_rigs import NativeBridge, field


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def utc_now():
    return datetime.now(timezone.utc).isoformat()


class JsonCapture:
    def __init__(self, ports):
        import zmq
        self.zmq = zmq
        self.ports = list(ports)
        self.records = {port: deque(maxlen=100000) for port in ports}
        self.received = {port: 0 for port in ports}
        self.errors = []
        self.ready = threading.Event()
        self.stop = threading.Event()
        self.thread = threading.Thread(target=self.receive, daemon=True)
        self.thread.start()
        if not self.ready.wait(5):
            raise RuntimeError('subscriber initialization timed out')

    def receive(self):
        context = self.zmq.Context()
        sockets = {}
        try:
            poller = self.zmq.Poller()
            for port in self.ports:
                sub = context.socket(self.zmq.SUB)
                sub.setsockopt(self.zmq.LINGER, 0)
                sub.setsockopt(self.zmq.RCVHWM, 20000)
                sub.setsockopt(self.zmq.RECONNECT_IVL, 10)
                sub.subscribe(b'')
                sub.connect(f'tcp://127.0.0.1:{port}')
                sockets[sub] = port
                poller.register(sub, self.zmq.POLLIN)
            self.ready.set()
            while not self.stop.is_set():
                for sub, events in poller.poll(20):
                    if events & self.zmq.POLLIN:
                        while True:
                            try:
                                parts = sub.recv_multipart(self.zmq.NOBLOCK)
                            except self.zmq.Again:
                                break
                            port = sockets[sub]
                            self.received[port] += 1
                            self.records[port].append((time.monotonic(), parts))
        except Exception as error:
            self.errors.append(str(error))
            self.ready.set()
        finally:
            for sub in sockets:
                sub.close(linger=0)
            context.term()

    def finish(self):
        self.stop.set()
        self.thread.join(timeout=5)
        if self.thread.is_alive():
            raise RuntimeError('subscriber thread did not close')


def original_converter(path):
    # Execute only the explicitly selected conversion function, never its
    # command-line entrypoint, network binding, or configuration-file text.
    tree = ast.parse(path.read_text())
    function = next(node for node in tree.body
                    if isinstance(node, ast.FunctionDef) and node.name == 'process_line')
    namespace = {'json': json, 'math': math}
    exec(compile(ast.Module(body=[function], type_ignores=[]), str(path), 'exec'), namespace)

    class Sink:
        def __init__(self):
            self.frames = []

        def send_string(self, payload):
            self.frames.append(payload.encode('utf-8'))

    def convert(row):
        sink = Sink()
        namespace['process_line']((b'FT, ' + row).decode('utf-8'), sink)
        if len(sink.frames) != 1:
            raise RuntimeError('original republisher did not emit exactly one JSON frame')
        return sink.frames[0]
    return convert


def packet_shape(parts):
    if len(parts) != 1:
        return False
    try:
        value = json.loads(parts[0])
        return (list(value) == ['x', 'y', 'z', 'yaw', 'pitch', 'roll']
                and all(type(v) in (float, int) and math.isfinite(v) for v in value.values()))
    except (ValueError, TypeError, UnicodeDecodeError):
        return False


def exact_window(expected, records):
    invalid = sum(not packet_shape(parts) for _, parts in records)
    payloads = [parts[0] for _, parts in records if len(parts) == 1]
    starts = [i for i, payload in enumerate(payloads) if expected and payload == expected[0]]
    for start in starts:
        if payloads[start:start + len(expected)] == expected:
            selected = payloads[start:start + len(expected)]
            return {'passed': invalid == 0, 'expected_rows': len(expected),
                    'observed_packets': len(records), 'invalid_schema_or_framing': invalid,
                    'expected_unique_payloads': len(set(expected)),
                    'window_first_observed_index': start,
                    'window_last_observed_index': start + len(expected) - 1,
                    'exact_contiguous_expected_rows': len(expected),
                    'window_payload_sha256': hashlib.sha256(b'\n'.join(selected)).hexdigest(),
                    'scope': 'Complete accepted-recording payload vector appears contiguously and byte-exactly. Identical JSON poses inherently carry no unique frame ID.'}
    observed_set = set(payloads)
    return {'passed': False, 'expected_rows': len(expected), 'observed_packets': len(records),
            'invalid_schema_or_framing': invalid,
            'expected_payloads_absent': sum(payload not in observed_set for payload in expected),
            'first_expected': expected[0].decode() if expected else None,
            'first_observed': payloads[0].decode(errors='replace') if payloads else None,
            'failure': 'Complete expected payload vector not found contiguously in captured messages'}


def run(args):
    root = Path(__file__).resolve().parents[1]
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    configs = output / 'configs'
    configs.mkdir()
    report = {'started_utc': utc_now(), 'binary': str(args.binary.resolve()),
              'binary_sha256': digest(args.binary), 'original_republisher': str(args.republisher),
              'original_republisher_sha256': digest(args.republisher),
              'physical_rigs_requested': [2, 3, 4], 'camera0_opened': False,
              'existing_publishers_modified': False, 'cases': [], 'passed': False,
              'scope': 'Live same-row numeric/UDP-to-existing-Python/parallel-Rust JSON byte check. Original VR1 receives passive observation only. Remote Unity movement requires its own receiver-side evidence.'}
    copied = []
    for rig in (2, 3, 4):
        original = root / f'VR{rig}.txt'
        text = original.read_text()
        fields = {'save_raw': 'n', 'save_debug': 'n', 'sock_output': 'y',
                  'sock_host': '127.0.0.1', 'sock_port': str(1110 + rig), 'com_port': '',
                  'zmq_host': '127.0.0.1', 'zmq_port': str(21980 + rig),
                  'zmq_output_mode': 'legacy_matrex_vr', 'zmq_framing': 'single',
                  'zmq_topic': 'FT', 'rust_instance_name': f'VR{rig}'}
        for key, value in fields.items():
            text = field(text, key, value)
        path = configs / f'VR{rig}.txt'
        path.write_text(text)
        copied.append(path)
    for port in (21982, 21983, 21984):
        with socket.socket() as reserve:
            reserve.bind(('127.0.0.1', port))
    capture = JsonCapture([9871, 9872, 9873, 9874, 21982, 21983, 21984])
    bridge = None
    identities = []
    valid = []
    try:
        if capture.errors:
            raise RuntimeError('; '.join(capture.errors))
        command = [str(args.binary.resolve()), '--config', str(copied[0]), '--windows', '3',
                   '--grid', '3x1', '--control-stdio', '--window-size', '520x480']
        report['command'] = command
        bridge = NativeBridge(command, output)
        states = bridge.request('sessions')
        identities = [state['session'] for state in states]
        if len(states) != 3:
            raise RuntimeError('native window count is not three')
        # Camera opens are asynchronous. A busy source can close its worker
        # channel before UI commands arrive; settle startup before touching it.
        startup_deadline = time.monotonic() + 15
        while any(not state['source_label'] and not state['error'] and not state['ended'] for state in states):
            if time.monotonic() > startup_deadline:
                break
            time.sleep(.1)
            states = bridge.request('sessions')
        report['startup_command_errors'] = {}
        for rig, state in zip((2, 3, 4), states):
            if (state['source'] == str(2 * (rig - 1)) and state['source_label']
                    and state['worker_active'] and not state['error'] and not state['ended']):
                try:
                    # The control bridge intentionally starts its first window
                    # paused; frames cannot be the prerequisite for Start.
                    bridge.request('start', state['session'])
                    bridge.request('view', state['session'], display_fps=15)
                except RuntimeError as error:
                    report['startup_command_errors'][state['session']] = str(error)
        time.sleep(2)  # Explicit subscription/source warmup before record_start.
        states = bridge.request('sessions')
        report['subscription_warmup_counts'] = dict(capture.received)
        report['startup'] = states
        for rig, state in zip((2, 3, 4), states):
            identity = state['session']
            case = {'rig': rig, 'session': identity, 'camera_id': 2 * (rig - 1),
                    'python_port': 9870 + rig, 'rust_port': 21980 + rig,
                    'original_config_sha256': digest(root / f'VR{rig}.txt'),
                    'copied_config_sha256': digest(configs / f'VR{rig}.txt')}
            if (state['source'] != str(case['camera_id']) or state['frames'] < 5
                    or state['error'] or state['command_error'] or state['ended']):
                case['skipped'] = True
                case['failure'] = 'Requested live camera unavailable/busy or failed; existing processes untouched'
                case['source_status'] = state
            else:
                directory = output / f'record-VR{rig}'
                bridge.request('record_start', identity, directory=str(directory),
                               record_options={'raw': False, 'debug': False})
                case['directory'] = str(directory)
                valid.append(case)
            report['cases'].append(case)
        if not valid:
            raise RuntimeError('none of the requested other cameras is available')
        deadline = time.monotonic() + 30
        while True:
            states = bridge.request('sessions')
            by_id = {state['session']: state for state in states}
            if all(by_id[case['session']]['recording']
                   and by_id[case['session']]['recording_directory'] == case['directory'] for case in valid):
                break
            if time.monotonic() > deadline:
                raise RuntimeError('recording start did not complete')
            time.sleep(.1)
        report['measurement_start_utc'] = utc_now()
        report['measurement_start_monotonic'] = time.monotonic()
        initial = states
        print('live cameras recording numeric data; observing original VR1 passively', flush=True)
        target = time.monotonic() + args.seconds
        while time.monotonic() < target:
            states = bridge.request('sessions')
            for case in valid:
                state = next(s for s in states if s['session'] == case['session'])
                if state['error'] or state['command_error'] or state['ended']:
                    raise RuntimeError(f"VR{case['rig']} stopped: {state['error']} {state['command_error']}")
            time.sleep(min(.5, max(0, target - time.monotonic())))
        report['measurement_end_utc'] = utc_now()
        report['measurement_end_monotonic'] = time.monotonic()
        report['measurement_end_status'] = states
        for case in valid:
            bridge.request('record_stop', case['session'])
        deadline = time.monotonic() + 30
        while not all((Path(case['directory']) / 'completion.json').exists() for case in valid):
            bridge.request('sessions')
            if time.monotonic() > deadline:
                raise RuntimeError('recording finalization marker did not arrive')
            time.sleep(.1)
        # Continue draining both PUB paths after the last recorded accepted row.
        time.sleep(.5)
        if not args.keep_running:
            for identity in reversed(identities):
                bridge.request('close', identity, discard=True)
            bridge.process.wait(timeout=20)
            report['native_exit_code'] = bridge.process.returncode
            if bridge.process.returncode:
                raise RuntimeError('native test process did not exit cleanly')
        capture.finish()
        if capture.errors:
            raise RuntimeError('; '.join(capture.errors))
        convert = original_converter(args.republisher)
        for case in valid:
            directory = Path(case['directory'])
            marker = json.loads((directory / 'completion.json').read_text())
            if marker['state'] != 'complete' or marker.get('numeric_error') or marker.get('completion_error'):
                raise RuntimeError('numeric completion failed')
            accepted = (directory / 'epoch-0000.dat').read_bytes().splitlines(keepends=True)
            all_rows = (directory / 'epoch-0000-all-frames.dat').read_bytes().splitlines()
            expected = [convert(row) for row in accepted]
            rust = exact_window(expected, list(capture.records[case['rust_port']]))
            python = exact_window(expected, list(capture.records[case['python_port']]))
            first = [float(x) for x in all_rows[0].split(b',')]
            last = [float(x) for x in all_rows[-1].split(b',')]
            span = (last[21] - first[21]) / 1000
            case.update({'numeric_rows': len(all_rows), 'accepted_rows': len(accepted),
                         'source_clock_span_seconds': span,
                         'processed_rows_per_source_second': (len(all_rows) - 1) / span,
                         'rust_direct': rust, 'existing_python_republisher': python,
                         'same_complete_payload_vector_sha256': hashlib.sha256(b'\n'.join(expected)).hexdigest(),
                         'passed': rust['passed'] and python['passed']})
        vr1 = list(capture.records[9871])
        times = [t for t, _ in vr1]
        report['original_VR1_passive'] = {'messages': len(vr1),
            'all_schema_and_single_frame_valid': all(packet_shape(parts) for _, parts in vr1),
            'observed_span_seconds': times[-1] - times[0] if len(times) > 1 else 0,
            'maximum_interarrival_seconds': max((b-a for a, b in zip(times, times[1:])), default=None),
            'scope': 'Passive observation; no original camera/publisher control or UDP bind.'}
        for port in capture.ports:
            with (output / f'wire-{port}.jsonl').open('w') as stream:
                for tick, parts in capture.records[port]:
                    stream.write(json.dumps({'monotonic': tick, 'frames_base64': [base64.b64encode(frame).decode('ascii') for frame in parts]}) + '\n')
        report['received_packets'] = capture.received
        report['passed'] = (all(case.get('passed', False) for case in report['cases'])
                            and len(valid) == 3 and bool(vr1)
                            and report['original_VR1_passive']['all_schema_and_single_frame_valid'])
        print(json.dumps({'passed': report['passed'], 'cases': [{k: case.get(k) for k in ['rig','numeric_rows','accepted_rows','processed_rows_per_source_second','passed','skipped']} for case in report['cases']], 'VR1': report['original_VR1_passive']}, indent=2), flush=True)
        if args.keep_running:
            import os
            import signal
            stopped = threading.Event()
            signal.signal(signal.SIGTERM, lambda *_: stopped.set())
            signal.signal(signal.SIGINT, lambda *_: stopped.set())
            report['keepalive'] = {'supervisor_pid': os.getpid(), 'native_pid': bridge.process.pid,
                                 'stop_file': str(output / 'stop.request'),
                                 'recording_after_test': False,
                                 'routes': {f'VR{case["rig"]}': f'UDP {1110 + case["rig"]} -> existing Python -> TCP {case["python_port"]}' for case in valid}}
            (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
            # The complete test transcript remains on disk. Keepalive status
            # replaces one small JSON file rather than accumulating camera
            # capability dumps forever. This affects future launches only.
            bridge.transcript.flush()
            bridge.transcript.close()
            bridge.transcript = open(os.devnull, 'w')
            progress = {}
            previous_warnings = None
            while not stopped.is_set() and not (output / 'stop.request').exists():
                states = bridge.request('sessions')
                active = [state for state in states if state['session'] in {case['session'] for case in valid}]
                failure = []
                warnings = []
                now = time.monotonic()
                for state in active:
                    identity = state['session']
                    frames, last_progress = progress.get(identity, (None, now))
                    if state['frames'] != frames or state['paused']:
                        last_progress = now
                    progress[identity] = (state['frames'], last_progress)
                    if state['ended'] or not state['worker_active']:
                        failure.append({'session': identity, 'reason': 'capture worker ended or inactive'})
                    elif not state['paused'] and now - last_progress > 30:
                        failure.append({'session': identity, 'reason': 'unpaused capture made no frame progress for 30 seconds'})
                    # Session.error also contains ordinary UI failures such as
                    # requesting an unavailable fourth window. Keep those
                    # messages visible without terminating healthy capture.
                    if state['error'] or state['command_error'] or state['recording'] or state['paused']:
                        warnings.append({key: state.get(key) for key in
                                         ['session','error','command_error','recording','paused']})
                missing = {case['session'] for case in valid} - {state['session'] for state in active}
                if missing:
                    warnings.append({'missing_sessions': sorted(missing)})
                if warnings != previous_warnings:
                    with (output / 'keepalive-events.jsonl').open('a') as events:
                        events.write(json.dumps({'updated_utc': utc_now(), 'warnings': warnings}) + '\n')
                    previous_warnings = warnings
                status = {'updated_utc': utc_now(),
                          'state': 'failed' if failure else ('streaming_with_warnings' if warnings else 'streaming'),
                          **report['keepalive'],
                          'failures': failure, 'warnings': warnings,
                          'sessions': [{key: state.get(key) for key in ['session','source','source_label','frames','actual_fps','recording','paused','ended','worker_active','error','command_error','outputs']} for state in active]}
                temporary = output / 'live-status.json.tmp'
                temporary.write_text(json.dumps(status, indent=2) + '\n')
                temporary.replace(output / 'live-status.json')
                if failure:
                    raise RuntimeError('streaming session failed; see live-status.json')
                stopped.wait(args.poll_seconds)
            for identity in reversed(identities):
                bridge.request('close', identity, discard=True)
            bridge.process.wait(timeout=20)
            report['native_exit_code'] = bridge.process.returncode
            report['keepalive']['stopped_utc'] = utc_now()
            if bridge.process.returncode:
                raise RuntimeError('native streaming process did not exit cleanly')
    except Exception as error:
        report['wire_test_passed'] = report['passed']
        report['passed'] = False
        report['failure'] = str(error)
        raise
    finally:
        if bridge:
            bridge.finish(identities)
        capture.finish()
        report['finished_utc'] = utc_now()
        (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    return report['passed']


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=Path('dist/hejjetrac-linux-x86_64/hejjetrac'))
    parser.add_argument('--republisher', type=Path, default=Path('/home/pkaushi/src/fictrac/VR_array/socket_zmq_republisher.py'))
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--seconds', type=float, default=20)
    parser.add_argument('--keep-running', action='store_true', help='After the numeric test, keep only this native process streaming until output/stop.request exists')
    parser.add_argument('--poll-seconds', type=float, default=5, help='Keepalive status cadence; no effect on tracking or output FPS')
    args = parser.parse_args()
    if not 5 <= args.seconds <= 60:
        parser.error('--seconds must be between 5 and 60')
    if not .25 <= args.poll_seconds <= 30:
        parser.error('--poll-seconds must be between 0.25 and 30')
    raise SystemExit(0 if run(args) else 2)
