"""DirectX container part inventory; shader code is opaque and never executed."""
# SPDX-License-Identifier: MIT
import hashlib
import struct
from .io import FormatError, LimitError, span


def inspect(data):
    if len(data) > 32 << 20:
        raise LimitError("container exceeds 32 MiB policy")
    header = span(data, 0, 32)
    if bytes(header[:4]) != b"DXBC":
        raise FormatError("not a DirectX container")
    version, size, count = struct.unpack_from("<3I", header, 20)
    if size != len(data):
        raise FormatError("container length mismatch")
    if count > 4096:
        raise LimitError("part count exceeds policy")
    table_end = 32 + 4 * count
    span(data, 32, 4 * count)
    parts, ranges = [], []
    for i in range(count):
        offset = struct.unpack_from("<I", data, 32 + 4 * i)[0]
        if offset < table_end:
            raise FormatError("part overlaps header or directory")
        tag, length = struct.unpack("<4sI", span(data, offset, 8))
        body = span(data, offset + 8, length)
        ranges.append((offset, offset + 8 + length))
        part = dict(index=i, tag=tag.decode("ascii", errors="backslashreplace"), offset=offset,
                    size=length)
        if tag == b"HASH" and length == 20:
            part["hash_flags"] = struct.unpack_from("<I", body)[0]
            part["hash128"] = bytes(body[4:20]).hex()
        parts.append(part)
    ordered = sorted(ranges)
    if any(a[1] > b[0] for a, b in zip(ordered, ordered[1:])):
        raise FormatError("overlapping parts")
    # Hash only after validating all disjoint ranges: overlapping hostile parts
    # must not cause the same large byte range to be hashed thousands of times.
    for part in parts:
        part["sha256"] = hashlib.sha256(span(data, part["offset"] + 8, part["size"])).hexdigest()
    return dict(format="DXBC", version=version, parts=parts, container_digest_verified=False,
                limitation="part bounds and optional HASH fields only; no shader validation or root-signature inference")
