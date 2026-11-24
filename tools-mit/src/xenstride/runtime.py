"""Prototype native loader for owned synthetic PE32+ test images (Windows x64 only).

`plan` and `layout` are pure Python and run anywhere. `execute` maps the image
and calls its entry point in a separate Python child process. The child is held
in a Windows Job Object that kills it when the job closes, allows one process
only, ends it on an unhandled exception, and caps its memory and CPU time. A
wall-clock timeout also applies.

This is containment for a test, not a security sandbox: the child keeps the
caller's user token, file system and network access. Execute only images you
built yourself. Supported subset: AMD64, no delay imports, no TLS, no exception
or CLR directory, DIR64 relocations, page-aligned sections without write+execute.
The entry point is called as `uint64 f(uint64, uint64, uint64, uint64)`; this is
a test ABI, not DllMain or a process entry.

Imports: only names listed in ALLOWED_IMPORTS (a few side-effect-free kernel32
functions with fixed prototypes) are accepted. They are bound in the child with
GetModuleHandleW + GetProcAddress on modules ALREADY loaded in the process; the
runtime never calls LoadLibrary. Each resolved address must belong to a module
in the Windows system directory. Ordinal, bound, unknown or misplaced imports are
refused before any process is created. A PE import table carries no prototypes:
the prototype is enforced by the allowlist, not verified against the image.
"""
# SPDX-License-Identifier: MIT
import argparse
import json
import os
from pathlib import Path
import re
import struct
import subprocess
import sys
import time

from . import pe
from .io import FormatError, LimitError, UnsupportedError

PAGE = 4096
MAX_IMAGE = 64 << 20
MASK64 = (1 << 64) - 1
EXECUTE, READ, WRITE = 0x20000000, 0x40000000, 0x80000000
PAGE_NOACCESS, PAGE_READONLY, PAGE_READWRITE = 0x01, 0x02, 0x04
PAGE_EXECUTE, PAGE_EXECUTE_READ = 0x10, 0x20
REFUSED_DIRECTORIES = {3: "exception directory present; unwind registration is not implemented",
                       11: "bound import directory present; pre-bound imports are not supported",
                       14: "CLR header present; managed images are not supported"}
MAX_IMPORT_DESCRIPTORS, MAX_IMPORTS_PER_LIBRARY = 16, 64

# Explicit allowlist: library (lower case) -> exported name (case-sensitive) -> prototype.
# Criteria: no handle, file, network, process, thread, memory-mapping or module-loading
# effect; the result is observable and checkable by an independent model.
ALLOWED_IMPORTS = {
    "kernel32.dll": {
        "GetCurrentProcessId": "DWORD WINAPI GetCurrentProcessId(void)",
        "GetLastError": "DWORD WINAPI GetLastError(void)",
        "SetLastError": "void WINAPI SetLastError(DWORD dwErrCode)",
        "MulDiv": "int WINAPI MulDiv(int nNumber, int nNumerator, int nDenominator)",
        "lstrlenA": "int WINAPI lstrlenA(LPCSTR lpString)",
    },
}


def _refuse(reason):
    raise UnsupportedError("runtime prototype: " + reason)


