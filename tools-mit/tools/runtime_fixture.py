"""Build owned synthetic PE32+ images with a few bytes of hand-assembled x64 code.

Every byte comes from literal fields below; nothing is extracted from another
binary. The test entry ABI is `uint64 entry(uint64 rcx, uint64 rdx, uint64 r8,
uint64 r9)` (Microsoft x64 calling convention). It is not DllMain or main.
"""
# SPDX-License-Identifier: MIT
import argparse
import hashlib
from pathlib import Path
import struct

IMAGE_BASE = 0x140000000
CONSTANT = 0x00000123456789AB  # stored in .rdata, read by the compute entry
TEXT, RDATA, DATA, RELOC = 0x1000, 0x2000, 0x3000, 0x4000
TEXT_RAW, RDATA_RAW, DATA_RAW, RELOC_RAW = 0x400, 0x600, 0x800, 0xA00
OPT = 0x98

# Section characteristics (PE/COFF): code/initialized data plus memory permissions.
CODE_RX = 0x60000020
DATA_R = 0x40000040
DATA_RW = 0xC0000040
RELOC_R = 0x42000040


def imm64(value):
    return struct.pack("<Q", value)


def program(kind):
    """Return (code bytes, offsets of 64-bit absolute addresses inside the code)."""
    if kind == "compute":
        # rax = [K_va]; rax += rcx; rax *= rdx; [counter_va] = rax; return rax
        code = (b"\x48\xB8" + imm64(IMAGE_BASE + RDATA)   # mov rax, imm64 (.rdata)
                + b"\x48\x8B\x00"                          # mov rax, [rax]
                + b"\x48\x01\xC8"                          # add rax, rcx
                + b"\x48\x0F\xAF\xC2"                      # imul rax, rdx
                + b"\x49\xB9" + imm64(IMAGE_BASE + DATA)   # mov r9, imm64 (.data)
                + b"\x49\x89\x01"                          # mov [r9], rax
                + b"\xC3")                                 # ret
        return code, (2, 22)
    if kind == "write_rdata":
        return b"\x48\xB8" + imm64(IMAGE_BASE + RDATA) + b"\x48\x89\x08\xC3", (2,)  # mov [rax], rcx
    if kind == "jump_to_data":
        return b"\x48\xB8" + imm64(IMAGE_BASE + DATA) + b"\xFF\xE0", (2,)           # jmp rax
    if kind == "spin":
        return b"\xEB\xFE", ()                                                      # jmp $
    raise ValueError("unknown program")


def expected(a, b):
    """Independent model of the compute entry: ((K + a) * b) mod 2**64."""
    return ((CONSTANT + a) * b) & 0xFFFFFFFFFFFFFFFF


def build(kind="compute", *, text_flags=CODE_RX, machine=0x8664, characteristics=0x2022,
          section_alignment=0x1000, extra_directories=(), relocation_type=10):
    code, absolutes = program(kind)
    data = bytearray(0xC00)
    data[:2] = b"MZ"
    struct.pack_into("<I", data, 60, 0x80)
    data[0x80:0x84] = b"PE\0\0"
    struct.pack_into("<HHIIIHH", data, 0x84, machine, 4, 0, 0, 0, 240, characteristics)
    struct.pack_into("<H", data, OPT, 0x20B)
    struct.pack_into("<I", data, OPT + 16, TEXT)                       # AddressOfEntryPoint
    struct.pack_into("<Q", data, OPT + 24, IMAGE_BASE)
    struct.pack_into("<II", data, OPT + 32, section_alignment, 0x200)
    struct.pack_into("<II", data, OPT + 56, 0x5000, 0x400)               # SizeOfImage, SizeOfHeaders
    struct.pack_into("<H", data, OPT + 68, 3)                            # console subsystem
    struct.pack_into("<H", data, OPT + 70, 0x0160)                       # HIGH_ENTROPY_VA | DYNAMIC_BASE | NX_COMPAT
    struct.pack_into("<I", data, OPT + 108, 16)
    block = struct.pack("<II", TEXT, 8 + 2 * len(absolutes) + (2 if len(absolutes) % 2 else 0))
    block += b"".join(struct.pack("<H", relocation_type << 12 | offset) for offset in absolutes)
    block = block.ljust(struct.unpack_from("<I", block, 4)[0], b"\0")
    if absolutes:
        struct.pack_into("<II", data, OPT + 112 + 5 * 8, RELOC, len(block))
    for index, rva, size in extra_directories:
        struct.pack_into("<II", data, OPT + 112 + 8 * index, rva, size)
    table = OPT + 240
    for i, (name, rva, vsize, raw, flags) in enumerate([
            (b".text", TEXT, len(code), TEXT_RAW, text_flags),
            (b".rdata", RDATA, 8, RDATA_RAW, DATA_R),
            (b".data", DATA, 16, DATA_RAW, DATA_RW),
            (b".reloc", RELOC, max(len(block), 1), RELOC_RAW, RELOC_R)]):
        struct.pack_into("<8s6I2HI", data, table + 40 * i, name, vsize, rva, 0x200, raw, 0, 0, 0, 0, flags)
    data[TEXT_RAW:TEXT_RAW + len(code)] = code
    struct.pack_into("<Q", data, RDATA_RAW, CONSTANT)
    data[DATA_RAW] = 0xC3  # a 'ret' byte in non-executable data, for the NX negative test
    data[RELOC_RAW:RELOC_RAW + len(block)] = block
    return data


