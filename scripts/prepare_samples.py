"""Read-only source access. All copies and manifests stay in one isolated run folder."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil

ROOT = Path(__file__).resolve().parents[1]
RUN = ROOT / ".tmp" / "acceptance-run"


def inventory(source):
    return {p.relative_to(source).as_posix(): {"size": p.stat().st_size, "sha256": hashlib.sha256(p.read_bytes()).hexdigest()}
            for p in sorted(source.rglob("*")) if p.is_file()}


def safe_run():
    resolved = RUN.resolve()
    expected = (ROOT / ".tmp").resolve()
    if resolved.parent != expected or RUN.is_symlink():
        raise RuntimeError("Refusing cleanup outside the isolated acceptance folder")
    return resolved


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("action", choices=["prepare", "cleanup"])
    parser.add_argument("source", nargs="?")
    args = parser.parse_args()
    run = safe_run()
    if args.action == "prepare":
        if not args.source:
            parser.error("source path is required")
        source = Path(args.source).resolve(strict=True)
        if source == run or source.is_relative_to(ROOT.resolve()) or run.is_relative_to(source):
            raise RuntimeError("Source must be outside the workspace and destination")
        if run.exists():
            raise RuntimeError("A previous acceptance folder exists; verify and clean it first")
        baseline = inventory(source)
        run.mkdir(parents=True)
        try:
            (run / "source-inventory.json").write_text(json.dumps({"source": str(source), "files": baseline}, ensure_ascii=False), encoding="utf-8")
            shutil.copytree(source, run / "samples")
            if inventory(run / "samples") != baseline:
                raise RuntimeError("Copied samples differ from the read-only source")
        except BaseException:
            shutil.rmtree(safe_run())
            raise
        print(f"Prepared {len(baseline)} verified copies in the isolated project folder")
    else:
        manifest = json.loads((run / "source-inventory.json").read_text(encoding="utf-8"))
        source = Path(manifest["source"])
        current = inventory(source)
        unchanged = current == manifest["files"]
        count = len(manifest["files"])
        shutil.rmtree(safe_run())
        summary = {"source_file_count": count, "source_hashes_unchanged": unchanged, "acceptance_folder_removed": not run.exists()}
        (ROOT / "docs" / "sample-integrity.json").write_text(json.dumps(summary, indent=2)+"\n", encoding="utf-8")
        print(json.dumps(summary))
        if not unchanged:
            raise RuntimeError("Source inventory changed; investigate before claiming verification")