def import_bindings(image, plan_, allowlist=None):
    """Walk the import directory of the mapped `image` and return one binding per IAT slot.

    Every import must be a by-name entry of the allowlist; the IAT slot must lie inside a
    non-executable section. Nothing is resolved here.
    """
    allowlist = ALLOWED_IMPORTS if allowlist is None else allowlist
    rva, size = plan_["import_directory"]
    if not rva:
        return []

    def get(at, fmt):
        n = struct.calcsize(fmt)
        if at < 0 or at + n > len(image):
            raise FormatError(f"import data at RVA {at:#x} outside mapped image")
        return struct.unpack_from(fmt, image, at)

    def name_at(at):
        end = image.find(b"\0", at, at + 257)
        if at >= len(image) or end < 0:
            raise FormatError(f"unterminated import name at RVA {at:#x}")
        try:
            return bytes(image[at:end]).decode("ascii")
        except UnicodeDecodeError as exc:
            raise FormatError("non-ASCII import name") from exc

    data_sections = [(s["rva"], s["rva"] + s["size"]) for s in plan_["sections"]
                     if s["protect"] in (PAGE_READONLY, PAGE_READWRITE)]
    bindings = []
    for index in range(MAX_IMPORT_DESCRIPTORS + 1):
        lookup, stamp, chain, name_rva, first = get(rva + 20 * index, "<5I")
        if not any((lookup, stamp, chain, name_rva, first)):
            return bindings
        if index == MAX_IMPORT_DESCRIPTORS:
            raise LimitError(f"more than {MAX_IMPORT_DESCRIPTORS} import descriptors")
        library = name_at(name_rva)
        if stamp or chain not in (0, 0xFFFFFFFF):
            _refuse(f"{library}: bound/forwarder-chained import descriptor (TimeDateStamp {stamp:#x})")
        allowed = allowlist.get(library.lower())
        if allowed is None:
            _refuse(f"{library} is not in the import allowlist")
        lookup = lookup or first
        for j in range(MAX_IMPORTS_PER_LIBRARY + 1):
            (entry,) = get(lookup + 8 * j, "<Q")
            if entry == 0:
                break
            if j == MAX_IMPORTS_PER_LIBRARY:
                raise LimitError(f"{library}: more than {MAX_IMPORTS_PER_LIBRARY} imports")
            if entry >> 63:
                _refuse(f"{library}!#{entry & 0xFFFF}: ordinal imports are refused (no name to check)")
            if entry >> 31:
                raise FormatError(f"{library}: import name RVA {entry:#x} out of range")
            symbol = name_at(entry + 2)  # skip the 16-bit hint
            if symbol not in allowed:
                _refuse(f"{library}!{symbol} is not in the import allowlist")
            slot = first + 8 * j
            if slot % 8 or not any(lo <= slot and slot + 8 <= hi for lo, hi in data_sections):
                _refuse(f"{library}!{symbol}: IAT slot RVA {slot:#x} is not inside a non-executable data section")
            bindings.append(dict(library=library, name=symbol, iat_rva=slot, prototype=allowed[symbol]))
    raise LimitError(f"more than {MAX_IMPORT_DESCRIPTORS} import descriptors")


def resolve_bindings(bindings, allowlist=None):
    """Resolve bindings in THIS process without loading anything (Windows only).

    GetModuleHandleW only returns modules that are already loaded; GetProcAddress follows
    export forwarders (e.g. kernel32 -> KERNELBASE/ntdll), so the module that really owns
    the address is looked up and must live in the Windows system directory.
    """
    _require_windows_x64()
    import ctypes
    from ctypes import wintypes as w
    allowlist = ALLOWED_IMPORTS if allowlist is None else allowlist
    k32 = ctypes.WinDLL("kernel32", use_last_error=True)
    k32.GetModuleHandleW.restype, k32.GetModuleHandleW.argtypes = w.HMODULE, [w.LPCWSTR]
    k32.GetProcAddress.restype, k32.GetProcAddress.argtypes = ctypes.c_void_p, [w.HMODULE, ctypes.c_char_p]
    k32.GetModuleHandleExW.restype = w.BOOL
    k32.GetModuleHandleExW.argtypes = [w.DWORD, ctypes.c_void_p, ctypes.POINTER(w.HMODULE)]
    k32.GetModuleFileNameW.restype, k32.GetModuleFileNameW.argtypes = w.DWORD, [w.HMODULE, w.LPWSTR, w.DWORD]
    k32.GetSystemDirectoryW.restype, k32.GetSystemDirectoryW.argtypes = w.UINT, [w.LPWSTR, w.UINT]
    buffer = ctypes.create_unicode_buffer(32768)
    k32.GetSystemDirectoryW(buffer, len(buffer))
    system_dir = os.path.normcase(buffer.value)
    resolved = []
    for b in bindings:
        if b["name"] not in allowlist.get(b["library"].lower(), {}):  # defence in depth
            _refuse(f"{b['library']}!{b['name']} is not in the import allowlist")
        module = k32.GetModuleHandleW(b["library"])
        if not module:
            _refuse(f"{b['library']} is not already loaded in the runtime process; XENSTRIDE never calls LoadLibrary")
        address = k32.GetProcAddress(module, b["name"].encode("ascii"))
        if not address:
            _refuse(f"{b['library']}!{b['name']} is not exported by the system {b['library']}")
        owner = w.HMODULE()
        # FROM_ADDRESS (4) | UNCHANGED_REFCOUNT (2): which loaded module contains the address
        if not k32.GetModuleHandleExW(0x4 | 0x2, address, ctypes.byref(owner)):
            _refuse(f"{b['library']}!{b['name']} resolved to {address:#x}, outside any loaded module")
        length = k32.GetModuleFileNameW(owner, buffer, len(buffer))
        path = buffer.value[:length]
        if os.path.normcase(os.path.dirname(path)) != system_dir:
            _refuse(f"{b['library']}!{b['name']} resolved into {path}, not the Windows system directory")
        resolved.append(dict(b, address=address, resolved_module=os.path.basename(path)))
    return resolved


