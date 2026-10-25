"""Developer-only rendering of pinned Lucide SVG artwork; no runtime tool."""
import argparse
from pathlib import Path
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--resvg", default="resvg", help="resvg 0.45.1 executable")
args = parser.parse_args()
root = Path(__file__).resolve().parent.parent / "assets" / "lucide"
for source in sorted(root.glob("*.svg")):
    # Preserve original geometry; bake the tint base into the offline raster.
    svg = source.read_text().replace('stroke="currentColor"', 'stroke="#ffffff"')
    subprocess.run([args.resvg, "-w", "72", "-h", "72", "-", str(source.with_suffix(".png"))], input=svg.encode(), check=True)
