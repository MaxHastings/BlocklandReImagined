"""Run the same model compiler the legacy and glTF importers use."""
import json
import os
import pathlib
import subprocess
from functools import cache

ROOT = pathlib.Path(__file__).resolve().parent.parent


@cache
def compiler():
    subprocess.run(['cargo', 'build', '--locked', '-p', 'bri-gltf', '--bin',
                    'bri-compile-model'], cwd=ROOT, check=True)
    return ROOT / 'target' / 'debug' / ('bri-compile-model.exe' if os.name == 'nt' else 'bri-compile-model')


def compile_model(shape):
    data = json.dumps(shape, separators=(',', ':')).encode()
    return subprocess.run([compiler()], cwd=ROOT, input=data,
                          stdout=subprocess.PIPE, check=True).stdout
