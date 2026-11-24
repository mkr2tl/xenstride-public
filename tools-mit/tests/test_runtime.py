"""Runtime prototype tests: pure planning/layout everywhere, real native calls on Windows x64.

The native tests execute only the hand-assembled code from tools/runtime_fixture.py,
inside a Job Object child process. No external or third-party binary is executed.
"""
# SPDX-License-Identifier: MIT
import importlib.util
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

from xenstride import runtime
from xenstride.io import FormatError, UnsupportedError

_spec = importlib.util.spec_from_file_location(
    "runtime_fixture", Path(__file__).resolve().parents[1] / "tools" / "runtime_fixture.py")
fixture = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(fixture)
NATIVE = os.name == "nt" and struct.calcsize("P") == 8


def with_imports(data):
    """Add one ordinal import descriptor inside .data (file-backed zeros)."""
    base = fixture.DATA_RAW
    struct.pack_into("<5I", data, base + 0x20, fixture.DATA + 0x60, 0, 0, fixture.DATA + 0x80, fixture.DATA + 0x70)
    struct.pack_into("<Q", data, base + 0x60, (1 << 63) | 1)
    data[base + 0x80:base + 0x86] = b"k.dll\0"
    struct.pack_into("<II", data, fixture.OPT + 112 + 8, fixture.DATA + 0x20, 40)
    return data


class PlanAndLayoutTests(unittest.TestCase):
    def test_plan_protections_and_entry(self):
        plan = runtime.plan(fixture.build())
        self.assertEqual([(s["name"], s["protect"]) for s in plan["sections"]],
                         [(".text", runtime.PAGE_EXECUTE_READ), (".rdata", runtime.PAGE_READONLY),
                          (".data", runtime.PAGE_READWRITE), (".reloc", runtime.PAGE_READONLY)])
        self.assertEqual((plan["entry_rva"], plan["preferred_base"], plan["image_size"]),
                         (0x1000, fixture.IMAGE_BASE, 0x5000))

    def test_layout_at_preferred_base_changes_nothing(self):
        data = fixture.build()
        image, applied = runtime.layout(data, runtime.plan(data), fixture.IMAGE_BASE)
        code, _ = fixture.program("compute")
        self.assertEqual(applied, 0)
        self.assertEqual(bytes(image[0x1000:0x1000 + len(code)]), code)
        self.assertEqual(struct.unpack_from("<Q", image, fixture.RDATA)[0], fixture.CONSTANT)

    def test_relocation_adds_delta_only_at_dir64_targets(self):
        data = fixture.build()
        plan = runtime.plan(data)
        preferred, _ = runtime.layout(data, plan, fixture.IMAGE_BASE)
        delta = 0x7FF5_1234_0000 - fixture.IMAGE_BASE
        moved, applied = runtime.layout(data, plan, 0x7FF5_1234_0000)
        self.assertEqual(applied, 2)
        self.assertEqual(struct.unpack_from("<Q", moved, 0x1002)[0], fixture.IMAGE_BASE + fixture.RDATA + delta)
        self.assertEqual(struct.unpack_from("<Q", moved, 0x1016)[0], fixture.IMAGE_BASE + fixture.DATA + delta)
        differing = [i for i in range(len(moved)) if moved[i] != preferred[i]]
        self.assertTrue(all(0x1002 <= i < 0x100A or 0x1016 <= i < 0x101E for i in differing))

    def test_unsupported_images_are_refused_with_reason(self):
        cases = [
            ("i386 machine", fixture.build(machine=0x14C), r"not AMD64"),
            ("writable code", fixture.build(text_flags=0xE0000020), r"\.text is writable and executable"),
            ("sub-page alignment", fixture.build(section_alignment=0x200), r"section alignment 0x200"),
            ("entry not executable", fixture.build(text_flags=fixture.DATA_R), r"entry point is not inside"),
            ("exception directory", fixture.build(extra_directories=[(3, fixture.RDATA, 12)]), r"unwind registration"),
            ("CLR header", fixture.build(extra_directories=[(14, fixture.RDATA, 8)]), r"managed images"),
            ("TLS", fixture.build(extra_directories=[(9, fixture.DATA + 8, 40)]), r"TLS"),
            ("HIGHLOW relocation", fixture.build(relocation_type=3), r"relocation types \[3\]"),
            ("imports", with_imports(fixture.build()), r"k\.dll is not in the import allowlist"),
        ]
        for label, data, message in cases:
            with self.subTest(label), self.assertRaisesRegex(UnsupportedError, message):
                runtime.plan(data)

    def test_relocs_stripped_image_cannot_move(self):
        data = fixture.build(characteristics=0x2023)
        plan = runtime.plan(data)
        runtime.layout(data, plan, fixture.IMAGE_BASE)  # preferred base still allowed
        with self.assertRaisesRegex(UnsupportedError, "RELOCS_STRIPPED"):
            runtime.layout(data, plan, fixture.IMAGE_BASE + 0x10000)

    def test_malformed_pe_is_a_format_error(self):
        with self.assertRaises(FormatError):
            runtime.plan(fixture.build()[:0x300])

    def test_refusal_happens_before_any_process_is_created(self):
        with mock.patch.object(runtime.subprocess, "Popen", side_effect=AssertionError("must not spawn")):
            with self.assertRaises(UnsupportedError):
                runtime.execute(with_imports(fixture.build()), [1, 2])


