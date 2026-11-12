"""Does a realistic warm-up at load time remove Vulkan's one-time first-request pause?

Starts llama-server (Vulkan), sends one Explain-sized request as warm-up and times it, then times
the first token of 5 real Explain drafts. Run twice to see whether a second process start pays again.
usage: python -I -X utf8 vulkan_warmup.py <model.gguf>
"""
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))  # -I leaves the script's folder off the path
import bench  # noqa: E402

model = sys.argv[1]
for attempt in (1, 2):
    proc, base, ready = bench.start_llama("vulkan", model)
    c = bench.CASES[0]
    t = time.perf_counter()
    bench.chat(base, "local", [{"role": "system", "content": bench.DRAFT_SYSTEM},
                               {"role": "user", "content": c["before"] + " " + c["after"]},
                               {"role": "assistant", "content": "Warm "}], 8)
    warm = time.perf_counter() - t
    ttft = []
    for c in bench.CASES[1:6]:
        _, t1, _, _ = bench.chat(base, "local", [
            {"role": "system", "content": bench.DRAFT_SYSTEM},
            {"role": "user", "content": f'Explain only these words: "{c["term"]}"\n\nText just before them: {c["before"][-500:]}\n\nText just after them: {c["after"][:500]}'},
            {"role": "assistant", "content": c["term"] + " "}], 60, prefill=True)
        ttft.append(round(1000 * t1))
    proc.terminate(); proc.wait(); time.sleep(2)
    print(f"start {attempt}: ready {ready:.2f} s, realistic warm-up request {warm:.2f} s, then first-token ms {ttft}")
