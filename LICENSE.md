# Component licences

There is no blanket MIT licence for this repository.

- The v0.3.0 VIctory-based runtime and its corresponding source archive
  are GPL-3.0-or-later. The archive carries its own GPLv3 text and attribution.
- The independently distributed capture tools are MIT, with their own notice.

- `runtime/`: GPL-2.0-only, an XCL derivative; see runtime/LICENSE, runtime/THIRD_PARTY.md
  and runtime/XENSTRIDE_CHANGES.md (dated modification notices, GPL-2.0 section 2a).
- `xcl-gpl/`: GPL-2.0-only, derived from ivall/xcl; see its LICENSE and README.
- `tools-mit/`: MIT, with its existing attribution and provenance notices.
- Newly written project overview documentation: MIT, using the notice in tools-mit/LICENSE.

## Historical XCL runtime binaries

A release of `xcl.exe` is distributed only together with:

- the complete corresponding source: the source archive of the same tagged commit, attached to
  the same GitHub release (GPL-2.0 section 3(a)), and the locked Windows dependency-source archive;
- `LICENSE` (GPL-2.0), `THIRD_PARTY.md` (code ported from other projects) and
  `THIRD_PARTY_LICENSES.txt` (every Rust crate linked into the binary, its licence and notices,
  including the BSD-2-Clause notice of the bundled udis86 C library).

All linked crates are under MIT, Apache-2.0 OR MIT, BSD-2-Clause, Zlib, Unlicense OR MIT or
Unicode-3.0 terms (each with a GPL-2.0-compatible choice). Not distributed: DXC
(`dxcompiler.dll`, `dxil.dll`), the D3D12 Agility SDK, `opus.dll`, the Windows GDK runtime and
any other Microsoft component; users obtain them from their own system or Microsoft's packages.

Do not relabel an XCL derivative MIT. This is a redistribution inventory, not a legal opinion
or an assurance that every possible use of third-party game material is permitted.
