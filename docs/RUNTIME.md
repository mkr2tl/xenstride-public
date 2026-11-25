# Building and changing the runtime

`runtime/` is a complete Cargo workspace (GPL-2.0-only): XCL at upstream `9aec53d` plus the
Xenstride changes in [runtime/XENSTRIDE_CHANGES.md](../runtime/XENSTRIDE_CHANGES.md).
Upstream's own guide, [runtime/README.md](../runtime/README.md), describes the runtime's features,
commands and data folders; this page lists what differs or matters for Xenstride builds.

## Requirements

- Windows 10/11 x64.
- Rust 1.100 or newer, `stable-x86_64-pc-windows-msvc` (releases are built with 1.100.0).
- Visual Studio 2022 Build Tools with the C++ x64 tools (the `libudis86-sys` crate compiles C code).
- To run titles or the GPU tests: DXC (`dxcompiler.dll` + `dxil.dll`) from a Windows SDK or a
  [DirectXShaderCompiler release](https://github.com/microsoft/DirectXShaderCompiler/releases)
  (1.8 or newer recommended; `XCL_DXC_DIR` points at its `bin\x64`); a D3D12 GPU with Shader Model 6.6
  and mesh shaders; on Windows 10, the D3D12 Agility SDK (see runtime/README.md).

## Build

```powershell
cd runtime
cargo build --workspace --release --locked   # target\release\xcl.exe
```

`--locked` builds the exact dependency versions of `Cargo.lock`, those of the published binaries.
The distributed Windows build uses the release profile with debug info disabled,
symbols stripped, and local source and user paths remapped. The build recipe is
below. Run it from the runtime workspace in a fresh PowerShell process.

```powershell
$source = (Get-Location).ProviderPath
$env:CARGO_PROFILE_RELEASE_DEBUG = '0'
$env:CARGO_PROFILE_RELEASE_STRIP = 'symbols'
$pathMap = @(
    "--remap-path-prefix=$source=xenstride/runtime"
    "--remap-path-prefix=$($source.Replace('\','/'))=xenstride/runtime"
    "--remap-path-prefix=$env:USERPROFILE=/build-user"
    "--remap-path-prefix=$($env:USERPROFILE.Replace('\','/'))=/build-user"
)
$env:CARGO_ENCODED_RUSTFLAGS = $pathMap -join [char]31
cargo +1.100.0 build --release --locked -p xcl --bin xcl
```

The `lab` profile (`--profile lab`) is release with full debug info, as used for development.

## Tests

```powershell
cd runtime
cargo build --workspace --release --locked               # the test title the end-to-end tests run
cargo test -p xcl --release --locked --lib -- --test-threads=1
cargo test -p xcl --release --locked --tests             # library + integration suites
```

- **Tests never use a physical GPU unless asked.** Every test device is created on WARP (Microsoft's
  software renderer), and every integration test binary sets `XCL_ADAPTER=warp` before it starts.
  `XCL_TEST_HARDWARE_GPU=1` lets the tests use the default adapter.
- `XCL_GPU_AUDIT_DIR=<folder>` makes each integration test binary write, at exit, the D3D12 user-mode
  drivers it loaded (`d3d10warp.dll` = WARP) to `<folder>\integration-tests.tsv`.
- The runtime CI runs the library suite on WARP. Some end-to-end paths need
  physical-GPU capabilities such as wave64 and 4 GiB sparse windows; read the
  skip messages before interpreting a pass.
- Benchmarks are `#[ignore]` tests, e.g.
  `cargo test -p xcl --release --lib p14_bench -- --ignored --nocapture`.

## Runtime switches added or used by Xenstride

| Variable | Effect |
| --- | --- |
| `XCL_ACTIVITY=1` | every 2 s: presents, submits, draws, dispatches, and a memory line (process commit, local / non-local video memory, render-target pages, arenas, sparse pools, pipelines built and built ahead) |
| `XCL_PSO_PRELOAD_COMPUTE=0` | compute pipelines are no longer built ahead of their first bind |
| `XCL_PSO_PRELOAD=0` | nothing is built ahead (neither an earlier run's pipelines nor compute ones) |
| `XCL_THREAD_DUMP_AT=s1,s2` | where every thread is at those seconds |
| `XCL_ADAPTER=warp` / `radeon` / `1` | the D3D12 adapter |

Pipelines a run used are remembered in `%LOCALAPPDATA%\xcl\dxil\used-pipelines-v2.txt` (keyed by
shaders and pipeline state) and built ahead in later runs; the driver's compiled compute pipelines
are kept in `%LOCALAPPDATA%\xcl\dxil\driver\`.

## Contributing

Keep changes small and tested; a change to `crates/xcl/src` needs a test that fails without it.
Add a dated notice to any file you change (GPL-2.0 section 2a) and keep `Cargo.lock` committed.
Do not add game files, dumps, logs, keys, SDK files or binaries. Before a release, the
repository's checks (`python -B scripts/check_publication.py --require-history`) must pass.
