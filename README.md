# Xenstride

Xenstride is an experimental Xbox Series compatibility project for Windows.
The v0.3.0 runtime is based on [VIctory](https://github.com/Marvisak/VIctory),
which provides the loader, platform services, D3D12 backend and shader
translator. Xenstride's changes include texture staging and readback, typed
descriptor views, shader rounding fixes, and reproducible GPU diagnostics.

- [v0.3.0 Experimental](https://github.com/mkr2tl/xenstride-public/releases/tag/runtime-v0.3.0) provides the Windows x64 build, corresponding source, checksums and benchmark evidence.
- [Build and reproduce](docs/RUNTIME.md), [status and limits](docs/STATUS.md), [changelog](CHANGELOG.md), and [provenance](PROVENANCE.md).
- The VIctory-based runtime uses GPL-3.0-or-later. [Component licences](LICENSE.md).
- [Standalone MIT tools](tools-mit/) cover format inspection and synthetic PE fixtures. The independent capture launcher ships as a separate MIT source archive.

Validation uses original authorized fixtures. Commercial game compatibility,
gameplay and in-game performance remain unverified. Development has used
Claude Code and OpenAI Codex. See provenance for the distinction between
VIctory, upstream fixes and Xenstride work.

[Discord](https://discord.gg/V7Ss2GYFZ5) · [TikTok](https://www.tiktok.com/@mkr_2tl)
