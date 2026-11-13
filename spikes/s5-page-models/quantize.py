"""Make an int8 (dynamic quantisation) copy of a layout model and print the Egret model's signature."""
import sys
from pathlib import Path

import onnxruntime as ort
from onnxruntime.quantization import QuantType, quantize_dynamic

store = Path(sys.argv[1])
src, dst = store / "heron" / "model.onnx", store / "heron" / "model.int8.onnx"
if not dst.exists():
    quantize_dynamic(str(src), str(dst), weight_type=QuantType.QInt8)
print(f"heron int8: {dst.stat().st_size / 1e6:.1f} MB (from {src.stat().st_size / 1e6:.1f} MB)")
o = ort.SessionOptions()
o.log_severity_level = 3
s = ort.InferenceSession(str(store / "egret-medium" / "model.onnx"), o, providers=["CPUExecutionProvider"])
for i in s.get_inputs():
    print("egret in ", i.name, i.shape, i.type)
for x in s.get_outputs():
    print("egret out", x.name, x.shape, x.type)
