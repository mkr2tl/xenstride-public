# XENSTRIDE

Static binary-format tools for Xbox Series-to-Windows compatibility research.
Version 0.1 inventories PE32+ structures, DirectX container parts, RDS5/ETPF
metadata and bounded RPF9 zlib stream candidates. It does not load or run games.
An opt-in Windows x64 runtime prototype maps a minimal owned PE32+ test image and
calls its entry point in a contained child process (see below).

## Install and test

From the repository root, enter `tools-mit` before running these commands.

Python 3.11+ and pip; no runtime dependencies, SDK, GPU or game files required.
In Windows PowerShell:

```powershell
python -m venv .venv
python -B tools/build_wheel.py dist
.\.venv\Scripts\python.exe -m pip install --no-index --no-deps dist/xenstride-0.1.0-py3-none-any.whl
.\.venv\Scripts\python.exe -I -B -m unittest discover -s tests -v
.\.venv\Scripts\python.exe -I -B -m xenstride --help
```

On Linux, use `.venv/bin/python` instead. The offline builder needs only Python's
standard library. The optional setuptools source-build route has not been tested.

Inspect an authorised file with `python -I -B -m xenstride FORMAT FILE`, using
the venv's Python. Formats: `pe`, `dxbc`, `rds5` (optional `--etpf`), `rpf9`.
RPF9 inspection scans at most 1 MiB by default; it is not a full archive decoder.

## Runtime prototype (Windows x64)

`xenstride.runtime` loads a minimal PE32+ subset and calls its entry point as real
x64 code. It runs only in a child Python process inside a Job Object, with a
timeout. The subset is: no TLS, delay-load or unwind data; DIR64 relocations;
page-aligned sections; imports only from a fixed allowlist of five side-effect-free
`kernel32.dll` functions (`MulDiv`, `GetCurrentProcessId`, `SetLastError`,
`GetLastError`, `lstrlenA`), by name. They are bound with `GetModuleHandleW` and
`GetProcAddress` on modules already loaded in the child. `LoadLibrary` is never
called, and each address must come from a module in the Windows system directory.
Ordinal, bound, unknown or misplaced imports are refused. Anything else is refused
before a process starts. The
fixtures in `tools/runtime_fixture.py` contain a few hand-assembled instructions
written for this repository. Run the checked demonstration with the venv's Python:

```powershell
.\.venv\Scripts\python.exe -B tools/runtime_demo.py
```

It computes `(K + 5) * 7` natively at the preferred base and after a forced
relocation. It shows write and execute faults on read-only and NX sections, a
killed infinite loop, and a refused TLS image. An import fixture calls the five
allowlisted functions through its IAT after relocation. Each result is checked:
`MulDiv` against an independent model, `GetCurrentProcessId` against the child
PID, `SetLastError`/`GetLastError` round trip, and `lstrlenA` on a relocated
pointer. Forbidden imports are refused. Exit status 0 means every check
passed. Without `--execute`, `python -m xenstride.runtime FILE` only prints the
mapping plan. Execute only images you built yourself: the job contains a test, it
is not a security sandbox. Details: [technical scope](docs/TECHNICAL.md#runtime-prototype-xenstrideruntime-windows-x64).

Input is opened read-only. JSON goes to stdout; diagnostics go to stderr.
No Xbox binary, game asset, shader code, XCL launcher or vendor SDK is included.
Passing structural checks proves neither authenticity nor runtime compatibility.

See [technical scope](docs/TECHNICAL.md), [provenance](PROVENANCE.md), and
[validation](docs/VALIDATION.md). Development was AI-assisted; tests and documented
limits take precedence over generated claims. License: MIT, with reference notice
in `LICENSES/`.
