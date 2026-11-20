"""Server regression against isolated copies. Never opens the original sample directory."""
import ast
import csv
from decimal import Decimal
import json
import os
from pathlib import Path
import re
import subprocess
from typing import List, Optional

from PIL import Image, ImageDraw
import pypdfium2 as pdfium
from reportlab.pdfgen import canvas

ROOT = Path(__file__).resolve().parents[1]
RUN = ROOT / ".tmp/acceptance-run"
SAMPLES = RUN / "samples"
RUNNER = ROOT / "target/debug/examples/acceptance"


def command(*arguments, success=True):
    result = subprocess.run([str(RUNNER), *map(str, arguments)], capture_output=True, text=True)
    if success and result.returncode:
        raise RuntimeError(result.stderr)
    if not success and result.returncode == 0:
        raise AssertionError("Invalid input unexpectedly succeeded")
    return result


def compact(text):
    return re.sub(r"\s+", "", text).translate(str.maketrans("：￥（），", ":¥(),"))


def oracle(path):
    # Independent Poppler layout extraction. Values are verified against rendered contact sheets.
    text = subprocess.check_output(["pdftotext", "-layout", str(path), "-"], text=True)
    normalized = compact(text)
    number = re.search(r"发票号码:?([0-9]{8,20})", normalized)
    date = re.search(r"开票日期:?([0-9]{4})[-年/.]([0-9]{1,2})[-月/.]([0-9]{1,2})日?", normalized)
    amount = re.search(r"小写[):]*¥?(-?[0-9,]+\.[0-9]{2})", normalized)
    names = []
    for line in text.splitlines():
        line = compact(line)
        if "名称:" in line:
            name = line.rsplit("名称:", 1)[1]
            name = re.split(r"统一社会|纳税人|识别号", name)[0]
            if len(name) >= 4:
                names.append(name)
    return {
        "number": number[1] if number else None,
        "date": f"{int(date[1]):04}-{int(date[2]):02}-{int(date[3]):02}" if date else None,
        "amount": int(Decimal(amount[1].replace(",", ""))*100) if amount else None,
        "seller": names[-1] if names else None,
    }


def existing_extractor():
    tree = ast.parse((ROOT / "invoice_merger_v6.py").read_text(encoding="utf-8"))
    cls = next(node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == "SimpleInvoiceMergerV6")
    methods = [node for node in cls.body if isinstance(node, ast.FunctionDef) and node.name in {"extract_by_patterns", "extract_total_amount"}]
    namespace = {"re": re, "List": List, "Optional": Optional}
    exec(compile(ast.Module(body=methods, type_ignores=[]), "<original v6 helpers>", "exec"), namespace)
    return namespace


def recognized(field):
    return field.get("values") if field["status"] == "recognized" else None


def read_text(path):
    document = pdfium.PdfDocument(str(path))
    chunks = []
    try:
        for index in range(min(3, len(document))):
            page = document[index]
            text = page.get_textpage()
            chunks.append(text.get_text_range())
            text.close()
            page.close()
    finally:
        document.close()
    return "\n".join(chunks)


def inspect_pdf(path, expected_pages):
    info = json.loads(command("inspect", path).stdout)
    assert len(info["pages"]) == expected_pages
    for width, height in info["pages"]:
        assert abs(width-210/25.4*72) < 0.1 and abs(height-297/25.4*72) < 0.1


