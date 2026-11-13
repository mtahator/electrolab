"""Print input/output names, shapes and types of each ONNX model in the store."""
import sys
from pathlib import Path

import onnxruntime as ort

store = Path(sys.argv[1])
print("onnxruntime", ort.__version__, ort.get_available_providers())
for f in sorted(store.glob("*/*.onnx")):
    s = ort.InferenceSession(str(f), providers=["CPUExecutionProvider"])
    print(f"== {f.parent.name}/{f.name}")
    for i in s.get_inputs():
        print("   in ", i.name, i.shape, i.type)
    for o in s.get_outputs():
        print("   out", o.name, o.shape, o.type)
