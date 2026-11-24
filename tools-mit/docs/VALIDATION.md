# Validation

Reviewed locally on 2026-11-23. Only owned synthetic inputs were used; no Xbox
image, shader, GPU operation or private corpus test was run.

| Windows interpreter | Discovered | Passed | Skipped | Failures/errors |
| --- | ---: | ---: | ---: | ---: |
| CPython 3.12.10 | 73 | 72 | 1 | 0 |
| CPython 3.14.0 | 73 | 72 | 1 | 0 |

The initial release had 60 tests. The runtime prototype adds 13: seven planning,
layout and refusal tests that run on any OS, and six native tests that run only on
64-bit Windows.

## Runtime prototype evidence

On Windows, the native tests run the hand-assembled fixture entry as real x64 code
in a Job Object child. They check the following:
- The returned value and the value stored in `.data` equal `((K + a) * b) mod 2**64`,
  computed independently in Python. This holds at the preferred base and after a
  forced relocation with two DIR64 fixes, including a wrapping case.
- A write to `.rdata` faults at RVA 0x2000.
- A jump into `.data` faults at RVA 0x3000 (NX).
- An infinite loop is killed by the job at the 2 s timeout.
- The job refuses a grandchild process, while the same command runs outside the job.
- Every run counted exactly one process in the job and none left afterwards.
- An image is refused before any process is created.

Control: with relocation writes disabled in a copy of `runtime.py`, the relocated
native cases faulted and the layout test failed. Microsoft `dumpbin` disassembled
the fixture to the intended instructions and listed the two DIR64 relocations.

Three problems were found and fixed during development:
- A console child brought `conhost.exe` into the job.
- A venv launcher would have started the real interpreter outside the job.
- In one Windows CI run, job accounting still counted an exited child as active.
  It is now polled, with a bound, until the count reaches zero.

`tools/runtime_demo.py` passes 10 of 10 automatic checks. The images are only the
repository's fixtures. No game, Xbox or third-party binary was executed.

Import resolution (2026-11-23, Windows 11, Python 3.12, local):
- An owned fixture imports five `kernel32.dll` functions by name. It calls them through its IAT, using `call [rip+disp32]`, after a forced relocation.
- Observed results:
  - `MulDiv(5,7,3)` = 12, equal to the independent model;
  - `GetCurrentProcessId` = the child PID reported by the parent;
  - `GetLastError` returns `0x0BADC0DE` after `SetLastError(0x0BADC0DE)`;
  - `lstrlenA` = 27 on a DIR64-relocated pointer.
- All five addresses resolved inside `KERNEL32.DLL` in the system directory, and `.idata` is read-only after binding.
- Refused before any process starts:
  - DLLs outside the allowlist, functions outside it (`CreateProcessW`, `LoadLibraryA`, `VirtualProtect`, `ExitProcess`), and wrong-case names;
  - ordinals;
  - bound descriptors, and a bound-import directory;
  - IAT slots in code or outside the image;
  - unterminated or out-of-range names.
- Checked in process:
  - an absent export is refused;
  - a DLL that is not loaded is refused, and the test checks that it is still not loaded afterwards.
- `unittest`: 84 tests, 83 passed, 1 skipped (the symlink test above).
- Limits:
  - The PE import table has no prototypes. Prototypes are fixed by the allowlist, not verified against the image.
  - Five functions only: no general Win32 or Xbox service layer.
  - No delay-load, TLS, unwind or CFG support.

The README build/install/test/help sequence succeeded in two new source copies,
with a fresh `.venv` and `dist` in each. No activation, SDK, package download,
external source tree or runtime dependency was needed. Installed CLI tests run
outside the checkout with Python isolated mode and exercise all four formats.
The release gate also passes after these local build directories exist.

The skipped test attempts to create a symbolic link, which this Windows user
cannot do. It is not counted as a pass; actual symlink containment is unvalidated
here. Linux and Python 3.11 are configured in CI but have not run locally.

The suite covers bounds, truncated/malformed structures, duplicate DXBC tags,
RDS5 following-codec semantics, address bits, ETPF upper bounds, bounded zlib,
read budgets/coverage and input immutability. A seeded mutation test exercises
240 PE/DXBC mutations; this is not a fuzzing campaign or security certification.
The tile test has nonuniform input and a wrong-pattern negative control.
Two same-toolchain wheel builds agree byte-for-byte and RECORD hashes validate;
this is not a guarantee across Python/zlib versions. The optional setuptools
source-build route remains untested locally because package downloads are blocked.

[Technical scope](TECHNICAL.md#comparison-with-the-earlier-research-tools)
records differential checks against earlier analyzers, deliberate functionality
losses and the corrected RDS5 terminator rejection. No older implementation was
added to the release. Raw logs and comparison results stay outside the repository.

`python -B tools/release_check.py` checks the source manifest, file types, heuristic
secret/personal-data patterns and reachable Git history. Local build outputs are
excluded unless explicitly tracked; a regression test checks that tracked binaries
still fail. Public GitHub noreply author addresses are allowed; personal email
addresses are flagged. Pattern matching cannot guarantee secret absence.

The original tools release began without inherited research history. The tools now
live under tools-mit/ in Xenstride. Root CI scans the enclosing repository history
with scripts/check_publication.py --require-history; the component check can take
--history-root. LF checkout attributes keep manifest digests stable. Publication
requires owner approval; repository visibility is not assumed by this document.
Original tracked-file hashes and existing Git states are checked separately;
those private verification records are not included in the release.