@unittest.skipUnless(NATIVE, "native execution requires 64-bit Python on Windows")
class NativeExecutionTests(unittest.TestCase):
    def assert_isolated(self, result):
        self.assertTrue(result["isolation_verified"], result)

    def test_entry_returns_expected_value_at_preferred_and_relocated_bases(self):
        for base, a, b in [("preferred", 5, 7), ("relocate", 5, 7), ("relocate", (1 << 64) - 3, 0x1_0000_0001)]:
            with self.subTest(base=base, a=a, b=b):
                result = runtime.execute(fixture.build(), [a, b], base=base, observe=[fixture.DATA], timeout=20)
                self.assertEqual(result["status"], "returned", result)
                self.assertEqual(result["value"], fixture.expected(a, b))
                self.assertEqual(result["observed"], {hex(fixture.DATA): fixture.expected(a, b)})
                self.assertEqual(result["relocations_applied"], 0 if result["delta"] == 0 else 2)
                if base == "relocate":
                    self.assertNotEqual(result["delta"], 0)
                self.assert_isolated(result)

    def test_write_to_read_only_section_faults_there(self):
        result = runtime.execute(fixture.build("write_rdata"), [1], base="relocate", timeout=20)
        self.assertEqual(result["status"], "native_exception", result)
        self.assertEqual(result["fault_rva"], fixture.RDATA)
        self.assert_isolated(result)

    def test_executing_data_section_faults_there(self):
        result = runtime.execute(fixture.build("jump_to_data"), base="relocate", timeout=20)
        self.assertEqual(result["status"], "native_exception", result)
        self.assertEqual(result["fault_rva"], fixture.DATA)  # .data holds a 'ret' byte; NX must stop it
        self.assert_isolated(result)

    def test_infinite_loop_is_killed_at_timeout(self):
        result = runtime.execute(fixture.build("spin"), timeout=2)
        self.assertEqual(result["status"], "timeout", result)
        self.assertLess(result["elapsed_seconds"], 10)
        self.assertEqual(result["job_active_processes"], 0)

    def test_job_refuses_grandchild_processes(self):
        interpreter = getattr(sys, "_base_executable", None) or sys.executable
        code = "import subprocess, sys; subprocess.run([sys.executable, '-c', '0'])"
        run = runtime.run_isolated([interpreter, "-I", "-c", code], b"", timeout=20)
        self.assertNotEqual(run["exit_code"], 0)
        self.assertEqual((run["job_total_processes"], run["job_active_processes"]), (1, 0))
        control = subprocess.run([interpreter, "-I", "-c", code], capture_output=True, timeout=20)
        self.assertEqual(control.returncode, 0)  # the same command works outside the job

    def test_cli_executes_and_reports_json(self):
        with tempfile.TemporaryDirectory() as temp:
            file = Path(temp) / "fixture.img"
            file.write_bytes(fixture.build())
            result = subprocess.run([sys.executable, "-I", "-B", "-m", "xenstride.runtime", str(file),
                                     "--execute", "--arg", "3", "--arg", "4", "--relocate"],
                                    capture_output=True, text=True, timeout=60)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(result.stdout)["value"], fixture.expected(3, 4))
            file.write_bytes(with_imports(fixture.build()))
            result = subprocess.run([sys.executable, "-I", "-B", "-m", "xenstride.runtime", str(file), "--execute"],
                                    capture_output=True, text=True, timeout=60)
            self.assertEqual(result.returncode, 2)
            self.assertEqual(json.loads(result.stderr)["error"], "UnsupportedError")


OUTPUTS = [fixture.OUT_MULDIV, fixture.OUT_PID, fixture.OUT_LASTERROR, fixture.OUT_STRLEN]


