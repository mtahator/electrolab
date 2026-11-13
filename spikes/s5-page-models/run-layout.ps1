# Spike S5: every layout model on the same 9 fixture papers (first 20 pages each), 8 threads,
# then the fastest acceptable model again with 4 threads (leaving cores for the UI and the LLM).
$r = Resolve-Path "$PSScriptRoot\..\.."
$papers = "arxiv-2111.12066v2", "arxiv-2406.19831v2", "arxiv-2402.12652v3", "arxiv-2407.06664v2", "arxiv-2310.09367v1",
          "arxiv-2106.03711v2", "arxiv-2410.21481v1", "plos-pone-0232683", "naca-report-1135"
$pdfs = $papers | ForEach-Object { "$r\.fixtures\papers\$_.pdf" }
$py = @("run", "--no-project", "--python", "3.13", "--with", "onnxruntime", "--with", "numpy", "--with", "pypdfium2", "--with", "pillow",
        "python", "-I", "-X", "utf8", "$PSScriptRoot\layout.py")
foreach ($m in "heron", "heron-int8", "egret", "pp") {
  & uv @py $m 8 "$r\spikes\s2-pdfium\out" "$PSScriptRoot\out\layout-$m-t8.json" @pdfs 2>&1 | Where-Object { $_ -notmatch 'onnxruntime:|CleanUnused' }
}
foreach ($m in "heron-int8", "egret") {
  & uv @py $m 4 "$r\spikes\s2-pdfium\out" "$PSScriptRoot\out\layout-$m-t4.json" @pdfs 2>&1 | Where-Object { $_ -notmatch 'onnxruntime:|CleanUnused' }
}
