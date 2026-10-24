"""Serial release-binary comparison; development tool, never an app dependency."""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import statistics
import subprocess
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--baseline', type=Path, required=True)
parser.add_argument('--candidate', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--runs', type=int, default=4, help='includes one discarded warmup')
args = parser.parse_args()
if not 3 <= args.runs <= 30:
    parser.error('--runs must be 3..30')
root = Path(__file__).resolve().parent.parent
args.output.mkdir(parents=True, exist_ok=False)
binaries = {'baseline': args.baseline.resolve(), 'candidate': args.candidate.resolve()}
input_path = root / 'examples/sample.mp4'
results = []
for iteration in range(args.runs):
    # No concurrent builds/tests. Alternate order to reduce ordering bias.
    order = list(binaries) if iteration % 2 == 0 else list(reversed(binaries))
    for label in order:
        directory = args.output.resolve() / f'{label}-{iteration}'
        started = time.perf_counter()
        completed = subprocess.run([
            str(binaries[label]), 'track', '--config', str(root / 'examples/benchmark/config.txt'),
            '--input', str(input_path), '--output', str(directory),
        ], cwd=root, capture_output=True, text=True, check=True)
        process_wall = time.perf_counter() - started
        (directory / 'console.log').write_text(completed.stdout + completed.stderr)
        summary = json.loads((directory / 'summary.json').read_text())
        if summary['frames'] != 300 or summary['rejected'] != 0:
            raise RuntimeError(f'incomplete or rejected sample: {directory}')
        subprocess.run([
            str(binaries['candidate']), 'compare', '--reference', str(root / 'examples/benchmark/upstream.dat'),
            '--candidate', str(directory / 'tracking.dat'), '--report', str(directory / 'comparison.json'),
        ], cwd=root, capture_output=True, text=True, check=True)
        results.append({'binary': label, 'iteration': iteration, 'warmup': iteration == 0,
                        'process_wall_seconds': process_wall, 'summary': summary})
        print(json.dumps({'binary': label, 'iteration': iteration,
                          'fps': summary['end_to_end_fps']}), flush=True)
    before = args.output / f'baseline-{iteration}' / 'all-frames.dat'
    after = args.output / f'candidate-{iteration}' / 'all-frames.dat'
    if before.read_bytes() != after.read_bytes():
        raise RuntimeError('all-frame numerical output changed; inspect before claiming an exact optimization')
report = {
    'platform': platform.platform(), 'machine': platform.machine(),
    'input_sha256': hashlib.sha256(input_path.read_bytes()).hexdigest(),
    'all_frame_numeric_output_identical': True,
    'medians': {label: {field: statistics.median(r['summary'][field] for r in results
                 if r['binary'] == label and not r['warmup'])
                 for field in ['wall_seconds', 'end_to_end_fps', 'tracking_fps']}
                for label in binaries},
    'runs': results,
}
(args.output / 'report.json').write_text(json.dumps(report, indent=2))
print(json.dumps(report['medians'], indent=2))
