"""RDS5 block boundaries and ETPF descriptors; no pixel decoding or extraction.

Format reference: paryzius/ttpf-viewer README, MIT. Independently written here.
"""
# SPDX-License-Identifier: MIT
from bisect import bisect_right
from dataclasses import asdict, dataclass
import struct

from .io import FormatError, LimitError, UnsupportedError, inflate


@dataclass(frozen=True)
class Block:
    physical_start: int
    physical_end: int
    logical_start: int
    logical_end: int
    codec: int
    following_codec: int
    reserved_bits: int


class Pool:
    def __init__(self, reader):
        self.reader = reader
        magic, count, low, high, etpf_units, reserved = struct.unpack("<4s5I", reader.read(0, 24))
        if magic != b"RDS5" or not count:
            raise FormatError("invalid RDS5 header")
        if count > 131072:
            raise LimitError("RDS5 slot count exceeds policy")
        self.logical_size = low | ((high & 255) << 32)
        self.etpf_offset = etpf_units * 256
        self.reserved_header = [high >> 8, reserved]
        if not self.logical_size or not 24 <= self.etpf_offset < reader.size:
            raise FormatError("invalid RDS5 extents")
        self.declared_slots = count
        self.bounded_slots = min(count, (self.etpf_offset - 24) // 8)
        table = reader.read(24, self.bounded_slots * 8)
        self.blocks = []
        self.uninterpreted_table_bytes = 0
        self.nonzero_table_tail = False
        physical, logical, codec = self.etpf_offset, 0, 0
        for i in range(self.bounded_slots):
            value = struct.unpack_from("<Q", table, i * 8)[0]
            if value == 0:
                tail = table[(i + 1) * 8:]
                self.uninterpreted_table_bytes = len(tail)
                self.nonzero_table_tail = bool(any(tail))
                break
            end = (value & ((1 << 26) - 1)) * 256
            logical_end = ((value >> 30) & ((1 << 22) - 1)) * 4096
            following = (value >> 26) & 15
            if not (physical < end <= reader.size and logical < logical_end <= self.logical_size):
                raise FormatError("RDS5 block extents are non-monotonic or outside input")
            self.blocks.append(Block(physical, end, logical, logical_end, codec, following, value >> 52))
            physical, logical, codec = end, logical_end, following
        if not self.blocks or physical != reader.size or logical != self.logical_size:
            raise FormatError("RDS5 final ends do not match physical and logical sizes")
        self._logical_ends = [b.logical_end for b in self.blocks]

    def decode(self, index, *, limit=8 << 20):
        if not 0 <= index < len(self.blocks):
            raise FormatError("block index outside table")
        if not 0 < limit <= 8 << 20:
            raise LimitError("block decode cap exceeds policy")
        block = self.blocks[index]
        size = block.logical_end - block.logical_start
        if block.codec not in (0, 8):
            kind = "BCPack class" if 1 <= block.codec <= 7 else "unknown storage class"
            raise UnsupportedError(kind + ": " + str(block.codec))
        if size > limit:
            raise LimitError("logical block exceeds decode cap")
        if block.codec == 0:
            if size > block.physical_end - block.physical_start:
                raise FormatError("raw block shorter than logical block")
            return self.reader.read(block.physical_start, size)
        raw = self.reader.read(block.physical_start, block.physical_end - block.physical_start)
        decoded, _ = inflate(raw, size)
        if len(decoded) != size:
            raise FormatError("decoded block length differs from table")
        return decoded

    def texture_descriptors(self):
        """Parse only first raw block. Logical end is an upper bound, not texture size."""
        first = self.blocks[0]
        head = self.reader.read(first.physical_start, 16)
        magic, version, count, section_field = struct.unpack("<4s3I", head)
        if magic != b"ETPF":
            raise FormatError("missing ETPF at declared physical position")
        if count > 131072:
            raise LimitError("ETPF descriptor count exceeds policy")
        length = 16 + 16 * count
        if length > min(first.logical_end, first.physical_end - first.physical_start):
            raise FormatError("ETPF descriptors cross first block")
        raw = self.reader.read(first.physical_start + 16, 16 * count)
        textures = []
        for i in range(count):
            a, b, c, d = struct.unpack_from("<4I", raw, 16 * i)
            textures.append(dict(index=i, width=(a & 16383) + 1, height=((a >> 14) & 16383) + 1,
                                 mip_field=a >> 28, dxgi=(b >> 3) & 127, array_size=((b >> 10) & 1023) + 1,
                                 tile_mode=(b >> 24) & 15, logical_start=(c & 0x1FFFFF) * 4096,
                                 raw_words=[a, b, c, d]))
        starts = sorted({t["logical_start"] for t in textures})
        for t in textures:
            start = t["logical_start"]
            j = bisect_right(starts, start)
            upper = starts[j] if j < len(starts) else self.logical_size
            if not first.logical_end <= start < upper <= self.logical_size:
                raise FormatError("texture start or upper bound outside payload")
            t["logical_end_upper"] = upper
        return dict(version=version, section_field=section_field, textures=textures,
                    limitation="descriptor fields only; no pixels, mip layout, tiling or material relationships")

    def summary(self):
        codecs = {}
        for b in self.blocks:
            key = str(b.codec)
            codecs[key] = codecs.get(key, 0) + 1
        return dict(format="RDS5", logical_size=self.logical_size, etpf_offset=self.etpf_offset,
                    declared_slots=self.declared_slots, inspected_slots=self.bounded_slots,
                    storage_classes=codecs, reserved_header=self.reserved_header,
                    uninterpreted_table_bytes=self.uninterpreted_table_bytes,
                    nonzero_table_tail=self.nonzero_table_tail,
                    blocks=[asdict(b) for b in self.blocks])
