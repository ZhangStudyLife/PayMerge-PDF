"""Fetch the OFL font once; record its exact upstream revision and checksum."""
import hashlib
import json
from pathlib import Path
import subprocess
import urllib.request
import urllib.parse

ROOT = Path(__file__).resolve().parents[1]
LOCK = ROOT / "assets/font-source.json"


def get_json(url):
    request = urllib.request.Request(url, headers={"User-Agent": "PayMerge-PDF-setup"})
    with urllib.request.urlopen(request, timeout=30) as response:
        return json.load(response)


if LOCK.exists():
    source = json.loads(LOCK.read_text())
else:
    commit = get_json("https://api.github.com/repos/google/fonts/commits?path=ofl/notosanssc&per_page=1")[0]["sha"]
    source = {"repository": "https://github.com/google/fonts", "revision": commit}

for upstream, filename in [("NotoSansSC[wght].ttf", "NotoSansSC.ttf"), ("OFL.txt", "OFL-NotoSansSC.txt")]:
    target = ROOT / "assets" / filename
    expected = source.get(filename)
    if not target.exists() or (expected and hashlib.sha256(target.read_bytes()).hexdigest() != expected):
        url = "https://raw.githubusercontent.com/google/fonts/" + source["revision"] + "/ofl/notosanssc/" + urllib.parse.quote(upstream)
        subprocess.run(["curl", "--fail", "--location", "--retry", "3", "--connect-timeout", "20", "--max-time", "300", url, "-o", str(target)], check=True)
    digest = hashlib.sha256(target.read_bytes()).hexdigest()
    if expected and digest != expected:
        raise RuntimeError("Font checksum mismatch: " + filename)
    source[filename] = digest
LOCK.write_text(json.dumps(source, indent=2) + "\n")
