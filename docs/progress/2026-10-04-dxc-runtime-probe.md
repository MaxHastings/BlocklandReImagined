# 2026-10-04 DXC shaders are tested against the D3D12 runtime

The Windows package ships DXC 1.8.2502's `dxcompiler.dll` beside the game but
not `dxil.dll` (Microsoft's own licence terms). The concern: DXIL that is not
signed with the validator's hash is refused by a D3D12 runtime that does not
accept the unsigned ("bypass") form, which may include older Windows 10
inbox runtimes. A refused shader fails pipeline creation: a black screen.
Max's PC rendering proves nothing for other machines.

## What is known, and what is not

- wgpu-hal 30.0.1 (read from the crate source,
  `src/dx12/shader_compilation.rs`, `src/dx12/instance.rs`): `Auto` loads
  `dxcompiler.dll` by name, falling back to FXC (`d3dcompiler_47.dll`) when it
  is missing. `compile_dxc` passes the DXC output blob straight to
  `CreatePipelineState`; wgpu never calls `IDxcValidator` or touches the
  container hash. A runtime refusal surfaces as
  `PipelineError::Linkage` (`src/dx12/device.rs`), a device error, not a
  panic.
- The DXC v1.8.2502 release page
  (https://github.com/microsoft/DirectXShaderCompiler/releases/tag/v1.8.2502),
  read once through a summarising fetch, says the DXIL validator hash was
  open-sourced and validator binaries are no longer required for signed
  shaders. That suggests `dxcompiler.dll` alone emits correctly hashed
  containers that every runtime accepts.
- Not verified online: the exact hash DXC 1.8.2502 writes without
  `dxil.dll`, and which D3D12 runtime versions accept the bypass hash
  (reported as 1.615 / Agility SDK and developer mode, per
  https://github.com/microsoft/hlsl-specs/blob/main/proposals/infra/INF-0004-validator-hashing.md,
  not read). Web research stopped here by the coordinator's direction, so
  the code does not depend on the answer.

## Change

`crates/client/src/platform.rs`: every GPU open (the early worker-thread one
and the windowed one) goes through `open_device`. On DirectX 12, unless
`WGPU_DX12_COMPILER=fxc` asked for FXC, it builds one tiny render pipeline
(vertex and fragment shader) inside OutOfMemory, Validation and Internal error
scopes. Any error drops that device, instance and surface and opens them
again with `Dx12Compiler::Fxc`; the log says
`Shader compiler: FXC (DXC shaders rejected by this D3D12 runtime)` after a
warning naming the error. No Windows version table: the runtime itself
answers. D3D12 has no capability query for bypass-hash support that wgpu
exposes, so a pipeline build is the cheapest honest test (milliseconds with
DXC). A device whose FXC reopen fails falls through to the next backend
(Vulkan, then WARP) as before. Non-DX12 backends are untouched.

Unit tests cover the decision (`dx12_tests_dxc_unless_fxc_was_asked_for`),
the log line for each outcome, and that the probe pipeline itself passes on
a working GPU (so it never pushes healthy machines to FXC).

## Next

The fallback has only run where the probe passes. On a Windows 10 machine
without developer mode and with an old inbox D3D12 runtime, check the log
for which compiler line appears. If it says DXC and renders, the shipped
hash is accepted there too.
