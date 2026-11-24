"""PE32+ structure inspection based on the public Microsoft PE/COFF specification.

No image mapping, imports resolution, DLL loading, signature verification or code execution.
"""
# SPDX-License-Identifier: MIT
import struct
from .io import FormatError, LimitError, UnsupportedError, span


def inspect(data):
    if len(data) > 32 << 20:
        raise LimitError("PE input exceeds 32 MiB policy")
    def unpack(fmt, offset):
        return struct.unpack(fmt, span(data, offset, struct.calcsize(fmt)))

    if bytes(span(data, 0, 2)) != b"MZ":
        raise FormatError("missing DOS signature")
    pe = unpack("<I", 60)[0]
    if pe < 64 or bytes(span(data, pe, 4)) != b"PE\0\0":
        raise FormatError("missing PE signature")
    machine, count, _, _, _, optional_size, characteristics = unpack("<HHIIIHH", pe + 4)
    if not 1 <= count <= 96:
        raise LimitError("section count outside inspection policy")
    opt = pe + 24
    span(data, opt, optional_size)
    if optional_size < 112:
        raise FormatError("short optional header")
    if unpack("<H", opt)[0] != 0x20B:
        raise UnsupportedError("only PE32+ optional headers are implemented")
    entry = unpack("<I", opt + 16)[0]
    image_base = unpack("<Q", opt + 24)[0]
    image_size, header_size = unpack("<II", opt + 56)
    directory_count = unpack("<I", opt + 108)[0]
    if directory_count > 16:
        raise UnsupportedError("nonstandard data-directory count")
    if optional_size < 112 + 8 * directory_count or not 0 < header_size <= len(data):
        raise FormatError("directory or header bounds")
    table = opt + optional_size
    if table + 40 * count > header_size or header_size > image_size:
        raise FormatError("section table outside headers")
    sections = []
    for i in range(count):
        name, virtual_size, rva, raw_size, raw_offset, _, _, _, _, flags = unpack("<8s6I2HI", table + 40 * i)
        extent = max(virtual_size, raw_size)
        if rva < header_size or rva + extent > image_size:
            raise FormatError("section virtual range outside image or inside headers")
        if raw_size:
            if raw_offset < header_size:
                raise FormatError("section data overlaps headers")
            span(data, raw_offset, raw_size)
        sections.append(dict(name=name.rstrip(b"\0").decode("ascii", errors="backslashreplace"),
                             rva=rva, virtual_size=virtual_size, raw_offset=raw_offset,
                             raw_size=raw_size, flags=flags))
    for key, size_key in (("rva", None), ("raw_offset", "raw_size")):
        intervals = sorted((s[key], s[key] + (s[size_key] if size_key else max(s["virtual_size"], s["raw_size"])))
                           for s in sections if (s[size_key] if size_key else max(s["virtual_size"], s["raw_size"])))
        if any(a[1] > b[0] for a, b in zip(intervals, intervals[1:])):
            raise FormatError("overlapping section ranges")

    def offset(rva, length):
        if rva < 0 or length < 0 or rva + length > image_size:
            raise FormatError("RVA outside image")
        if rva < header_size and length <= header_size - rva:
            return rva
        for section in sections:
            delta = rva - section["rva"]
            if 0 <= delta and delta + length <= section["raw_size"]:
                return section["raw_offset"] + delta
        raise FormatError("RVA has no complete file-backed range")

    def at(rva, fmt):
        return unpack(fmt, offset(rva, struct.calcsize(fmt)))

    def directory(index):
        if index >= directory_count:
            return 0, 0
        rva, size = unpack("<II", opt + 112 + 8 * index)
        if bool(rva) != bool(size):
            raise FormatError("half-empty directory")
        return rva, size

    def text(rva):
        value = bytearray()
        for n in range(257):
            byte = data[offset(rva + n, 1)]
            if byte == 0:
                try:
                    return value.decode("ascii")
                except UnicodeDecodeError as exc:
                    raise FormatError("non-ASCII import name") from exc
            value.append(byte)
        raise LimitError("import string exceeds policy")

    imports, total = [], 0
    base, size = directory(1)
    if base:
        offset(base, size)
        terminated = False
        for index in range(min(size // 20, 4096)):
            thunk, stamp, chain, name, first = at(base + 20 * index, "<5I")
            if not any((thunk, stamp, chain, name, first)):
                terminated = True
                break
            if not name or not first:
                raise FormatError("incomplete import descriptor")
            symbols = []
            lookup = thunk or first
            for j in range(8193):
                value = at(lookup + 8 * j, "<Q")[0]
                if value == 0:
                    break
                total += 1
                if total > 8192:
                    raise LimitError("import count exceeds policy")
                if value & (1 << 63):
                    if value & 0x7FFFFFFFFFFF0000:
                        raise FormatError("reserved bits in ordinal import")
                    symbols.append(dict(ordinal=value & 65535))
                else:
                    at(value, "<H")
                    symbols.append(dict(name=text(value + 2)))
                offset(first + 8 * j, 8)
            imports.append(dict(library=text(name), symbols=symbols))
        if not terminated:
            raise FormatError("unterminated import descriptors")

    relocations, unsupported_types = 0, set()
    base, size = directory(5)
    if base:
        offset(base, size)
        cursor = 0
        while cursor < size:
            if size - cursor < 8:
                raise FormatError("truncated relocation block")
            page, length = at(base + cursor, "<II")
            if length < 8 or length % 2 or length > size - cursor or page % 4096:
                raise FormatError("invalid relocation block")
            for j in range((length - 8) // 2):
                value = at(base + cursor + 8 + j * 2, "<H")[0]
                kind, within = value >> 12, value & 4095
                if kind:
                    relocations += 1
                    if relocations > 1_000_000:
                        raise LimitError("relocation count exceeds policy")
                    # Relocation targets may be zero-filled virtual memory, not file-backed.
                    target = page + within
                    if target + (8 if kind == 10 else 1) > image_size:
                        raise FormatError("relocation target outside image")
                    if kind != 10:
                        unsupported_types.add(kind)
            cursor += length

    tls = None
    base, size = directory(9)
    if base:
        if size < 40:
            raise FormatError("short TLS directory")
        offset(base, size)
        begin, end, index_va, callbacks_va, zero_fill, tls_flags = at(base, "<4Q2I")
        if end < begin or bool(begin) != bool(end):
            raise FormatError("invalid TLS template interval")
        if begin:
            offset(begin - image_base, end - begin)
        if index_va:
            index_rva = index_va - image_base
            if not 0 <= index_rva <= image_size - 4:
                raise FormatError("TLS index outside image")
        callbacks = []
        if callbacks_va:
            for n in range(257):
                callback = at(callbacks_va - image_base + 8 * n, "<Q")[0]
                if callback == 0:
                    break
                if n == 256:
                    raise LimitError("TLS callback count exceeds policy")
                rva = callback - image_base
                offset(rva, 1)
                executable = any(s["rva"] <= rva < s["rva"] + max(s["virtual_size"], s["raw_size"])
                                 and s["flags"] & 0x20000000 for s in sections)
                callbacks.append(dict(rva=rva, executable_section=bool(executable)))
        tls = dict(template_size=end - begin, zero_fill=zero_fill, flags=tls_flags,
                   index_present=bool(index_va), callbacks=callbacks)
    delay_rva, delay_size = directory(13)
    if delay_rva:
        offset(delay_rva, delay_size)
    return dict(format="PE32+", machine=machine, amd64=machine == 0x8664,
                characteristics=characteristics, image_base=image_base, image_size=image_size,
                entry_rva=entry, entry_in_executable_section=any(s["rva"] <= entry < s["rva"] + max(s["virtual_size"], s["raw_size"])
                                                               and bool(s["flags"] & 0x20000000) for s in sections),
                sections=sections, imports=imports, relocation_count=relocations,
                relocation_types_outside_dir64=sorted(unsupported_types), tls=tls,
                delay_imports_present=bool(delay_rva),
                limitation="structural subset only; no exports, delay-import decoding, signatures, "
                           "provenance, service availability, execution safety or boot compatibility verdict")
