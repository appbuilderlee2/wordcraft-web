"""Finalize Trunk output with offline manifest and a generated iOS icon."""
from pathlib import Path
import json
import subprocess
from PIL import Image, ImageDraw
root = Path(__file__).resolve().parents[1]
out = root / "dist/web"
if not (out / "index.html").exists():
    raise SystemExit("Trunk output missing")
# The icon is generated from shapes, not an external font or trademark.
im = Image.new("RGB", (180, 180), "#2256c7")
d = ImageDraw.Draw(im)
d.rounded_rectangle((43, 25, 137, 155), radius=11, fill="white")
d.line([(62, 78),(78, 124),(90, 96),(102,124),(119,78)], fill="#2256c7", width=10, joint="curve")
im.save(out / "apple-touch-icon.png")
sha = subprocess.check_output(["git", "rev-parse", "--short", "HEAD"], cwd=root, text=True).strip()
assets = ["./", "./index.html", "./manifest.webmanifest", "./app-icon.svg", "./apple-touch-icon.png"]
assets += ["./"+p.name for p in out.iterdir() if p.is_file() and p.suffix in {".js",".wasm",".css"}]
sw = (out / "sw.js")
sw.write_text(sw.read_text().replace("__BUILD_VERSION__",sha).replace("__SHELL_FILES__",json.dumps(sorted(set(assets)))),encoding="utf-8")
print("Prepared offline app shell:", len(set(assets)), "assets, build", sha)