def _round(value, alignment):
    return (value + alignment - 1) // alignment * alignment


def plan(data):
    """Validate the supported subset and return the mapping plan; nothing is executed."""
    info = pe.inspect(data)
    if info["machine"] != 0x8664:
        _refuse(f"machine {info['machine']:#06x} is not AMD64")
    for library in info["imports"]:  # early, file-level allowlist check (the IAT walk below is authoritative)
        allowed = ALLOWED_IMPORTS.get(library["library"].lower())
        if allowed is None:
            _refuse(f"{library['library']} is not in the import allowlist")
        for symbol in library["symbols"]:
            if "ordinal" in symbol:
                _refuse(f"{library['library']}!#{symbol['ordinal']}: ordinal imports are refused (no name to check)")
            if symbol["name"] not in allowed:
                _refuse(f"{library['library']}!{symbol['name']} is not in the import allowlist")
    if info["delay_imports_present"]:
        _refuse("delay-load imports present; not implemented")
    if info["tls"] is not None:
        _refuse("TLS directory present; TLS setup is not implemented")
    if info["relocation_types_outside_dir64"]:
        _refuse(f"relocation types {info['relocation_types_outside_dir64']} are not implemented (DIR64 only)")
    opt = struct.unpack_from("<I", data, 60)[0] + 24  # bounds already checked by pe.inspect
    section_alignment = struct.unpack_from("<I", data, opt + 32)[0]
    header_size = struct.unpack_from("<I", data, opt + 60)[0]
    count = struct.unpack_from("<I", data, opt + 108)[0]
    directories = [struct.unpack_from("<II", data, opt + 112 + 8 * i) for i in range(count)]
    for index, reason in REFUSED_DIRECTORIES.items():
        if index < count and directories[index][0]:
            _refuse(reason)
    if section_alignment < PAGE or section_alignment % PAGE:
        _refuse(f"section alignment {section_alignment:#x} is below or not a multiple of the 4 KiB page")
    size = info["image_size"]
    if size > MAX_IMAGE:
        raise LimitError("image size exceeds 64 MiB runtime policy")
    if size % section_alignment:
        raise FormatError("SizeOfImage is not a multiple of SectionAlignment")
    if not info["entry_rva"] or not info["entry_in_executable_section"]:
        _refuse("entry point is not inside an executable section")
    sections = []
    for s in info["sections"]:
        flags = s["flags"]
        if s["rva"] % section_alignment:
            _refuse(f"section {s['name']} is not aligned to SectionAlignment")
        if flags & EXECUTE and flags & WRITE:
            _refuse(f"section {s['name']} is writable and executable")
        protect = (PAGE_EXECUTE_READ if flags & EXECUTE and flags & READ else PAGE_EXECUTE if flags & EXECUTE
                   else PAGE_READWRITE if flags & WRITE else PAGE_READONLY if flags & READ else PAGE_NOACCESS)
        extent = _round(max(s["virtual_size"], s["raw_size"]), section_alignment)
        if s["rva"] + extent > size:
            raise FormatError(f"section {s['name']} extends past SizeOfImage after alignment")
        sections.append(dict(name=s["name"], rva=s["rva"], size=extent, protect=protect,
                             raw_offset=s["raw_offset"],
                             copy=min(s["raw_size"], s["virtual_size"] or s["raw_size"])))
    result = dict(preferred_base=info["image_base"], image_size=size, header_size=header_size,
                  entry_rva=info["entry_rva"], sections=sections,
                  relocation_directory=directories[5] if count > 5 else (0, 0),
                  import_directory=directories[1] if count > 1 else (0, 0),
                  relocations_stripped=bool(info["characteristics"] & 1))
    # Import tables are RVAs: their position does not depend on the load address.
    result["imports"] = import_bindings(layout(data, result, result["preferred_base"])[0], result)
    return result


