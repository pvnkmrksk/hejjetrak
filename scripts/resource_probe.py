#!/usr/bin/env python3
"""Serial native resource validation; no application runtime dependency.

Measures the launched desktop PID's cumulative CPU-time delta, RSS and OS thread
count while driving its real JSONL sessions. It samples once per second by
calling ps (PowerShell Get-Process on Windows); the Python/control/OS sampler is outside the application's CPU total
but still consumes a small amount of host time. Startup/open/warmup/teardown are
excluded. Source-paced rows are never skipped by this helper. A short input that
ends during an active case fails instead of silently measuring EOF idle time.
Quality fields are sampled last-row states, not all per-frame decisions; use
separate recorded all-frame/quality evidence for complete acceptance counts.

The bundled 300-frame video supports a 20-second measurement at --fps10, after
three seconds warmup. For 100/120/240 FPS provide a sufficiently long input
(>= rate*(warmup+seconds) frames); this tool does not fabricate camera capability
or claim high-rate results from the 30 FPS built-in Mac camera. Run cases
serially, with all builds/tests/other benchmark processes stopped.
"""
import argparse
import hashlib
import json
import math
import platform
import queue
import threading
import statistics
import subprocess
import time
from pathlib import Path


def field(text, key, value):
    lines = []
    found = False
    for line in text.splitlines():
        if line.split('#', 1)[0].split(':', 1)[0].strip() == key:
            line = f'{key} : {value}'
            found = True
        lines.append(line)
    if not found:
        lines.append(f'{key} : {value}')
    return '\n'.join(lines) + '\n'


def cpu_seconds(value):
    days = 0
    if '-' in value:
        day, value = value.split('-', 1)
        days = int(day)
    values = [float(part) for part in value.split(':')]
    total = 0.0
    for value in values:
        total = total * 60.0 + value
    return total + days * 86400