# ---- Fixture with a real import table (kernel32 calls through the IAT) -----------------------
IDATA, IDATA_RAW = 0x4000, 0xA00
IMP_RELOC, IMP_RELOC_RAW = 0x5000, 0xE00
MESSAGE = b"XENSTRIDE import resolution\0"   # in .rdata; length checked through lstrlenA
CALLS = ["MulDiv", "GetCurrentProcessId", "SetLastError", "GetLastError", "lstrlenA"]
# .data result slots (each written as a 32-bit value into a zeroed 64-bit slot)
OUT_MULDIV, OUT_PID, OUT_LASTERROR, OUT_STRLEN = DATA, DATA + 8, DATA + 16, DATA + 24
DESC, INT_RVA, IAT_RVA, NAMES = IDATA, IDATA + 0x100, IDATA + 0x200, IDATA + 0x300


def import_program():
    """entry(a, b, c, d): MulDiv(a,b,c) -> [OUT_MULDIV]; GetCurrentProcessId() -> [OUT_PID];
    SetLastError(d); GetLastError() -> [OUT_LASTERROR]; lstrlenA(&MESSAGE) -> [OUT_STRLEN];
    return sign-extended MulDiv result. Calls use `call [rip+disp32]` through the IAT."""
    code = bytearray()
    absolutes = []

    def rip(target, opcode):  # instruction = opcode + disp32 relative to the next instruction
        code.extend(opcode)
        code.extend(struct.pack("<i", target - (TEXT + len(code) + 4)))

    def call(name):
        rip(IAT_RVA + 8 * CALLS.index(name), b"\xFF\x15")             # call qword [rip+iat]

    code += b"\x48\x83\xEC\x28"                                       # sub rsp, 0x28 (shadow + align)
    code += b"\x4C\x89\x4C\x24\x48"                                   # mov [rsp+0x48], r9 (home of arg 4)
    call("MulDiv")                                                    # ecx, edx, r8d already = a, b, c
    rip(OUT_MULDIV, b"\x89\x05")                                      # mov [rip+out], eax
    call("GetCurrentProcessId")
    rip(OUT_PID, b"\x89\x05")
    code += b"\x48\x8B\x4C\x24\x48"                                   # mov rcx, [rsp+0x48] (d)
    call("SetLastError")
    call("GetLastError")
    rip(OUT_LASTERROR, b"\x89\x05")
    code += b"\x48\xB9"                                               # mov rcx, imm64 (&MESSAGE, DIR64)
    absolutes.append(len(code))
    code += imm64(IMAGE_BASE + RDATA)
    call("lstrlenA")
    rip(OUT_STRLEN, b"\x89\x05")
    rip(OUT_MULDIV, b"\x48\x63\x05")                                  # movsxd rax, dword [rip+out]
    code += b"\x48\x83\xC4\x28\xC3"                                   # add rsp, 0x28; ret
    return bytes(code), tuple(absolutes)


def muldiv(a, b, c):
    """Independent model of Win32 MulDiv: 64-bit product, rounded half away from zero, -1 on /0 or overflow."""
    a, b, c = (((v & 0xFFFFFFFF) ^ 0x80000000) - 0x80000000 for v in (a, b, c))
    if c == 0:
        return -1
    q, r = divmod(abs(a * b), abs(c))
    q += 2 * r >= abs(c)
    q = -q if (a * b < 0) != (c < 0) else q
    return q if -2**31 <= q < 2**31 else -1


