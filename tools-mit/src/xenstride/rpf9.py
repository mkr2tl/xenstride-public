"""Bounded independent zlib stream discovery, not an archive-index decoder."""
# SPDX-License-Identifier: MIT
import hashlib
from .io import FormatError, LimitError, inflate


def scan(reader, *, start=0, length=1 << 20, max_candidates=4096, max_pages=1024):
    if length <= 0 or length > 8 << 20 or not 1 <= max_candidates <= 4096 or not 1 <= max_pages <= 1024:
        raise LimitError("scan policy outside supported bounds")
    if not 0 <= start <= reader.size:
        raise FormatError("scan start outside file")
    if reader.read(0, 4) != b"9FPR":
        raise FormatError("unrecognized RPF9 signature")
    stop = min(reader.size, start + length)
    data = reader.read(start, stop - start)
    i, candidates, pages = 0, 0, []
    reason = "range_complete"
    while i + 1 < len(data):
        a, b = data[i], data[i + 1]
        if a & 15 != 8 or a >> 4 > 7 or ((a << 8) | b) % 31 or b & 32:
            i += 1
            continue
        if candidates >= max_candidates or len(pages) >= max_pages:
            reason = "candidate_or_page_limit"
            break
        candidates += 1
        try:
            decoded, used = inflate(memoryview(data)[i:i + 65536], 65536)
        except (FormatError, LimitError):
            i += 1
            continue
        pages.append(dict(offset=start + i, compressed_size=used, decoded_size=len(decoded),
                          sha256=hashlib.sha256(decoded).hexdigest()))
        i += used
    return dict(format="RPF9", scan_start=start, scan_stop=stop, stop_reason=reason,
                candidates=candidates, pages=pages,
                limitation="only complete zlib streams within the scan range, <=64 KiB input/output; "
                           "no resource boundaries, index, encryption, grouping or names are inferred")