def process_metrics(pid):
    if platform.system() == 'Windows':
        script = (f'$p=Get-Process -Id {int(pid)} -ErrorAction Stop; '
                  '[pscustomobject]@{cpu_seconds=$p.CPU; rss_bytes=$p.WorkingSet64; '
                  'threads=$p.Threads.Count} | ConvertTo-Json -Compress')
        metrics = json.loads(subprocess.check_output(
            ['powershell', '-NoProfile', '-NonInteractive', '-Command', script], text=True,
            creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0)))
        metrics['ps_cpu_percent'] = None
        return metrics
    values = subprocess.check_output(
        ['ps', '-p', str(pid), '-o', 'pid=,pcpu=,rss=,cputime='], text=True
    ).split()
    if len(values) != 4:
        raise RuntimeError(f'could not read process metrics: {values}')
    if platform.system() == 'Darwin':
        threads = len(subprocess.check_output(
            ['ps', '-M', '-p', str(pid), '-o', 'pid='], text=True
        ).splitlines())
    else:
        threads = int(subprocess.check_output(
            ['ps', '-p', str(pid), '-o', 'nlwp='], text=True
        ).strip())
    return {'cpu_seconds': cpu_seconds(values[3]), 'ps_cpu_percent': float(values[1]),
            'rss_bytes': int(values[2]) * 1024, 'threads': threads}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--sessions', type=int, choices=(1, 2, 16), default=1)
    parser.add_argument('--mode', choices=('video', 'paused', 'camera'), default='video')
    parser.add_argument('--input', type=Path)
    parser.add_argument('--window-size', help='logical preview dimensions, e.g.320x240; otherwise grid-derived')
    parser.add_argument('--config', type=Path)
    parser.add_argument('--field', action='append', default=[], metavar='KEY=VALUE',
                        help='literal config override, e.g. rust_resilient=n; no source files modified')
    parser.add_argument('--camera', type=int, default=0)
    parser.add_argument('--display', choices=('y', 'n'), default='y', help='exercise visible preview by default')
    parser.add_argument('--fps', type=float, default=10.0)
    parser.add_argument('--seconds', type=float, default=20.0)
    parser.add_argument('--warmup', type=float, default=3.0)
    args = parser.parse_args()
    if not (1 <= args.seconds <= 300 and 0 <= args.warmup <= 60):
        parser.error('seconds must be1..300 and warmup0..60')
    if not math.isfinite(args.fps) or not (0.1 <= args.fps <= 1000):
        parser.error('fps must be finite0.1..1000')
    if args.mode == 'camera' and args.sessions != 1:
        parser.error('camera case opens exactly one actual device')
    root = Path(__file__).resolve().parent.parent
    binary = args.binary.resolve()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    source = args.input.resolve() if args.input else root / 'examples/sample.mp4'
    config_source = args.config.resolve() if args.config else root / 'examples/benchmark/config.txt'
    text = config_source.read_text()
    for override in args.field:
        if '=' not in override or not override.split('=', 1)[0].strip():
            parser.error('--field requires a nonempty KEY=VALUE')
        key, value = override.split('=', 1)
        text = field(text, key.strip(), value)
    for key, value in [('src_fn', str(args.camera) if args.mode == 'camera' else str(source)),
                       ('src_fps', '-1' if args.mode == 'camera' else str(args.fps)),
                       ('sock_port', '-1'), ('zmq_port', '-1'), ('com_port', ''),
                       ('save_raw', 'n'), ('save_debug', 'n'), ('do_display', args.display), ('rust_instance_name', 'VR1')]:
        text = field(text, key, value)
    config = output / 'resource.txt'
    config.write_text(text)
    columns = 4 if args.sessions == 16 else args.sessions
    rows = 4 if args.sessions == 16 else 1
    error_file = (output / 'stderr.log').open('w')
    command = [str(binary), '--config', str(config), '--windows', str(args.sessions),
               '--grid', f'{columns}x{rows}', '--control-stdio', '--exit-after',
               str(args.seconds + args.warmup + 90)]
    if args.window_size:
        command.extend(['--window-size', args.window_size])
    process = subprocess.Popen(command, cwd=root, stdin=subprocess.PIPE,
                               stdout=subprocess.PIPE, stderr=error_file, text=True, bufsize=1)
    responses = queue.Queue(maxsize=16)
    def read_responses():
        for line in process.stdout:
            responses.put(line)
        responses.put(None)
    threading.Thread(target=read_responses, name='resource-probe-jsonl', daemon=True).start()
    trace = []

    def request(name, session=None, **arguments):
        message = {'id': len(trace) + 1, 'command': name, 'args': arguments}
        if session is not None:
            message['session'] = session
        process.stdin.write(json.dumps(message) + '\n')
        process.stdin.flush()
        try:
            line = responses.get(timeout=20)
        except queue.Empty as error:
            raise RuntimeError(f'native control response timed out: {message}') from error
        if line is None:
            raise RuntimeError(f'native control closed: {message}')
        response = json.loads(line)
        trace.append({'request': message, 'response': response})
        if not response.get('ok'):
            raise RuntimeError(response)
        return response['result']

    samples = []
    try:
        deadline = time.monotonic() + 30
        while True:
            sessions = request('sessions')
            if len(sessions) == args.sessions and all(s.get('worker_active') for s in sessions):
                break
            if time.monotonic() >= deadline or any(s['error'] for s in sessions):
                raise RuntimeError(f'native sessions failed to open: {sessions}')
            time.sleep(.1)
        # Native startup may intentionally be paused. Queue explicit play for
        # EVERY session and observe measured frames, not just queued responses.
        for session in sessions:
            request('start', session['session'])
        deadline = time.monotonic() + 30
        while True:
            sessions = request('sessions')
            if all(s['frames'] >= 3 and not s['paused'] for s in sessions):
                break
            if time.monotonic() >= deadline or any(s['error'] for s in sessions):
                raise RuntimeError(f'native sessions failed to play: {sessions}')
            time.sleep(.1)
        if args.mode == 'paused':
            for session in sessions:
                request('pause', session['session'])
            deadline = time.monotonic() + 10
            while True:
                sessions = request('sessions')
                if all(s['paused'] for s in sessions):
                    break
                if time.monotonic() >= deadline:
                    raise RuntimeError('pause did not complete')
                time.sleep(.05)
        time.sleep(args.warmup)
        before = request('sessions')
        start_metrics = process_metrics(process.pid)
        started = time.monotonic()
        while True:
            elapsed = time.monotonic() - started
            if elapsed >= args.seconds:
                break
            time.sleep(min(1.0, args.seconds - elapsed))
            status = request('sessions')
            if args.mode != 'paused' and any(s['ended'] for s in status):
                raise RuntimeError('input ended during active measurement; provide a longer fixture or reduce rate/duration')
            samples.append({'elapsed_seconds': time.monotonic() - started,
                            'process': process_metrics(process.pid), 'sessions': status})
        end_metrics = process_metrics(process.pid)
        elapsed = time.monotonic() - started
        after = request('sessions')
        deltas = [a['frames'] - b['frames'] for a, b in zip(after, before)]
        if args.mode == 'paused' and any(deltas):
            raise RuntimeError(f'paused source advanced: {deltas}')
        if args.mode != 'paused' and any(delta <= 0 for delta in deltas):
            raise RuntimeError(f'active source did not advance: {deltas}')
        result = {'binary': str(binary), 'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
                  'mode': args.mode, 'sessions': args.sessions, 'window_size': args.window_size, 'display': args.display,
                  'input': str(source) if args.mode != 'camera' else f'camera:{args.camera}',
                  'input_sha256': hashlib.sha256(source.read_bytes()).hexdigest() if args.mode != 'camera' else None,
                  'config_source': str(config_source), 'field_overrides': args.field, 'requested_source_fps': args.fps if args.mode != 'camera' else None,
                  'seconds': elapsed, 'warmup_seconds': args.warmup,
                  'application_cpu_seconds': end_metrics['cpu_seconds'] - start_metrics['cpu_seconds'],
                  'cpu_percent_one_core': 100 * (end_metrics['cpu_seconds'] - start_metrics['cpu_seconds']) / elapsed,
                  'rss_median_bytes': statistics.median(s['process']['rss_bytes'] for s in samples),
                  'rss_max_bytes': max(s['process']['rss_bytes'] for s in samples),
                  'threads_median': statistics.median(s['process']['threads'] for s in samples),
                  'threads_max': max(s['process']['threads'] for s in samples),
                  'frame_deltas': deltas, 'delivered_fps': [delta / elapsed for delta in deltas],
                  'before': before, 'after': after, 'samples': samples,
                  'platform': platform.platform(), 'timing_boundaries': __doc__,
                  'quality_scope': 'Sampled last_row states only; not a complete log or acceptance count.',
                  'scope': 'Measured process resources; no physical sphere accuracy or unsupported high-rate camera claim.'}
        (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
        request('close', 1, discard=True)
        process.stdin.close()
        if process.wait(timeout=15):
            raise RuntimeError('desktop closed with an error')
        print(json.dumps({key: value for key, value in result.items()
                          if key not in ('before', 'after', 'samples', 'timing_boundaries')}))
    finally:
        if process.poll() is None:
            process.terminate()
            process.wait(timeout=10)
        (output / 'transcript.json').write_text(json.dumps(trace, indent=2) + '\n')
        error_file.close()


if __name__ == '__main__':
    main()