def build_imports(libraries=None, *, iat_rva=IAT_RVA, stamp=0, idata_flags=DATA_R):
    """PE32+ with .text/.rdata/.data/.idata/.reloc. `libraries`: [(dll name, [name or int ordinal, ...])];
    default = KERNEL32.dll with CALLS (in that order: the code indexes the IAT by position)."""
    libraries = [("KERNEL32.dll", CALLS)] if libraries is None else libraries
    code, absolutes = import_program()
    data = bytearray(0x1000)
    data[:2] = b"MZ"
    struct.pack_into("<I", data, 60, 0x80)
    data[0x80:0x84] = b"PE\0\0"
    struct.pack_into("<HHIIIHH", data, 0x84, 0x8664, 5, 0, 0, 0, 240, 0x2022)
    struct.pack_into("<H", data, OPT, 0x20B)
    struct.pack_into("<I", data, OPT + 16, TEXT)
    struct.pack_into("<Q", data, OPT + 24, IMAGE_BASE)
    struct.pack_into("<II", data, OPT + 32, 0x1000, 0x200)
    struct.pack_into("<II", data, OPT + 56, 0x6000, 0x400)
    struct.pack_into("<H", data, OPT + 68, 3)
    struct.pack_into("<H", data, OPT + 70, 0x0160)
    struct.pack_into("<I", data, OPT + 108, 16)

    def at(rva):  # file offset of an .idata RVA
        return rva - IDATA + IDATA_RAW
    names, slot = NAMES, 0
    for d, (library, symbols) in enumerate(libraries):
        lib_rva = names
        data[at(lib_rva):at(lib_rva) + len(library) + 1] = library.encode() + b"\0"
        names += (len(library) + 2) & ~1
        int_first, iat_first = INT_RVA + 8 * slot, iat_rva + 8 * slot
        for j, symbol in enumerate(symbols):
            if isinstance(symbol, int):
                value = (1 << 63) | symbol
            else:
                value = names
                data[at(names) + 2:at(names) + 2 + len(symbol) + 1] = symbol.encode() + b"\0"
                names += (2 + len(symbol) + 2) & ~1
            struct.pack_into("<Q", data, at(INT_RVA) + 8 * (slot + j), value)
            if IDATA <= iat_rva < IDATA + 0x400:
                struct.pack_into("<Q", data, at(iat_rva) + 8 * (slot + j), value)
        slot += len(symbols) + 1                                       # zero terminator per library
        struct.pack_into("<5I", data, at(DESC) + 20 * d, int_first, stamp, 0, lib_rva, iat_first)
    assert names <= IDATA + 0x400 and slot <= 32
    struct.pack_into("<II", data, OPT + 112 + 8 * 1, DESC, 20 * (len(libraries) + 1))   # import directory
    struct.pack_into("<II", data, OPT + 112 + 8 * 12, IAT_RVA, 8 * slot)                 # IAT directory
    block = struct.pack("<II", TEXT, 12) + struct.pack("<H", 10 << 12 | absolutes[0]) + b"\0\0"
    struct.pack_into("<II", data, OPT + 112 + 8 * 5, IMP_RELOC, len(block))
    table = OPT + 240
    for i, (name, rva, vsize, raw, raw_size, flags) in enumerate([
            (b".text", TEXT, len(code), TEXT_RAW, 0x200, CODE_RX),
            (b".rdata", RDATA, len(MESSAGE), RDATA_RAW, 0x200, DATA_R),
            (b".data", DATA, 32, DATA_RAW, 0x200, DATA_RW),
            (b".idata", IDATA, 0x400, IDATA_RAW, 0x400, idata_flags),
            (b".reloc", IMP_RELOC, len(block), IMP_RELOC_RAW, 0x200, RELOC_R)]):
        struct.pack_into("<8s6I2HI", data, table + 40 * i, name, vsize, rva, raw_size, raw, 0, 0, 0, 0, flags)
    data[TEXT_RAW:TEXT_RAW + len(code)] = code
    data[RDATA_RAW:RDATA_RAW + len(MESSAGE)] = MESSAGE
    data[IMP_RELOC_RAW:IMP_RELOC_RAW + len(block)] = block
    return data


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, help="new file to create outside the repository")
    parser.add_argument("--program", default="compute", choices=["compute", "write_rdata", "jump_to_data", "spin"])
    args = parser.parse_args()
    image = bytes(build(args.program))
    try:
        with open(args.output, "xb") as file:
            file.write(image)
    except FileExistsError:
        parser.error("output already exists; choose a new path")
    print(f"{len(image)} bytes sha256={hashlib.sha256(image).hexdigest()}")