def layout(data, plan_, base):
    """Return (image bytes as mapped at `base`, DIR64 relocations applied)."""
    image = bytearray(plan_["image_size"])
    image[:plan_["header_size"]] = data[:plan_["header_size"]]
    for s in plan_["sections"]:
        image[s["rva"]:s["rva"] + s["copy"]] = data[s["raw_offset"]:s["raw_offset"] + s["copy"]]
    delta = (base - plan_["preferred_base"]) & MASK64
    rva, size = plan_["relocation_directory"]
    if not delta:
        return image, 0
    if plan_["relocations_stripped"]:
        _refuse("image must be relocated but IMAGE_FILE_RELOCS_STRIPPED is set")
    if rva + size > len(image):
        raise FormatError("relocation directory outside mapped image")
    applied, cursor = 0, 0
    while cursor < size:
        page, length = struct.unpack_from("<II", image, rva + cursor) if size - cursor >= 8 else (0, 0)
        if length < 8 or length % 2 or length > size - cursor:
            raise FormatError(f"invalid relocation block at directory offset {cursor:#x} in mapped image")
        for (entry,) in struct.iter_unpack("<H", image[rva + cursor + 8:rva + cursor + length]):
            kind, target = entry >> 12, page + (entry & 0xFFF)
            if kind == 0:
                continue
            if kind != 10:
                _refuse(f"relocation type {kind} is not implemented (DIR64 only)")
            if target + 8 > len(image):
                raise FormatError("relocation target outside mapped image")
            struct.pack_into("<Q", image, target, (struct.unpack_from("<Q", image, target)[0] + delta) & MASK64)
            applied += 1
        cursor += length
    return image, applied


# ---- Windows-only process containment and native mapping ---------------------------------

def _kernel32():
    import ctypes
    from ctypes import wintypes as w
    k32 = ctypes.WinDLL("kernel32", use_last_error=True)
    for name, res, args in [
            ("VirtualAlloc", ctypes.c_void_p, [ctypes.c_void_p, ctypes.c_size_t, w.DWORD, w.DWORD]),
            ("VirtualFree", w.BOOL, [ctypes.c_void_p, ctypes.c_size_t, w.DWORD]),
            ("VirtualProtect", w.BOOL, [ctypes.c_void_p, ctypes.c_size_t, w.DWORD, ctypes.POINTER(w.DWORD)]),
            ("FlushInstructionCache", w.BOOL, [w.HANDLE, ctypes.c_void_p, ctypes.c_size_t]),
            ("GetCurrentProcess", w.HANDLE, []),
            ("SetErrorMode", w.UINT, [w.UINT]),
            ("CreateJobObjectW", w.HANDLE, [ctypes.c_void_p, w.LPCWSTR]),
            ("SetInformationJobObject", w.BOOL, [w.HANDLE, ctypes.c_int, ctypes.c_void_p, w.DWORD]),
            ("QueryInformationJobObject", w.BOOL, [w.HANDLE, ctypes.c_int, ctypes.c_void_p, w.DWORD, ctypes.c_void_p]),
            ("AssignProcessToJobObject", w.BOOL, [w.HANDLE, w.HANDLE]),
            ("TerminateJobObject", w.BOOL, [w.HANDLE, w.UINT]),
            ("OpenProcess", w.HANDLE, [w.DWORD, w.BOOL, w.DWORD]),
            ("CloseHandle", w.BOOL, [w.HANDLE])]:
        function = getattr(k32, name)
        function.restype, function.argtypes = res, args
    return ctypes, w, k32


