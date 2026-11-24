"""Generic single-tile bit interleaving, with no hardware tile-pattern database."""
# SPDX-License-Identifier: MIT
from .io import FormatError, LimitError


def linearize_tile(data, pattern, element_size=1):
    if not pattern or set(pattern) - {"x", "y"} or element_size < 1:
        raise FormatError("invalid bit pattern or element size")
    if len(pattern) > 16 or element_size > 16:
        raise LimitError("tile policy exceeded")
    if len(data) != (1 << len(pattern)) * element_size:
        raise FormatError("tile byte count mismatch")
    width = 1 << pattern.count("x")
    result = bytearray(len(data))
    for index in range(1 << len(pattern)):
        x = y = nx = ny = 0
        for bit, axis in enumerate(pattern):
            value = (index >> bit) & 1
            if axis == "x":
                x |= value << nx
                nx += 1
            else:
                y |= value << ny
                ny += 1
        target = (y * width + x) * element_size
        result[target:target + element_size] = data[index * element_size:(index + 1) * element_size]
    return bytes(result)
