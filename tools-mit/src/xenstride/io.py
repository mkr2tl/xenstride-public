"""Explicit-root, read-only input and bounded decompression. No execution APIs."""
# SPDX-License-Identifier: MIT
import os
from pathlib import Path, PureWindowsPath
import stat
import zlib


class FormatError(ValueError):
    """Malformed or inconsistent input; not a compatibility verdict."""


class LimitError(FormatError):
    """An inspection policy limit, not evidence of a malformed file."""


class UnsupportedError(FormatError):
    """Recognized feature outside the implemented subset."""


def span(data, offset, length):
    if offset < 0 or length < 0 or offset > len(data) - length:
        raise FormatError("range outside input")
    return memoryview(data)[offset:offset + length]


def inflate(data, limit):
    """Return (decoded, consumed); require EOF, bound output, allow trailing padding."""
    if not 0 <= limit <= 8 * 1024 * 1024:
        raise LimitError("decompression limit must be between 0 and 8 MiB")
    decoder = zlib.decompressobj()
    try:
        decoded = decoder.decompress(data, limit + 1)
    except zlib.error as exc:
        raise FormatError("invalid zlib stream") from exc
    if len(decoded) > limit:
        raise LimitError("decompressed output exceeds limit")
    if not decoder.eof:
        raise FormatError("incomplete zlib stream")
    return decoded, len(data) - len(decoder.unused_data)


class Reader:
    """No default root. Cumulative returned bytes and distinct byte coverage differ.

    Containment checks are not an OS sandbox against concurrent path replacement.
    Use files in a directory controlled by the caller. Reads never modify input.
    """
    def __init__(self, root, relative, *, budget=64 << 20, max_read=16 << 20):
        rel = str(relative).replace("\\", "/")
        if (not rel or rel.startswith("/") or ":" in rel or ".." in rel.split("/")
                or PureWindowsPath(rel).is_absolute()):
            raise FormatError("a contained relative path is required")
        if budget < 0 or max_read <= 0:
            raise LimitError("invalid read policy")
        root = Path(root).resolve(strict=True)
        target = (root / rel).resolve(strict=True)
        if not target.is_relative_to(root):
            raise FormatError("path escapes declared root")
        if not stat.S_ISREG(target.stat().st_mode):
            raise FormatError("input must be a regular file")
        self._file = target.open("rb")
        self.size = os.fstat(self._file.fileno()).st_size
        self.budget, self.max_read = budget, max_read
        self.returned = 0
        self.ranges = []

    def read(self, offset, length):
        if offset < 0 or length < 0 or offset > self.size - length:
            raise FormatError("file read outside bounds")
        if length > self.max_read or self.returned + length > self.budget:
            raise LimitError("read policy exceeded")
        self._file.seek(offset)
        data = self._file.read(length)
        self.returned += len(data)
        if len(data) != length:
            raise FormatError("input changed or short read")
        if length:
            merged = []
            lo, hi = offset, offset + length
            for a, b in self.ranges:
                if b < lo:
                    merged.append((a, b))
                elif hi < a:
                    merged.append((lo, hi))
                    lo, hi = a, b
                else:
                    lo, hi = min(a, lo), max(b, hi)
            merged.append((lo, hi))
            self.ranges = merged
        return data

    @property
    def distinct_bytes(self):
        return sum(end - start for start, end in self.ranges)

    def close(self):
        self._file.close()

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        self.close()
