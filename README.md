# Xenstride

Xbox Series-to-Windows compatibility research, built around [XCL](https://github.com/ivall/xcl).
Gameplay is not demonstrated by this release.

This repository contains the complete, buildable Xenstride runtime (an XCL derivative,
GPL-2.0-only), standalone binary-format tools, a small native PE test loader, and earlier
reviewed XCL module contributions. It contains no game files, console SDKs, keys or a
ready-to-play PC port.

## Contents

- [Runtime](runtime/) (GPL-2.0-only): XCL at upstream `9aec53d` plus the Xenstride changes listed in
  [runtime/XENSTRIDE_CHANGES.md](runtime/XENSTRIDE_CHANGES.md). Build it with
  [docs/RUNTIME.md](docs/RUNTIME.md); test a release build with [docs/TESTERS.md](docs/TESTERS.md).
- [MIT tools](tools-mit/): bounded PE, DXBC, RDS5 and RPF9 inspection; an owned-fixture PE loader.
- [Earlier GPL module contributions](xcl-gpl/): the pipeline-scheduling snapshots and patches of the
  previous publication. The same changes are part of `runtime/`.
- [Evidence and known limitations](docs/STATUS.md), [provenance](PROVENANCE.md), [milestones](docs/HISTORY.md).

The runtime is XCL's work first: its loader, Xbox services, GPU translation and contributors are
not Xenstride inventions. Xenstride adds scheduling, memory, diagnostics and test changes on top.
Development has used Claude Code and OpenAI Codex. AI assistance is disclosed, and
implementation, test results and title observations are recorded separately.

## Build and evaluate

- Runtime: Windows 10/11 x64, Rust 1.100 (MSVC), Visual Studio Build Tools; see [docs/RUNTIME.md](docs/RUNTIME.md).
- Tools: Python 3.11+, no SDK or game files; see [tools-mit/README.md](tools-mit/README.md).
- Release builds (`xcl.exe`) are published as GitHub release assets together with the exact source
  archive they were built from and all licence notices; see [docs/TESTERS.md](docs/TESTERS.md).

Use only programs and inputs you are authorised to use. The runtime does not download, decrypt
or unpack game packages and contains nothing for circumventing DRM, licence or account checks.

## Community

[Discord](https://discord.gg/V7Ss2GYFZ5) | [TikTok](https://www.tiktok.com/@mkr_2tl) | [GitHub](https://github.com/mkr2tl/xenstride-public)

GPL-2.0-only applies to `runtime/` and `xcl-gpl/`; MIT applies to `tools-mit/`. Keep the component notices.
No affiliation with Rockstar Games, Take-Two Interactive or Microsoft.