def synthetic():
    directory = RUN / "synthetic"
    directory.mkdir(exist_ok=True)
    purchase, payment = directory/"购买 记录.png", directory/"支付 记录.png"
    for path, color, label in [(purchase, (235, 64, 64), "PURCHASE"), (payment, (54, 105, 230), "PAYMENT")]:
        image = Image.new("RGB", (400, 700), color)
        ImageDraw.Draw(image).text((25, 30), label, fill="white", font_size=32)
        image.save(path)
    colors = [(0.25, 0.7, 0.4), (0.7, 0.35, 0.75), (0.95, 0.65, 0.2), (0.25, 0.7, 0.8)]
    for count in range(1, 5):
        source = directory/f"invoice-{count}.pdf"
        pdf = canvas.Canvas(str(source), pagesize=(600, 400))
        for i in range(count):
            pdf.setFillColorRGB(*colors[i])
            pdf.rect(0, 0, 600, 400, fill=1, stroke=0)
            pdf.setFillColorRGB(1, 1, 1)
            pdf.setFont("Helvetica", 55)
            pdf.drawString(40, 180, f"INVOICE PAGE {i+1}")
            pdf.showPage()
        pdf.save()
        output = directory/f"merged-{count}.pdf"
        command("merge", source, purchase, payment, output)
        inspect_pdf(output, count//2+1)
        document = pdfium.PdfDocument(str(output))
        for index in range(len(document)):
            page = document[index]
            bitmap = page.render(scale=1240/page.get_width())
            image = bitmap.to_pil().convert("RGB")
            image.save(directory/f"layout-{count}-{index+1}.png")
            page.close()
            bitmap.close()
            if index == len(document)-1:
                left, right = image.getpixel((350, 1250)), image.getpixel((900, 1250))
                assert left[0] > left[2]+100 and right[2] > right[0]+100
        document.close()
    invalid = directory/"invalid.pdf"
    invalid.write_bytes(b"not a pdf")
    bad_image = directory/"invalid.png"
    bad_image.write_bytes(b"not an image")
    protected_output = directory/"existing.pdf"
    protected_output.write_bytes(b"existing content")
    command("merge", invalid, purchase, payment, protected_output, "overwrite", success=False)
    command("merge", directory/"invoice-1.pdf", bad_image, payment, protected_output, "overwrite", success=False)
    assert protected_output.read_bytes() == b"existing content"
    command("merge", directory/"invoice-1.pdf", purchase, payment, protected_output, success=False)
    assert protected_output.read_bytes() == b"existing content"
    command("merge", directory/"invoice-1.pdf", purchase, payment, protected_output, "overwrite")
    inspect_pdf(protected_output, 1)
    command("merge", directory/"invoice-1.pdf", purchase, payment, directory/"missing/output.pdf", success=False)
    read_only = directory/"read-only"
    read_only.mkdir(exist_ok=True)
    read_only.chmod(0o500)
    try:
        command("merge", directory/"invoice-1.pdf", purchase, payment, read_only/"output.pdf", success=False)
    finally:
        read_only.chmod(0o700)
    assert not list(read_only.iterdir())
    print("synthetic layout, overwrite and failure regressions passed", flush=True)


def main():
    assert RUN.resolve().parent == (ROOT/".tmp").resolve()
    assert SAMPLES.is_dir()
    command("extract-dir", SAMPLES/"发票", RUN/"rust-fields.json")
    results = json.loads((RUN/"rust-fields.json").read_text())
    baseline = existing_extractor() if (ROOT / "invoice_merger_v6.py").is_file() else None
    counts = {field: {"correct": 0, "missing": 0, "wrong": 0} for field in ["number", "amount", "date", "seller"]}
    old_amount_correct = 0 if baseline else None
    old_seller_nonempty = 0 if baseline else None
    text_invoices = 0
    cases, failures = [], []
    contact = Image.new("RGB", (1600, ((len(results)+3)//4)*450), "#e8e8e8")
    for index, item in enumerate(results):
        path = SAMPLES/"发票"/item["file"]
        expected = oracle(path)
        text = read_text(path)
        data = item["data"]
        cases.append({"id": index+1, "file": item["file"], "expected": expected, "page_count": data["page_count"]})
        if text.strip():
            text_invoices += 1
            if baseline:
                old_amount = baseline["extract_total_amount"](None, text)
                old_amount_correct += old_amount != "未识别" and int(Decimal(str(old_amount))*100) == expected["amount"]
                seller = baseline["extract_by_patterns"](None, text, [r'销售方[：:\s]*([^\n\r]+?)(?:\s|纳税人)', r'卖方[：:\s]*([^\n\r]+?)(?:\s|纳税人)'])
                old_seller_nonempty += bool(seller and len(seller)>3)
            for field in counts:
                got = recognized(data[field])
                if expected[field] is None:
                    failures.append(f"I{index+1:02} independent oracle missing {field}")
                elif got is None:
                    counts[field]["missing"] += 1
                    failures.append(f"I{index+1:02} missing {field}")
                elif compact(str(got)) == compact(str(expected[field])):
                    counts[field]["correct"] += 1
                else:
                    counts[field]["wrong"] += 1
                    failures.append(f"I{index+1:02} mismatched {field}")
        else:
            assert not data["has_text"] and data["amount"]["status"] == data["number"]["status"] == "missing"
        doc = pdfium.PdfDocument(str(path))
        page = doc[0]
        bitmap = page.render(scale=390/page.get_width())
        image = bitmap.to_pil().convert("RGB")
        image.thumbnail((390, 415))
        x, y = (index%4)*400, (index//4)*450
        contact.paste(image, (x, y+24))
        ImageDraw.Draw(contact).text((x+8, y+4), f"I{index+1:02}", fill="black", font_size=18)
        bitmap.close()
        page.close()
        doc.close()
    contact.save(RUN/"invoice-contact.png")
    (RUN/"oracle.json").write_text(json.dumps(cases, ensure_ascii=False, indent=2), encoding="utf-8")
    summary = {"invoice_count": len(results), "text_invoice_count": text_invoices, "image_only_count": len(results)-text_invoices, "fields": counts, "v6_amount_correct": old_amount_correct, "v6_seller_nonempty": old_seller_nonempty, "failures": failures}
    (RUN/"field-summary.json").write_text(json.dumps(summary, ensure_ascii=False, indent=2), encoding="utf-8")
    print(json.dumps(summary, ensure_ascii=False), flush=True)
    if failures:
        raise AssertionError("Resolve recognition discrepancies before PDF acceptance")
    synthetic()
    def keyed(directory, marker):
        return {p.stem.split(marker, 1)[1]: p for p in directory.iterdir() if p.is_file() and marker in p.stem}
    invoices = keyed(SAMPLES/"发票", "_发票_")
    purchases = keyed(SAMPLES/"购买记录", "_购买记录_")
    payments = keyed(SAMPLES/"支付记录", "_支付记录_")
    keys = sorted(invoices.keys() & purchases.keys() & payments.keys())
    output = RUN/"real-merged"
    output.mkdir(exist_ok=True)
    for index, key in enumerate(keys):
        dest = output/f"case-{index+1:02}.pdf"
        source = pdfium.PdfDocument(str(invoices[key]))
        pages = len(source)//2+1
        source.close()
        command("merge", invoices[key], purchases[key], payments[key], dest)
        inspect_pdf(dest, pages)
        print(f"real merge {index+1}/{len(keys)} passed", flush=True)
    assert keys
    summary.update({"synthetic_page_cases": 4, "real_merge_cases": len(keys), "merge_failure_regressions_passed": True})
    (ROOT/"docs/server-acceptance.json").write_text(json.dumps(summary, ensure_ascii=False, indent=2)+"\n", encoding="utf-8")
    print("server acceptance complete", flush=True)


if __name__ == "__main__":
    main()
