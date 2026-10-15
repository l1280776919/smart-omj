"""Stage the CUDA 13 / cuDNN 9 runtime used by native Rust inference."""
import json
from pathlib import Path
import shutil
import torch
import onnxruntime

root = Path(__file__).resolve().parents[1]
if not torch.cuda.is_available() or not torch.version.cuda.startswith('13.'):
    raise RuntimeError('Install training/requirements-gpu.txt with the CUDA 13 PyTorch index first')
if 'CUDAExecutionProvider' not in onnxruntime.get_available_providers():
    raise RuntimeError('onnxruntime-gpu is required')
target = root / 'src-tauri/resources/gpu'
target.mkdir(parents=True, exist_ok=True)
sources = list((Path(onnxruntime.__file__).parent / 'capi').glob('*.dll'))
lib = Path(torch.__file__).parent / 'lib'
for pattern in ['cudart*.dll', 'cublas*.dll', 'cufft*.dll', 'cudnn*.dll', 'nvrtc*.dll', 'nvJitLink*.dll', 'curand*.dll', 'zlibwapi.dll']:
    sources.extend(lib.glob(pattern))
for source in sources:
    shutil.copy2(source, target / source.name)
(target / 'manifest.json').write_text(json.dumps(dict(torch=torch.__version__, cuda=torch.version.cuda,
    onnxruntime=onnxruntime.__version__, files=[p.name for p in sources]), indent=2), encoding='utf-8')
print(f'Staged {len(sources)} DLLs to {target}')