class ImportPlanTests(unittest.TestCase):
    """Allowlist and IAT validation: pure Python, no process, no resolution."""

    def test_allowlist_is_exactly_the_reviewed_set(self):
        self.assertEqual({lib: sorted(names) for lib, names in runtime.ALLOWED_IMPORTS.items()},
                         {"kernel32.dll": sorted(["GetCurrentProcessId", "GetLastError", "SetLastError", "MulDiv",
                                                  "lstrlenA"])})

    def test_fixture_bindings_in_iat_order(self):
        plan = runtime.plan(fixture.build_imports())
        self.assertEqual([(b["library"], b["name"], b["iat_rva"]) for b in plan["imports"]],
                         [("KERNEL32.dll", name, fixture.IAT_RVA + 8 * i) for i, name in enumerate(fixture.CALLS)])
        self.assertTrue(all(b["prototype"].startswith(("DWORD", "void", "int")) for b in plan["imports"]))

    def test_forbidden_malformed_and_unsupported_imports_are_refused(self):
        cases = [
            ("DLL outside allowlist", fixture.build_imports([("USER32.dll", ["MessageBoxW"])]), UnsupportedError,
             r"USER32\.dll is not in the import allowlist"),
            ("process creation", fixture.build_imports([("KERNEL32.dll", ["CreateProcessW"])]), UnsupportedError,
             r"KERNEL32\.dll!CreateProcessW is not in the import allowlist"),
            ("module loading", fixture.build_imports([("KERNEL32.dll", ["LoadLibraryA"])]), UnsupportedError,
             r"LoadLibraryA is not in the import allowlist"),
            ("memory protection", fixture.build_imports([("KERNEL32.dll", ["VirtualProtect"])]), UnsupportedError,
             r"VirtualProtect is not in the import allowlist"),
            ("names are case-sensitive", fixture.build_imports([("KERNEL32.dll", ["muldiv"])]), UnsupportedError,
             r"muldiv is not in the import allowlist"),
            ("ordinal import", fixture.build_imports([("KERNEL32.dll", [1])]), UnsupportedError, r"ordinal imports"),
            ("one bad among good", fixture.build_imports([("KERNEL32.dll", ["MulDiv", "ExitProcess"])]),
             UnsupportedError, r"ExitProcess is not in the import allowlist"),
            ("second DLL refused", fixture.build_imports([("KERNEL32.dll", ["MulDiv"]), ("WS2_32.dll", ["socket"])]),
             UnsupportedError, r"WS2_32\.dll is not in the import allowlist"),
            ("bound descriptor", fixture.build_imports(stamp=0xFFFFFFFF), UnsupportedError, r"bound"),
            ("IAT inside code", fixture.build_imports(iat_rva=fixture.TEXT + 0x100), UnsupportedError,
             r"not inside a non-executable data section"),
            ("IAT outside image", fixture.build_imports(iat_rva=0x7000), FormatError, r"outside"),
            ("bound import directory", self.with_directory(fixture.build_imports(), 11, fixture.IDATA + 0x3F0, 8),
             UnsupportedError, r"bound import directory"),
        ]
        for label, data, error, message in cases:
            with self.subTest(label), self.assertRaisesRegex(error, message):
                runtime.plan(data)

    def test_malformed_name_tables_are_format_errors(self):
        data = fixture.build_imports()
        name_rva = struct.unpack_from("<Q", data, fixture.IDATA_RAW + 0x100)[0]
        data[fixture.IDATA_RAW + name_rva - fixture.IDATA + 2:fixture.IDATA_RAW + 0x400] = b"A" * (
            0x400 - (name_rva - fixture.IDATA + 2))                       # name runs to the end of .idata
        with self.assertRaises(FormatError):
            runtime.plan(data)
        data = fixture.build_imports()
        struct.pack_into("<Q", data, fixture.IDATA_RAW + 0x100, 0x7FFFFFF0)  # name RVA outside the image
        with self.assertRaises(FormatError):
            runtime.plan(data)

    @staticmethod
    def with_directory(data, index, rva, size):
        struct.pack_into("<II", data, fixture.OPT + 112 + 8 * index, rva, size)
        return data

    def test_forbidden_import_refused_before_any_process(self):
        with mock.patch.object(runtime.subprocess, "Popen", side_effect=AssertionError("must not spawn")):
            with self.assertRaises(UnsupportedError):
                runtime.execute(fixture.build_imports([("KERNEL32.dll", ["CreateProcessW"])]), [1, 2])

    def test_muldiv_model(self):
        self.assertEqual([fixture.muldiv(*v) for v in [(5, 7, 3), (-10, 3, 4), (10, 3, -4), (1, 1, 0),
                                                        (0x7FFFFFFF, 2, 1), (7, 1, 2), (-7, 1, 2)]],
                         [12, -8, -8, -1, -1, 4, -4])


