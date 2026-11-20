import json
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
package = Path(sys.argv[1])
metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--locked", "--format-version", "1", "--filter-platform", "x86_64-pc-windows-gnullvm"], cwd=ROOT))
records = []
nodes = {node['id']: node for node in metadata['resolve']['nodes']}
included = set()
pending = [metadata['resolve']['root']]
while pending:
    identifier = pending.pop()
    if identifier in included:
        continue
    included.add(identifier)
    pending.extend(dependency['pkg'] for dependency in nodes[identifier]['deps']
                   if any(kind['kind'] != 'dev' for kind in dependency['dep_kinds']))
for item in metadata["packages"]:
    if item["name"] == "paymerge-pdf" or item['id'] not in included:
        continue
    source = Path(item["manifest_path"]).parent
    destination = package / "licenses" / (item["name"]+"-"+item["version"])
    candidates = [p for p in source.iterdir() if p.is_file() and p.name.upper().startswith(("LICENSE", "LICENCE", "COPYING", "NOTICE"))]
    if item.get("license_file"):
        extra = source / item["license_file"]
        if extra.is_file() and extra not in candidates:
            candidates.append(extra)
    destination.mkdir(parents=True, exist_ok=True)
    for file in candidates:
        shutil.copyfile(file, destination / file.name)
    records.append({"name": item["name"], "version": item["version"], "license": item.get("license"), "repository": item.get("repository")})
pdfium = ROOT / ".tools/pdfium-win"
for name in ["LICENSE", "licenses"]:
    source = pdfium / name
    if source.is_dir():
        shutil.copytree(source, package / "licenses" / "PDFium-third-party", dirs_exist_ok=True)
    elif source.is_file():
        shutil.copyfile(source, package / "licenses" / ("PDFium-"+name))
(package / "licenses" / "dependencies.json").write_text(json.dumps(records, ensure_ascii=False, indent=2)+"\n", encoding="utf-8")
