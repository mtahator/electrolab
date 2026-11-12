"""Summarise spike S3 runs (out/*.json) into out/report.md."""
import json
from pathlib import Path

out = Path(__file__).parent / "out"
runs = []
for f in sorted(out.glob("*.json")):
    if f.stem == "smoke":
        continue
    r = json.loads(f.read_text("utf-8"))
    if "explain" in r:
        runs.append(r)
vllm_ready = (out / "vllm-ready.txt").read_text().strip() if (out / "vllm-ready.txt").exists() else None

L = []
p = L.append
p("## Speed and start-up\n")
p("| backend | model | ready s (1st / 2nd start) | VRAM MB | first token ms (median / p95) | tokens/s | Explain draft s | check s | cases |")
p("|---|---|---|---|---|---|---|---|---|")
for r in runs:
    e = r["explain"]
    ready = " / ".join(str(x) for x in r.get("ready_s", [])) or (vllm_ready or "?")
    p(f"| {r['backend']} | {r['model']} | {ready} | {r.get('vram_mb_loaded', '—')} | {e['ttft_ms_median']} / {e['ttft_ms_p95']} | "
      f"{e['tokens_per_s_median']} | {e['draft_s_median']} | {e['check_s_median']} | {e['cases']} |")

p("\n## Explain quality (all 54 cases; same prompts as ElectroChat 2.5)\n")
p("| backend | model | draft correct % | after check % | check said OK / corrected / unchecked | style problems % | JSON valid: schema / free |")
p("|---|---|---|---|---|---|---|")
for r in runs:
    e = r["explain"]
    if e["cases"] < 54:
        continue
    v = e["verdicts"]
    p(f"| {r['backend']} | {r['model']} | {e['draft_correct_pct']} | {e['final_correct_pct']} | {v['ok']} / {v['corrected']} / {v['unchecked']} | "
      f"{e['style_problems_pct']} | {r['json_schema_valid']}/{r['json_tests']} / {r['json_free_valid']}/{r['json_tests']} |")

p("\n\"Correct\" = the explanation contains the paper's own long form of the abbreviation (≥ 80% of its words). "
  "\"After check\" = the text shown to the reader after ElectroChat's check step (draft if the check said OK, else the correction).")
text = "\n".join(L)
(out / "report.md").write_text(text + "\n", "utf-8", newline="\n")
print(text)
