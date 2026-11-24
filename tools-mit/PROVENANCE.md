# Provenance and inclusion decisions

Reviewed on 2026-11-23. The original release began without inherited research Git history.
This is now the MIT component of the parent Xenstride publication; XCL source
is separately licensed under ../xcl-gpl, outside this component.
No module, test file, binary, output log, database or fixture was copied from
the earlier unlicensed research tree. Implementations and synthetic fixture
builders were written for this release, informed by the earlier audit and public
format descriptions. This is not an independent clean-room process: the developer
had access to earlier source. It does not claim to settle ownership of that tree.

| Included group | Basis and redistribution decision |
| --- | --- |
| `src/xenstride/io.py`, CLI | Newly written standard-library code; MIT |
| `src/xenstride/pe.py` | New implementation of selected public PE/COFF fields; no Microsoft implementation code or specification extracts copied |
| `src/xenstride/dxbc.py` | New implementation of documented container fields; no LLVM/DXC implementation code copied |
| `src/xenstride/rds5.py` | New block/descriptor parser from public TTPF Viewer format description; reference is MIT, full reference license retained conservatively |
| `src/xenstride/rpf9.py` | New heuristic using the standard zlib format; no third-party RPF index/parser code copied |
| `src/xenstride/runtime.py` | Newly written; uses only documented Win32 calls (VirtualAlloc/VirtualProtect, Job Objects) through ctypes. No XCL (GPL) loader code, Lab code or other loader implementation was copied; XCL's documented loader defects only informed which checks happen before mapping |
| `tools/runtime_fixture.py` | x64 instruction bytes hand-assembled for this repository from the public instruction encodings, checked with a disassembler; no bytes taken from other binaries |
| `src/xenstride/tiling.py` | New generic bit-interleave utility; no Xbox/AMD pattern tables copied |
| Tests, tools, CI and docs | Newly written; synthetic data generated from literal fields, not extracted assets; MIT |

References consulted (facts used, not bundled source):

- Microsoft PE/COFF: https://learn.microsoft.com/en-us/windows/win32/debug/pe-format
- LLVM DirectX container: https://llvm.org/docs/DirectX/DXContainer.html
- TTPF Viewer format: https://github.com/paryzius/ttpf-viewer/blob/bafe9afcaafd037232b2f32fdc19bf8a725f52bf/README.md#the-format
- TTPF Viewer license: https://github.com/paryzius/ttpf-viewer/blob/bafe9afcaafd037232b2f32fdc19bf8a725f52bf/LICENSE
- zlib framing: RFC 1950, https://www.rfc-editor.org/rfc/rfc1950

No external program is a runtime dependency. Python and pip are supplied by the
user, not redistributed. The optional standard source builder is setuptools;
its code is not bundled. The offline wheel builder executes only repository code.
CI references GitHub's checkout/setup-python actions at immutable commit IDs.
The referenced LICENSE and action.yml files were checked in the official repositories:
checkout `11bd71901bbe5b1630ceea73d27597364c9af683` and setup-python
`a26af69be951a213d495a4c3e4e4022e16d87065`, both MIT. Their implementations are
not vendored, and no claim is made of a complete audit of their transitive dependencies.

Excluded from this MIT component: XCL and its GPL-derived code; all original modules without explicit
license grants; PS09, BC7 and shader translators pending file-level provenance
work; tiled texture reconstruction; vendor SDKs/DLLs; corpus data and outputs;
old Git history, personal logs, symbol files, dumps, caches and monitoring tools.
These exclusions do not authorize deletion or relicensing of the originals.

Publication layout update (2026-11-24): CI moved to the repository root; the
component release gate accepts an explicit enclosing history root. Original
tool/runtime source provenance remains unchanged.