def _require_windows_x64():
    if os.name != "nt" or struct.calcsize("P") != 8:
        raise UnsupportedError("native execution requires 64-bit Python on Windows")


def run_isolated(argv, stdin_bytes, *, timeout, memory_limit=256 << 20, cpu_seconds=10):
    """Run argv in a kill-on-close Job Object; return outputs, exit code and job accounting."""
    _require_windows_x64()
    ctypes, w, k32 = _kernel32()

    class Basic(ctypes.Structure):
        _fields_ = [("PerProcessUserTimeLimit", ctypes.c_int64), ("PerJobUserTimeLimit", ctypes.c_int64),
                    ("LimitFlags", w.DWORD), ("MinimumWorkingSetSize", ctypes.c_size_t),
                    ("MaximumWorkingSetSize", ctypes.c_size_t), ("ActiveProcessLimit", w.DWORD),
                    ("Affinity", ctypes.c_size_t), ("PriorityClass", w.DWORD), ("SchedulingClass", w.DWORD)]

    class Extended(ctypes.Structure):
        _fields_ = [("Basic", Basic), ("IoInfo", ctypes.c_uint64 * 6), ("ProcessMemoryLimit", ctypes.c_size_t),
                    ("JobMemoryLimit", ctypes.c_size_t), ("PeakProcessMemoryUsed", ctypes.c_size_t),
                    ("PeakJobMemoryUsed", ctypes.c_size_t)]

    class Accounting(ctypes.Structure):
        _fields_ = [("Times", ctypes.c_int64 * 4), ("TotalPageFaultCount", w.DWORD), ("TotalProcesses", w.DWORD),
                    ("ActiveProcesses", w.DWORD), ("TotalTerminatedProcesses", w.DWORD)]

    def check(ok, what):
        if not ok:
            raise OSError(ctypes.get_last_error(), what + " failed")

    job = k32.CreateJobObjectW(None, None)
    check(job, "CreateJobObjectW")
    process_handle = None
    try:
        limits = Extended()
        # KILL_ON_JOB_CLOSE | DIE_ON_UNHANDLED_EXCEPTION | PROCESS_MEMORY | ACTIVE_PROCESS | PROCESS_TIME
        limits.Basic.LimitFlags = 0x2000 | 0x400 | 0x100 | 0x8 | 0x2
        limits.Basic.ActiveProcessLimit = 1
        limits.Basic.PerProcessUserTimeLimit = int(cpu_seconds * 10_000_000)
        limits.ProcessMemoryLimit = memory_limit
        check(k32.SetInformationJobObject(job, 9, ctypes.byref(limits), ctypes.sizeof(limits)),
              "SetInformationJobObject")
        started = time.perf_counter()
        # The child blocks on stdin until the parent has placed it in the job.
        # DETACHED_PROCESS: no console, so no conhost.exe joins the job.
        child = subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                 creationflags=0x00000008)
        process_handle = k32.OpenProcess(0x0100 | 0x0001 | 0x1000, False, child.pid)  # SET_QUOTA|TERMINATE|QUERY_LIMITED
        if not process_handle or not k32.AssignProcessToJobObject(job, process_handle):
            child.kill()
            child.communicate()
            check(False, "AssignProcessToJobObject")
        timed_out = False
        try:
            out, err = child.communicate(stdin_bytes, timeout=timeout)
        except subprocess.TimeoutExpired:
            timed_out = True
            k32.TerminateJobObject(job, 0xDEAD)
            out, err = child.communicate()
        elapsed = time.perf_counter() - started
        accounting = Accounting()
        # Job accounting is updated asynchronously after a process exits; poll briefly
        # (bounded) so a just-exited child is not reported as still active.
        for _ in range(200):
            check(k32.QueryInformationJobObject(job, 1, ctypes.byref(accounting), ctypes.sizeof(accounting),
                                                None), "QueryInformationJobObject")
            if accounting.ActiveProcesses == 0:
                break
            time.sleep(0.01)
        return dict(stdout=out, stderr=err, exit_code=child.returncode, timed_out=timed_out, pid=child.pid,
                    elapsed_seconds=round(elapsed, 3), job_total_processes=accounting.TotalProcesses,
                    job_active_processes=accounting.ActiveProcesses)
    finally:
        if process_handle:
            k32.CloseHandle(process_handle)
        k32.CloseHandle(job)  # kill-on-close ends anything still in the job