@unittest.skipUnless(NATIVE, "native execution requires 64-bit Python on Windows")
class ImportResolutionNativeTests(unittest.TestCase):
    def test_resolution_never_loads_a_module(self):
        import ctypes
        k32 = ctypes.WinDLL("kernel32")
        k32.GetModuleHandleW.restype = ctypes.c_void_p
        absent = "xinput1_4.dll" if not k32.GetModuleHandleW("xinput1_4.dll") else "dbghelp.dll"
        self.assertFalse(k32.GetModuleHandleW(absent), f"{absent} unexpectedly loaded in the test process")
        binding = [dict(library=absent, name="X", iat_rva=0, prototype="")]
        with self.assertRaisesRegex(UnsupportedError, "never calls LoadLibrary"):
            runtime.resolve_bindings(binding, allowlist={absent: {"X": ""}})
        self.assertFalse(k32.GetModuleHandleW(absent), "resolution must not have loaded the module")

    def test_absent_export_and_defence_in_depth(self):
        missing = [dict(library="kernel32.dll", name="XenstrideDoesNotExist", iat_rva=0, prototype="")]
        with self.assertRaisesRegex(UnsupportedError, "not exported"):
            runtime.resolve_bindings(missing, allowlist={"kernel32.dll": {"XenstrideDoesNotExist": ""}})
        with self.assertRaisesRegex(UnsupportedError, "not in the import allowlist"):
            runtime.resolve_bindings([dict(library="kernel32.dll", name="CreateProcessW", iat_rva=0, prototype="")])

    def test_resolved_addresses_come_from_the_system_directory(self):
        resolved = runtime.resolve_bindings(runtime.plan(fixture.build_imports())["imports"])
        self.assertEqual([r["name"] for r in resolved], fixture.CALLS)
        self.assertTrue(all(r["address"] and r["resolved_module"].lower() in ("kernel32.dll", "kernelbase.dll", "ntdll.dll")
                            for r in resolved), resolved)

    def test_fixture_calls_windows_through_its_iat(self):
        cases = [("preferred", (5, 7, 3, 0x0BADC0DE)), ("relocate", (5, 7, 3, 0x0BADC0DE)),
                 ("relocate", ((1 << 64) - 10, 3, 4, 0)), ("relocate", (0x7FFFFFFF, 2, 1, 0xFFFFFFFF)),
                 ("relocate", (1, 1, 0, 42))]
        for base, (a, b, c, d) in cases:
            with self.subTest(base=base, args=(a, b, c, d)):
                result = runtime.execute(fixture.build_imports(), [a, b, c, d], base=base, observe=OUTPUTS, timeout=20)
                self.assertEqual(result["status"], "returned", result)
                want = fixture.muldiv(a, b, c)
                self.assertEqual(result["value"], want & runtime.MASK64)                 # entry return = MulDiv
                observed = result["observed"]
                self.assertEqual(observed[hex(fixture.OUT_MULDIV)], want & 0xFFFFFFFF)
                self.assertEqual(observed[hex(fixture.OUT_PID)], result["child_pid"])    # GetCurrentProcessId
                self.assertEqual(observed[hex(fixture.OUT_LASTERROR)], d & 0xFFFFFFFF)   # Set/GetLastError round trip
                self.assertEqual(observed[hex(fixture.OUT_STRLEN)], len(fixture.MESSAGE) - 1)  # lstrlenA, relocated ptr
                self.assertEqual([i["name"] for i in result["imports"]], fixture.CALLS)
                self.assertEqual(result["relocations_applied"], 0 if result["delta"] == 0 else 1)
                idata = next(s for s in result["protections"] if s["name"] == ".idata")
                self.assertEqual(idata["protect"], runtime.PAGE_READONLY)               # IAT read-only once bound
                self.assertTrue(result["isolation_verified"], result)

    def test_cli_reports_bound_imports(self):
        with tempfile.TemporaryDirectory() as temp:
            file = Path(temp) / "imports.img"
            file.write_bytes(fixture.build_imports())
            result = subprocess.run([sys.executable, "-I", "-B", "-m", "xenstride.runtime", str(file), "--execute",
                                     "--arg", "6", "--arg", "7", "--arg", "2", "--arg", "99", "--relocate",
                                     "--observe", hex(fixture.OUT_LASTERROR)],
                                    capture_output=True, text=True, timeout=60)
            self.assertEqual(result.returncode, 0, result.stderr)
            report = json.loads(result.stdout)
            self.assertEqual((report["value"], report["observed"][hex(fixture.OUT_LASTERROR)]), (21, 99))
            self.assertEqual(len(report["imports"]), 5)


if __name__ == "__main__":
    unittest.main()
