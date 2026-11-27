# Xenstride

[![Pre-release](https://img.shields.io/github/v/release/mkr2tl/xenstride-public?include_prereleases)](https://github.com/mkr2tl/xenstride-public/releases) [![CI](https://github.com/mkr2tl/xenstride-public/actions/workflows/tests.yml/badge.svg)](https://github.com/mkr2tl/xenstride-public/actions) [![Issues](https://img.shields.io/github/issues/mkr2tl/xenstride-public)](https://github.com/mkr2tl/xenstride-public/issues)

Xenstride is an experimental Xbox Series compatibility runtime for Windows x64, built on [fxceless/VIctory](https://github.com/fxceless/victory-fork) and [VIctory](https://github.com/Marvisak/VIctory). Its V21 line focuses on AMD shader compiler compatibility, compute pipeline preparation, GPU memory budgeting and CPU submission overhead.

**[runtime-v0.4.0 ? experimental prerelease](https://github.com/mkr2tl/xenstride-public/releases/tag/runtime-v0.4.0)** includes a table-free Windows binary, corresponding source, locked dependency sources and SHA-256 checksums. No game files, private BCPack tables, shaders, caches or saves are supplied.

## V21 work

- Correct shader optimiser reuse of view-size queries within their valid block and query scope, addressing AMD compiler failures.
- Prewarm compute pipelines before execution, using the same translation settings and caches as the run.
- Scale copy-memory defaults with the GPU budget and record presentation timing.
- Watch GPU queue progress, write durable stall diagnostics and terminate a stalled run.
- Validate a Fast CPU configuration combining eight inherited submission and copy settings, with synthetic GPU tests for further investigation.

[Release notes](docs/releases/runtime-v0.4.0.md) distinguish Xenstride's commits from upstream work. Asynchronous submission, copyback and the underlying loader, graphics backend and shader translator include substantial upstream contributions.

## Validation

| Build / setup | Result |
| --- | --- |
| Public rebuilt binary | Locked offline release build; local workspace suite: **691 passed, 0 failed, 44 ignored**; two private-table tests passed separately |
| Historical internal V21 Fast CPU | Ryzen 7 5800X3D, RX 9060 XT 16 GB, 32 GB RAM; about eight minutes interactive, RT off, no reported crash |
| Other GPUs and operating systems | Not validated by Xenstride for this release |

The historical capture reports **5.78 Present calls/s**, reaching **6.33** in a later interval, compared with **4.87** for V20 Default. These are presentation-call rates, not verified unique rendered-world FPS. Scenes and caches differed; this was not a controlled A/B benchmark. The public binary was rebuilt without embedded tables and **has no interactive performance result of its own**.

Graphical defects remain. Neither the short session nor automated tests establish general compatibility or long-session stability. A clean Windows installation and interactive testing of the public rebuild remain to be validated.

## Install and run

1. Download the Windows x64 archive and `SHA256SUMS.txt` from the [release](https://github.com/mkr2tl/xenstride-public/releases/tag/runtime-v0.4.0). Verify the archive with `Get-FileHash -Algorithm SHA256`, then extract it.
2. Use Windows x64 with a compatible D3D12 GPU and driver, the Microsoft Visual C++ x64 runtime, and Windows D3D12/DXGI/D3DCompiler components. The archive does not bundle Microsoft DLLs. Optional shader validation and title-specific imports may require additional components.
3. Copy and edit the supplied configuration. Use only executable files and supporting data you are authorised to use.

```powershell
Copy-Item '.\victory.example.toml' '.\victory.toml'
# Edit victory.toml for your authorised files and private cache/data folders.
.\victory-v20.exe config
$env:VICTORY_BVH_BUILDER = '0'
# Required only when your workload needs BCPack; provide your own authorised file.
$env:VICTORY_BCPACK_TABLES = 'C:\YourPrivateData\bcpack-tables.bin'
.\victory-v20.exe run 'C:\YourGame\program.exe' --execute
```

The executable keeps its historical filename for driver-cache continuity. For compute prewarming, use your own private shader containers with the same runtime settings and cache configuration:

```powershell
.\victory-v20.exe prewarm 'C:\YourPrivateData\shader-containers' --jobs 4
```

[Build instructions](docs/RUNTIME.md) explain the separate source archive. The `runtime/` directory in this Git repository is the historical XCL workspace, **not the v0.4.0 source**.

## Contribute

Report issues with the exact runtime hash, Windows and driver versions, hardware, settings, reproduction steps and a short redacted log. State whether measurements cover loading, menus or gameplay, how long the run lasted and whether caches were warm. Do not attach game executables, extracted shaders, saves, credentials or personal paths.

Small, focused pull requests with synthetic reproductions and clear provenance are welcome. [Issues](https://github.com/mkr2tl/xenstride-public/issues) ? [Discord](https://discord.gg/V7Ss2GYFZ5)

## Credits and licences

VIctory by Marvisak and contributors supplies the runtime foundation; fxceless and contributors provide the upstream fork used by V21. The source preserves upstream notices and credits, including myokex, Spexivus, joroyt, tightfall and Joshua Odom. See [provenance](PROVENANCE.md).

The current runtime is **GPL-3.0-or-later**. Earlier XCL-derived components remain **GPL-2.0-only**; independent tools retain their existing licences. [Component licences](LICENSE.md). BCPack tables are external private input and are not relicensed or distributed. Development used Claude Code and OpenAI Codex.

No affiliation with Microsoft, Rockstar Games or Take-Two Interactive.
