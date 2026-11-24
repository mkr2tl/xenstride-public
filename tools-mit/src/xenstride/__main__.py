"""Metadata-only command line; inputs opened read-only, JSON written to stdout."""
# SPDX-License-Identifier: MIT
import argparse
import json
from pathlib import Path
import sys
from . import __version__, dxbc, pe, rpf9
from .io import FormatError, LimitError, Reader, UnsupportedError
from .rds5 import Pool


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", action="version", version=__version__)
    parser.add_argument("format", choices=["pe", "dxbc", "rds5", "rpf9"])
    parser.add_argument("input", type=Path)
    parser.add_argument("--etpf", action="store_true", help="also inspect ETPF descriptors (RDS5 only)")
    parser.add_argument("--start", type=int, default=0, help="RPF9 scan start")
    parser.add_argument("--scan-bytes", type=int, default=1 << 20, help="RPF9 scan length, max 8 MiB")
    args = parser.parse_args(argv)
    try:
        path = args.input.absolute()
        with Reader(path.parent, path.name, budget=64 << 20, max_read=32 << 20) as reader:
            if args.format == "rds5":
                pool = Pool(reader)
                result = pool.summary()
                if args.etpf:
                    result["etpf"] = pool.texture_descriptors()
            elif args.format == "rpf9":
                result = rpf9.scan(reader, start=args.start, length=args.scan_bytes)
            else:
                if reader.size > 32 << 20:
                    raise LimitError("PE/DXBC file exceeds 32 MiB inspection cap")
                data = reader.read(0, reader.size)
                result = (pe.inspect if args.format == "pe" else dxbc.inspect)(data)
            result["coverage"] = dict(file_size=reader.size, bytes_returned=reader.returned,
                                      distinct_bytes=reader.distinct_bytes, ranges=reader.ranges)
        print(json.dumps(result, indent=2))
        return 0
    except (FormatError, OSError) as exc:
        category = "limit" if isinstance(exc, LimitError) else "unsupported" if isinstance(exc, UnsupportedError) else "invalid_or_unreadable"
        # Do not disclose a full host path from OSError in the normal diagnostic.
        message = "input cannot be opened or read" if isinstance(exc, OSError) else str(exc)
        print(json.dumps(dict(error=category, message=message)), file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
