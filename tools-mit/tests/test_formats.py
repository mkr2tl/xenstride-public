"""Owned synthetic byte fixtures. None contains executable instructions or assets."""
# SPDX-License-Identifier: MIT
import hashlib
import importlib.metadata
import json
from pathlib import Path
import random
import struct
import subprocess
import sys
import tempfile
import unittest
import zlib

from xenstride import dxbc, pe, rpf9
from xenstride.io import FormatError, LimitError, Reader, UnsupportedError, inflate
from xenstride.rds5 import Pool
from xenstride.tiling import linearize_tile


def pe_bytes():
    data = bytearray(0x800)
    data[:2] = b"MZ"
    struct.pack_into("<I", data, 60, 0x80)
    data[0x80:0x84] = b"PE\0\0"
    struct.pack_into("<HHIIIHH", data, 0x84, 0x8664, 1, 0, 0, 0, 240, 0x2022)
    opt = 0x98
    struct.pack_into("<H", data, opt, 0x20B)
    struct.pack_into("<I", data, opt + 16, 0x1000)
    struct.pack_into("<Q", data, opt + 24, 0x140000000)
    struct.pack_into("<II", data, opt + 56, 0x2000, 0x200)
    struct.pack_into("<I", data, opt + 108, 16)
    for index, rva, size in [(1, 0x1100, 40), (5, 0x1200, 12), (9, 0x1240, 40)]:
        struct.pack_into("<II", data, opt + 112 + index * 8, rva, size)
    struct.pack_into("<8s6I2HI", data, opt + 240, b".sample", 0x600, 0x1000, 0x600, 0x200, 0, 0, 0, 0, 0xE0000020)
    struct.pack_into("<5I", data, 0x300, 0x1140, 0, 0, 0x1180, 0x1160)
    struct.pack_into("<3Q", data, 0x340, 0x1190, (1 << 63) | 7, 0)
    data[0x380:0x38B] = b"fixture.dll"
    data[0x390:0x39D] = b"\0\0FixtureCall"
    struct.pack_into("<IIHH", data, 0x400, 0x1000, 12, 0xA080, 0)
    base = 0x140000000
    struct.pack_into("<4Q2I", data, 0x440, base + 0x1300, base + 0x1308, base + 0x1310, base + 0x1320, 16, 0)
    struct.pack_into("<2Q", data, 0x520, base + 0x1000, 0)
    return data


def dxbc_bytes(parts):
    directory_end = 32 + len(parts) * 4
    payload, offsets = bytearray(), []
    for tag, body in parts:
        offsets.append(directory_end + len(payload))
        payload.extend(struct.pack("<4sI", tag, len(body)) + body)
    size = directory_end + len(payload)
    return (struct.pack("<4s16s3I", b"DXBC", bytes(16), 1, size, len(parts))
            + struct.pack("<%dI" % len(offsets), *offsets) + payload)


