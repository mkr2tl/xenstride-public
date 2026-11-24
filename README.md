# Xenstride

Xbox Series-to-Windows compatibility research, built around [XCL](https://github.com/ivall/xcl).
Our long-term target includes GTA VI. Gameplay is not demonstrated by this release.

This repository contains standalone binary-format tools, a small native PE test loader,
and selected XCL-derived pipeline scheduling changes with regression tests. It does not
contain a complete XCL runtime, game files, console SDKs or a ready-to-play PC port.

## Work available for review

- [MIT tools](tools-mit/): bounded PE, DXBC, RDS5 and RPF9 inspection; an owned-fixture PE loader.
- [GPL XCL contributions](xcl-gpl/): priority handling for pipeline build workers and
  state-sensitive prebuild keys, with source snapshots and patches.
- [Evidence and limits](docs/STATUS.md), [provenance](PROVENANCE.md), [milestones](docs/HISTORY.md).

The public tools are independent of XCL. The compatibility runtime depends on XCL's
loader, Xbox services, GPU translation and contributors; those are not Xenstride inventions.
Development has used Claude Code and OpenAI Codex. AI assistance is disclosed, and
implementation, test results and title observations are recorded separately.

## Build and evaluate

See [tools installation and tests](tools-mit/README.md). Python 3.11+ is required;
the format tools need no SDK or game files. The native PE fixtures require Windows x64.
The GPL directory is a reviewable module contribution, not a standalone runtime build.
Use only programs and inputs you are authorised to use.

## Community

[Discord](https://discord.gg/V7Ss2GYFZ5) | [TikTok](https://www.tiktok.com/@mkr_2tl) | [GitHub](https://github.com/mkr2tl/xenstride-public)

MIT applies to `tools-mit/`. GPL-2.0-only applies to `xcl-gpl/`; keep the component notices.
No affiliation with Rockstar Games, Take-Two Interactive or Microsoft.