def _child_main():
    """Child side: read a JSON header line and the image from stdin, map it, call the entry."""
    def emit(**fields):
        sys.stdout.write(json.dumps(fields) + "\n")
        sys.stdout.flush()

    header = json.loads(sys.stdin.buffer.readline())
    data = sys.stdin.buffer.read()
    ctypes, w, k32 = _kernel32()
    k32.SetErrorMode(0x0001 | 0x0002)  # no critical-error or fault dialogs
    try:
        p = plan(data)
    except FormatError as exc:
        emit(event="refused", message=str(exc))
        return 4
    size, preferred = p["image_size"], p["preferred_base"]
    commit, readwrite = 0x1000 | 0x2000, PAGE_READWRITE
    base = k32.VirtualAlloc(preferred, size, commit, readwrite) if header["base"] == "preferred" else None
    if not base:
        if header["base"] == "relocate":  # occupy the preferred range so the image cannot land there
            k32.VirtualAlloc(preferred, size, 0x2000, PAGE_NOACCESS)
        base = k32.VirtualAlloc(None, size, commit, readwrite)
    if not base or (header["base"] == "relocate" and base == preferred):
        emit(event="refused", message="could not reserve an image range")
        return 4
    try:
        image, applied = layout(data, p, base)
        bound = resolve_bindings(p["imports"])
    except FormatError as exc:
        emit(event="refused", message=str(exc))
        return 4
    for b in bound:  # written before the final protections; the IAT section then becomes read-only if declared so
        struct.pack_into("<Q", image, b["iat_rva"], b["address"])
    if bound:
        emit(event="imports_bound", imports=[{k: b[k] for k in ("library", "name", "iat_rva", "address", "resolved_module")}
                                             for b in bound])
    ctypes.memmove(base, bytes(image), size)
    old = w.DWORD()
    protections = [(0, size, PAGE_NOACCESS), (0, _round(p["header_size"], PAGE), PAGE_READONLY)]
    protections += [(s["rva"], s["size"], s["protect"]) for s in p["sections"]]
    for rva, length, protect in protections:
        if not k32.VirtualProtect(base + rva, length, protect, ctypes.byref(old)):
            emit(event="refused", message=f"VirtualProtect failed at RVA {rva:#x}: {ctypes.get_last_error()}")
            return 4
    k32.FlushInstructionCache(k32.GetCurrentProcess(), base, size)
    emit(event="mapped", base=base, preferred_base=preferred, delta=(base - preferred) & MASK64,
         relocations_applied=applied, entry=base + p["entry_rva"],
         protections=[dict(name=s["name"], rva=s["rva"], size=s["size"], protect=s["protect"])
                      for s in p["sections"]])
    args = [int(a) & MASK64 for a in header["args"]] + [0] * (4 - len(header["args"]))
    entry = ctypes.CFUNCTYPE(ctypes.c_uint64, *[ctypes.c_uint64] * 4)(base + p["entry_rva"])
    try:
        value = entry(*args)
    except OSError as exc:  # ctypes reports a native SEH exception raised inside the call
        emit(event="native_exception", message=str(exc))
        return 3
    readable = [(s["rva"], s["rva"] + s["size"]) for s in p["sections"] if s["protect"] != PAGE_NOACCESS]
    observed = {}
    for rva in header["observe"]:
        if any(lo <= rva and rva + 8 <= hi for lo, hi in readable):
            observed[f"{rva:#x}"] = ctypes.c_uint64.from_address(base + rva).value
    emit(event="returned", value=value, observed=observed)
    return 0