def texture(width=64, height=64, start=4096, array=1, mip=0, mode=1):
    return struct.pack("<4I", (width - 1) | ((height - 1) << 14) | (mip << 28),
                       (61 << 3) | ((array - 1) << 10) | (mode << 24), start // 4096, 0)


def pool_bytes(blocks, descriptors=()):
    metadata = (b"ETPF" + struct.pack("<3I", 1, len(descriptors), 0) + b"".join(descriptors)).ljust(4096, b"\0")
    streams = [(0, metadata), *blocks]
    position, logical, slots, stored = 256, 0, [], []
    for i, (codec, body) in enumerate(streams):
        packed = zlib.compress(body) if codec == 8 else body if codec == 0 else bytes(256)
        packed = packed.ljust((len(packed) + 255) // 256 * 256, b"\0")
        position += len(packed)
        logical += len(body)
        following = streams[i + 1][0] if i + 1 < len(streams) else 0
        slots.append(position // 256 | (following << 26) | ((logical // 4096) << 30))
        stored.append(packed)
    header = struct.pack("<4s5I", b"RDS5", len(slots) + 1, logical & 0xFFFFFFFF, logical >> 32, 1, 0)
    return (header + struct.pack("<%dQ" % (len(slots) + 1), *slots, 0)).ljust(256, b"\0") + b"".join(stored)


class TemporaryInput(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def reader(self, data, **kwargs):
        (self.root / "sample").write_bytes(data)
        reader = Reader(self.root, "sample", **kwargs)
        self.addCleanup(reader.close)
        return reader


class IOTests(TemporaryInput):
    def test_read_only_and_distinct_coverage(self):
        reader = self.reader(bytes(range(100)))
        reader.read(20, 20)
        reader.read(0, 25)
        reader.read(60, 10)
        reader.read(35, 30)
        self.assertEqual(reader.returned, 85)
        self.assertEqual(reader.distinct_bytes, 70)
        self.assertEqual(reader.ranges, [(0, 70)])
        self.assertEqual((self.root / "sample").read_bytes(), bytes(range(100)))

    def test_budget_and_read_limit(self):
        reader = self.reader(bytes(30), budget=20, max_read=15)
        with self.assertRaises(LimitError): reader.read(0, 16)
        reader.read(0, 15)
        with self.assertRaises(LimitError): reader.read(0, 6)

    def test_ranges_are_strict(self):
        reader = self.reader(bytes(10))
        for offset, length in [(-1, 1), (0, -1), (10, 1), (11, 0)]:
            with self.subTest(offset=offset, length=length), self.assertRaises(FormatError):
                reader.read(offset, length)

    def test_path_escapes(self):
        for path in ["../outside", "/absolute", "C:/outside", "..\\outside", "\\\\server\\share"]:
            with self.subTest(path=path), self.assertRaises(FormatError): Reader(self.root, path)

    def test_symlink_escape_if_available(self):
        with tempfile.TemporaryDirectory() as outside:
            target = Path(outside) / "outside"
            target.write_bytes(b"outside")
            link = self.root / "link"
            try:
                link.symlink_to(target)
            except (OSError, NotImplementedError):
                self.skipTest("OS does not permit symlink creation for this user")
            with self.assertRaises(FormatError): Reader(self.root, "link")

    def test_inflate_exact_and_padding(self):
        packed = zlib.compress(b"synthetic" * 100)
        data, used = inflate(packed + b"padding", 900)
        self.assertEqual(data, b"synthetic" * 100)
        self.assertEqual(used, len(packed))

    def test_inflate_refuses_bomb(self):
        with self.assertRaises(LimitError): inflate(zlib.compress(bytes(100000)), 16)

    def test_inflate_truncated_and_invalid(self):
        for packed in [b"not-zlib", zlib.compress(b"body")[:-1]]:
            with self.assertRaises(FormatError): inflate(packed, 100)

    def test_inflate_policy(self):
        for limit in [-1, (8 << 20) + 1]:
            with self.assertRaises(LimitError): inflate(b"", limit)


class PETests(unittest.TestCase):
    def test_imports_relocations_tls_and_entry(self):
        value = pe.inspect(pe_bytes())
        self.assertTrue(value["amd64"])
        self.assertEqual(value["imports"], [dict(library="fixture.dll", symbols=[dict(name="FixtureCall"), dict(ordinal=7)])])
        self.assertEqual(value["relocation_count"], 1)
        self.assertEqual(value["tls"]["template_size"], 8)
        self.assertEqual(value["tls"]["callbacks"], [dict(rva=0x1000, executable_section=True)])
        self.assertTrue(value["entry_in_executable_section"])

    def test_bad_signatures(self):
        for offset in [0, 0x80]:
            data = pe_bytes()
            data[offset] = 0
            with self.assertRaises(FormatError): pe.inspect(data)

    def test_truncations(self):
        for length in [0, 1, 60, 128, 200, 500, 1000]:
            with self.subTest(length=length), self.assertRaises(FormatError): pe.inspect(pe_bytes()[:length])

    def test_section_outside_file(self):
        data = pe_bytes()
        struct.pack_into("<I", data, 0x98 + 240 + 20, 0x700)
        with self.assertRaises(FormatError): pe.inspect(data)

    def test_import_pointer_outside_image(self):
        data = pe_bytes()
        struct.pack_into("<I", data, 0x300 + 12, 0xFFFFFFF0)
        with self.assertRaises(FormatError): pe.inspect(data)

    def test_invalid_relocation_block(self):
        data = pe_bytes()
        struct.pack_into("<I", data, 0x404, 7)
        with self.assertRaises(FormatError): pe.inspect(data)

    def test_non_dir64_type_is_reported(self):
        data = pe_bytes()
        struct.pack_into("<H", data, 0x408, 0x3080)
        self.assertEqual(pe.inspect(data)["relocation_types_outside_dir64"], [3])

    def test_nonexecutable_tls_callback_is_reported(self):
        data = pe_bytes()
        struct.pack_into("<I", data, 0x98 + 240 + 36, 0xC0000040)
        self.assertFalse(pe.inspect(data)["tls"]["callbacks"][0]["executable_section"])

    def test_tls_pointer_outside_image(self):
        data = pe_bytes()
        struct.pack_into("<Q", data, 0x440 + 24, 1)
        with self.assertRaises(FormatError): pe.inspect(data)

    def test_pe32_refused(self):
        data = pe_bytes()
        struct.pack_into("<H", data, 0x98, 0x10B)
        with self.assertRaises(UnsupportedError): pe.inspect(data)


class DXBCTests(unittest.TestCase):
    def test_duplicate_tags_preserved(self):
        result = dxbc.inspect(dxbc_bytes([(b"XDAT", b"first"), (b"XDAT", b"second")]))
        self.assertEqual([p["tag"] for p in result["parts"]], ["XDAT", "XDAT"])
        self.assertNotEqual(result["parts"][0]["sha256"], result["parts"][1]["sha256"])

    def test_hash_metadata(self):
        result = dxbc.inspect(dxbc_bytes([(b"HASH", struct.pack("<I", 7) + bytes(range(16)))]))
        self.assertEqual(result["parts"][0]["hash128"], bytes(range(16)).hex())
        self.assertFalse(result["container_digest_verified"])

    def test_overlap_refused(self):
        data = bytearray(dxbc_bytes([(b"AAAA", b"x"), (b"BBBB", b"y")]))
        data[36:40] = data[32:36]
        with self.assertRaises(FormatError): dxbc.inspect(data)

    def test_bad_size_and_short_parts(self):
        data = dxbc_bytes([(b"AAAA", b"x")])
        for bad in [data[:-1], data + b"x", data[:31]]:
            with self.assertRaises(FormatError): dxbc.inspect(bad)

    def test_part_count_cap(self):
        data = bytearray(dxbc_bytes([]))
        struct.pack_into("<I", data, 28, 4097)
        with self.assertRaises(LimitError): dxbc.inspect(data)

    def test_overlap_rejected_before_payload_hashing(self):
        from unittest.mock import patch
        data = bytearray(dxbc_bytes([(b"AAAA", b"x"), (b"BBBB", b"y")]))
        data[36:40] = data[32:36]
        with patch.object(dxbc.hashlib, "sha256", side_effect=AssertionError("must not hash overlapping parts")):
            with self.assertRaises(FormatError): dxbc.inspect(data)


class RDS5Tests(TemporaryInput):
    def test_next_codec_raw_zlib_and_unknown(self):
        pool = Pool(self.reader(pool_bytes([(8, b"x" * 4096), (3, bytes(4096)), (15, bytes(4096))])))
        self.assertEqual([b.codec for b in pool.blocks], [0, 8, 3, 15])
        self.assertEqual(pool.decode(1), b"x" * 4096)
        self.assertTrue(pool.decode(0).startswith(b"ETPF"))
        for index in [2, 3]:
            with self.assertRaises(UnsupportedError): pool.decode(index)

    def test_descriptors_preserve_raw_fields(self):
        pool = Pool(self.reader(pool_bytes([(0, bytes(4096)), (8, bytes(4096))],
                     [texture(width=7, height=9, array=6, mip=4, mode=9), texture(start=8192)])))
        desc = pool.texture_descriptors()["textures"]
        self.assertEqual((desc[0]["width"], desc[0]["height"], desc[0]["array_size"], desc[0]["mip_field"]), (7, 9, 6, 4))
        self.assertEqual(desc[0]["logical_end_upper"], 8192)

    def test_etpf_start_outside_payload(self):
        pool = Pool(self.reader(pool_bytes([(0, bytes(4096))], [texture(start=8192)])))
        with self.assertRaises(FormatError): pool.texture_descriptors()

    def test_table_nonmonotonic(self):
        data = bytearray(pool_bytes([]))
        struct.pack_into("<Q", data, 24, 1 | (1 << 30))
        with self.assertRaises(FormatError): Pool(self.reader(data))

    def test_final_ends_must_match(self):
        data = pool_bytes([]) + bytes(256)
        with self.assertRaises(FormatError): Pool(self.reader(data))

    def test_nonzero_after_sentinel(self):
        data = bytearray(pool_bytes([(0, bytes(4096))]))
        struct.pack_into("<Q", data, 24, 0)
        with self.assertRaises(FormatError): Pool(self.reader(data))

    def test_uninterpreted_tail_after_valid_terminator_is_reported(self):
        data = bytearray(pool_bytes([]))
        struct.pack_into("<I", data, 4, 3)
        struct.pack_into("<Q", data, 40, 123)
        pool = Pool(self.reader(data))
        self.assertEqual(len(pool.blocks), 1)
        self.assertEqual(pool.summary()["uninterpreted_table_bytes"], 8)
        self.assertTrue(pool.summary()["nonzero_table_tail"])

    def test_physical_bit25_and_40bit_logical_size_without_large_file(self):
        class SparseHeader:
            size = ((1 << 25) + 1) * 256
            def read(self, offset, length):
                logical = (1 << 32) + 4096
                data = struct.pack("<4s5I", b"RDS5", 2, 4096, 1, 1, 0)
                slot = ((1 << 25) + 1) | ((logical // 4096) << 30)
                data += struct.pack("<2Q", slot, 0)
                return data[offset:offset + length]
        pool = Pool(SparseHeader())
        self.assertEqual(pool.logical_size, (1 << 32) + 4096)
        self.assertEqual(pool.blocks[0].physical_end, SparseHeader.size)

    def test_decode_limit(self):
        pool = Pool(self.reader(pool_bytes([(8, bytes(4096))])))
        with self.assertRaises(LimitError): pool.decode(1, limit=100)

    def test_decode_size_mismatch(self):
        data = bytearray(pool_bytes([(8, bytes(4096))]))
        position = 256 + 4096
        packed = zlib.compress(b"short")
        data[position:position + 256] = packed.ljust(256, b"\0")
        with self.assertRaises(FormatError): Pool(self.reader(data)).decode(1)


class RPFTests(TemporaryInput):
    def test_two_independent_pages(self):
        a, b = zlib.compress(b"a" * 65536), zlib.compress(b"b" * 100)
        data = b"9FPR" + bytes(60) + a + bytes(7) + b
        result = rpf9.scan(self.reader(data), length=len(data))
        self.assertEqual([p["decoded_size"] for p in result["pages"]], [65536, 100])
        self.assertEqual(result["pages"][0]["sha256"], hashlib.sha256(b"a" * 65536).hexdigest())

    def test_cross_range_stream_not_claimed(self):
        data = b"9FPR" + zlib.compress(b"x" * 100)
        self.assertFalse(rpf9.scan(self.reader(data), length=len(data) - 1)["pages"])

    def test_oversized_output_not_accepted(self):
        self.assertFalse(rpf9.scan(self.reader(b"9FPR" + zlib.compress(bytes(65537))))["pages"])

    def test_candidate_limit(self):
        result = rpf9.scan(self.reader(b"9FPR" + b"\x78\x9c\x00" * 30), max_candidates=2)
        self.assertEqual(result["candidates"], 2)
        self.assertEqual(result["stop_reason"], "candidate_or_page_limit")

    def test_no_magic_only_claim(self):
        self.assertFalse(rpf9.scan(self.reader(b"9FPR" + b"PSIN RSC9 XML"))["pages"])

    def test_bad_signature_and_policy(self):
        reader = self.reader(b"nope")
        with self.assertRaises(FormatError): rpf9.scan(reader)
        with self.assertRaises(LimitError): rpf9.scan(reader, length=9 << 20)


class TilingTests(unittest.TestCase):
    def test_known_nonidentity_permutation(self):
        self.assertEqual(linearize_tile(bytes(range(16)), "yyxx"), bytes([0, 4, 8, 12, 1, 5, 9, 13, 2, 6, 10, 14, 3, 7, 11, 15]))

    def test_nonuniform_multibyte_roundtrip_and_negative_control(self):
        rng = random.Random(2)  # instantiate once; the former test produced a constant image
        image = bytes(rng.getrandbits(8) for _ in range(16 * 4))
        self.assertGreater(len(set(image)), 16)
        # Explicit independent 4x4 transposition; no production coordinate helper.
        tiled = b"".join(image[(y * 4 + x) * 4:(y * 4 + x + 1) * 4] for x in range(4) for y in range(4))
        self.assertNotEqual(tiled, image)  # an identity implementation must fail
        self.assertEqual(linearize_tile(tiled, "yyxx", 4), image)
        self.assertNotEqual(linearize_tile(tiled, "xxyy", 4), image)

    def test_bad_pattern_size_and_cap(self):
        for pattern, data in [("xyz", bytes(8)), ("", bytes(1)), ("xy", bytes(3))]:
            with self.assertRaises(FormatError): linearize_tile(data, pattern)
        with self.assertRaises(LimitError): linearize_tile(b"", "x" * 17)


class InstalledCLITests(TemporaryInput):
    def run_cli(self, *args):
        # System Python runs only the newly installed, reviewed Python package.
        return subprocess.run([sys.executable, "-I", "-B", "-m", "xenstride", *map(str, args)],
                              capture_output=True, text=True, cwd=self.root, timeout=20)

    def test_version_installed_outside_checkout(self):
        result = self.run_cli("--version")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), importlib.metadata.version("xenstride"))

    def test_pe_cli_and_input_unchanged(self):
        file = self.root / "input"
        data = pe_bytes()
        file.write_bytes(data)
        result = self.run_cli("pe", file)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)["coverage"]["distinct_bytes"], len(data))
        self.assertEqual(file.read_bytes(), data)

    def test_missing_file_has_no_private_path_in_diagnostic(self):
        result = self.run_cli("pe", self.root / "absent")
        self.assertEqual(result.returncode, 2)
        self.assertNotIn(str(self.root), result.stderr)

    def test_invalid_and_policy_are_different(self):
        file = self.root / "input"
        file.write_bytes(b"bad")
        self.assertEqual(json.loads(self.run_cli("pe", file).stderr)["error"], "invalid_or_unreadable")
        file.write_bytes(b"9FPR")
        self.assertEqual(json.loads(self.run_cli("rpf9", file, "--scan-bytes", 9 << 20).stderr)["error"], "limit")

    def test_rds5_cli(self):
        file = self.root / "input"
        file.write_bytes(pool_bytes([(8, bytes(4096))], [texture()]))
        result = self.run_cli("rds5", file, "--etpf")
        self.assertEqual(result.returncode, 0, result.stderr)
        value = json.loads(result.stdout)
        self.assertEqual(value["etpf"]["textures"][0]["width"], 64)
        self.assertLess(value["coverage"]["distinct_bytes"], len(file.read_bytes()))

    def test_dxbc_and_rpf_cli(self):
        file = self.root / "input"
        file.write_bytes(dxbc_bytes([(b"XDAT", b"owned synthetic metadata")]))
        result = self.run_cli("dxbc", file)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)["parts"][0]["tag"], "XDAT")
        file.write_bytes(b"9FPR" + zlib.compress(b"owned" * 20))
        result = self.run_cli("rpf9", file)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)["pages"][0]["decoded_size"], 100)


class MalformedInputTests(unittest.TestCase):
    def test_seeded_mutations_return_or_raise_documented_errors(self):
        rng = random.Random(1729)
        fixtures = [(pe.inspect, bytes(pe_bytes())),
                    (dxbc.inspect, bytes(dxbc_bytes([(b"XDAT", b"fixture")])))]
        for inspect, fixture in fixtures:
            for index in range(120):
                data = bytearray(fixture)
                for _ in range(1 + index % 4):
                    data[rng.randrange(len(data))] = rng.randrange(256)
                with self.subTest(parser=inspect.__module__, case=index):
                    try:
                        inspect(data)
                    except FormatError:
                        pass


if __name__ == "__main__":
    unittest.main()  # all test classes are defined before this entry point
