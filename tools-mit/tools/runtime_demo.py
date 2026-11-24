"""Native execution demonstration with automatic checks (Windows x64 only).

Builds the owned fixtures in memory, runs each in an isolated child, and compares
every outcome with an expectation computed independently in this script.
Exit status 0 only if every check passes. No file is written.
"""
# SPDX-License-Identifier: MIT
import json
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
import runtime_fixture as fixture  # noqa: E402
from xenstride import runtime  # noqa: E402
from xenstride.io import UnsupportedError  # noqa: E402


def check(label, ok, detail):
    print(f"{'PASS' if ok else 'FAIL'}  {label}: {detail}")
    return ok


def main():
    results = []
    a, b = 5, 7
    for base in ("preferred", "relocate"):
        r = runtime.execute(fixture.build(), [a, b], base=base, observe=[fixture.DATA])
        want = fixture.expected(a, b)
        results.append(check(
            f"compute at {base} base", r.get("value") == want and r["observed"].get(hex(fixture.DATA)) == want
            and r["isolation_verified"],
            f"returned {r.get('value')} expected {want}; mapped {r['base']:#x}, delta {r['delta']:#x}, "
            f"{r['relocations_applied']} DIR64 applied, {r['elapsed_seconds']} s"))
    r = runtime.execute(fixture.build("write_rdata"), [1], base="relocate")
    results.append(check("write to .rdata faults", r["status"] == "native_exception" and r.get("fault_rva") == fixture.RDATA,
                         f"{r['status']} at RVA {r.get('fault_rva', -1):#x}"))
    r = runtime.execute(fixture.build("jump_to_data"), base="relocate")
    results.append(check("execute in .data faults (NX)", r["status"] == "native_exception" and r.get("fault_rva") == fixture.DATA,
                         f"{r['status']} at RVA {r.get('fault_rva', -1):#x}"))
    r = runtime.execute(fixture.build("spin"), timeout=2)
    results.append(check("infinite loop killed", r["status"] == "timeout" and r["job_active_processes"] == 0,
                         f"{r['status']} after {r['elapsed_seconds']} s, {r['job_active_processes']} process left"))
    try:
        image = fixture.build(extra_directories=[(9, fixture.DATA + 8, 40)])
        runtime.execute(image)
        results.append(check("TLS image refused", False, "was executed"))
    except UnsupportedError as exc:
        results.append(check("TLS image refused before spawn", True, str(exc)))

    # Imports: the fixture calls 5 allowlisted kernel32 functions through its IAT.
    a, b, c, d = 5, 7, 3, 0x0BADC0DE
    outputs = [fixture.OUT_MULDIV, fixture.OUT_PID, fixture.OUT_LASTERROR, fixture.OUT_STRLEN]
    r = runtime.execute(fixture.build_imports(), [a, b, c, d], base="relocate", observe=outputs)
    o = r.get("observed", {})
    got = [o.get(hex(x)) for x in outputs]
    want = [fixture.muldiv(a, b, c) & 0xFFFFFFFF, r.get("child_pid"), d, len(fixture.MESSAGE) - 1]
    bound = ", ".join(f"{i['name']}->{i['resolved_module']}" for i in r.get("imports", []))
    results.append(check(
        "imports resolved and called through the IAT",
        r["status"] == "returned" and got == want and r.get("value") == fixture.muldiv(a, b, c) and r["isolation_verified"],
        f"MulDiv={got[0]} (model {want[0]}), GetCurrentProcessId={got[1]} (child pid {want[1]}), "
        f"GetLastError={got[2]:#x} after SetLastError({d:#x}), lstrlenA={got[3]} (model {want[3]}); "
        f"delta {r.get('delta', 0):#x}; bound: {bound}"))
    for label, libraries in [("CreateProcessW", [("KERNEL32.dll", ["CreateProcessW"])]),
                             ("USER32.dll", [("USER32.dll", ["MessageBoxW"])]),
                             ("ordinal #1", [("KERNEL32.dll", [1])])]:
        try:
            runtime.execute(fixture.build_imports(libraries))
            results.append(check(f"import {label} refused", False, "was executed"))
        except UnsupportedError as exc:
            results.append(check(f"import {label} refused before spawn", True, str(exc)))
    print(json.dumps(dict(passed=sum(results), total=len(results))))
    return 0 if all(results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