def _child_command():
    # A venv python.exe on Windows is a launcher that starts the base interpreter as
    # its own child before the parent can assign it to the job; start the base
    # interpreter directly so the process that runs native code is the one in the job.
    interpreter = getattr(sys, "_base_executable", None) or sys.executable
    package_parent = str(Path(__file__).resolve().parents[1])
    code = ("import sys; sys.path.insert(0, %r); from xenstride.runtime import _child_main; "
            "raise SystemExit(_child_main())" % package_parent)
    return [interpreter, "-I", "-B", "-c", code]


def execute(data, args=(), *, base="preferred", observe=(), timeout=10.0):
    """Validate, then map and call the entry in an isolated child; never maps in this process."""
    if base not in ("preferred", "relocate"):
        raise ValueError("base must be 'preferred' or 'relocate'")
    if len(args) > 4:
        raise ValueError("at most four integer arguments")
    plan(data)  # refuse unsupported images before any process is created
    _require_windows_x64()
    header = json.dumps(dict(args=list(args), base=base, observe=list(observe))).encode() + b"\n"
    run = run_isolated(_child_command(), header + bytes(data), timeout=timeout)
    events = []
    for line in run["stdout"].decode("utf-8", "replace").splitlines():
        try:
            events.append(json.loads(line))
        except ValueError:
            pass
    last = events[-1]["event"] if events else None
    status = ("timeout" if run["timed_out"] else last if last in ("returned", "native_exception", "refused")
              else "child_failed")
    result = dict(status=status, exit_code=run["exit_code"], elapsed_seconds=run["elapsed_seconds"], child_pid=run["pid"],
                  job_total_processes=run["job_total_processes"], job_active_processes=run["job_active_processes"])
    for event in events:
        result.update({k: v for k, v in event.items() if k != "event"})
    # One process ran in the job and none survived it.
    result["isolation_verified"] = run["job_total_processes"] == 1 and run["job_active_processes"] == 0
    if status == "native_exception" and "base" in result:
        # ctypes labels every non-read access violation "writing" (DEP faults included),
        # so report where the fault happened relative to the mapped image instead.
        address = re.search(r"0x([0-9A-Fa-f]+)", result.get("message", ""))
        if address:
            result["fault_rva"] = int(address.group(1), 16) - result["base"]
    if status == "child_failed":
        result["stderr_tail"] = run["stderr"].decode("utf-8", "replace")[-600:]
    return result


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("input", type=Path)
    parser.add_argument("--execute", action="store_true", help="map and call the entry in an isolated child")
    parser.add_argument("--arg", type=lambda v: int(v, 0), action="append", default=[], help="up to four integers")
    parser.add_argument("--relocate", action="store_true", help="refuse the preferred base to force relocation")
    parser.add_argument("--observe", type=lambda v: int(v, 0), action="append", default=[],
                        help="RVA of a 64-bit value to read after the call")
    parser.add_argument("--timeout", type=float, default=10.0)
    args = parser.parse_args(argv)
    data = args.input.read_bytes()
    try:
        if args.execute:
            result = execute(data, args.arg, base="relocate" if args.relocate else "preferred",
                             observe=args.observe, timeout=args.timeout)
        else:
            result = dict(status="planned", plan=plan(data))
    except FormatError as exc:
        print(json.dumps(dict(status="refused", error=type(exc).__name__, message=str(exc))), file=sys.stderr)
        return 2
    print(json.dumps(result, indent=2))
    return 0 if result["status"] in ("returned", "planned") else 1


if __name__ == "__main__":
    raise SystemExit(main())
