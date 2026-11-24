# Technical scope

All format operations are static. The only exception is the opt-in runtime
prototype described below, which launches one contained Python child to execute
owned test images. There are no GPU, network, decryption or signature-changing
operations in the package. The format CLI emits metadata, not decoded resource
files. Nothing in the package claims a boot verdict.

| Module | Implemented | Not implemented |
| --- | --- | --- |
| PE32+ | Header/section bounds, ordinary named/ordinal imports, relocation entries/types, TLS template/callback metadata | Full Windows validation, exports, delay imports, exception/unwind tables, signature checking, service resolution, execution |
| DXBC | Container length, ordered parts including duplicate tags, nonoverlap, SHA-256 and optional HASH fields | Container digest verification, shader validation, PSV interpretation, root signatures, PSO construction |
| RDS5 | 40-bit logical size, 26-bit physical end units, following-block codec, monotonic/final bounds, raw/zlib block API | BCPack, unknown codecs, automatic full-pool decoding |
| ETPF | Descriptor bounds, dimensions, DXGI number, raw mip field, array size, tile field, logical-start upper bounds | Accurate byte allocation, mip layout, format support validation, pixel decoding, texture/material relations |
| RPF9 | Explicit bounded range, complete zlib streams up to 64 KiB compressed/decompressed, hashes and physical offsets | Archive index, resource segmentation/names, cross-range streams, encrypted regions, grouping or full archive coverage |
| Tiling | One generic caller-supplied x/y bit pattern | Xbox tile-pattern selection or texture reconstruction |

## Runtime prototype (`xenstride.runtime`, Windows x64)

What executes: the entry point of a PE32+ image, as real x64 code on the host CPU,
called through ctypes with the test ABI `uint64 f(uint64, uint64, uint64, uint64)`.
It is neither DllMain nor a process entry, and no loader callbacks run.

Loading steps, in a child process only:
1. `plan` (pure Python) validates the image with `pe.inspect` and refuses anything
   outside the subset. It does this in the parent, before any process starts.
2. The child receives the image bytes on stdin and calls `VirtualAlloc` for
   SizeOfImage, at the preferred base or, with `relocate`, elsewhere after reserving
   the preferred range.
3. `layout` (pure Python) builds the image: headers, section raw data
   (min(raw, virtual) bytes), zero fill and DIR64 relocations. The child copies it
   into the allocation.
4. Protections are applied with `VirtualProtect`: NOACCESS for gaps, read-only
   headers, and per section RX, X, RW, R or NOACCESS from its characteristics.
   `FlushInstructionCache` follows, then the call.
5. Optional 64-bit reads of readable RVAs after the call (`--observe`).

Refused with `UnsupportedError`:
- non-AMD64 machines;
- imports, delay imports, TLS, exception (.pdata) and CLR directories;
- relocation types other than DIR64;
- sub-page section alignment;
- write+execute sections;
- an entry outside executable sections;
- moving an image marked RELOCS_STRIPPED.

Ignored, neither applied nor checked: load config/CFG metadata, security
directory, resources, exports, DllCharacteristics policy bits.

Containment: a Job Object with these limits:
- kill on job close;
- ActiveProcessLimit 1;
- die on unhandled exception;
- 256 MiB process memory;
- 10 s user CPU time;
- a wall-clock timeout that terminates the job.

The child is started detached, with no console, so no conhost joins the job. It
is started from the base interpreter rather than a venv launcher, which would
otherwise start the real interpreter outside the job. It waits on stdin until it
has been assigned. Native faults inside the call are reported by ctypes' SEH
handler. The fault RVA is derived from the reported address, because ctypes labels
execute (DEP) faults as "writing". This is test containment, not a security
boundary: same user token, file system and network access.

Unknown RDS5 header/slot bits and ETPF words are retained. An ETPF end is the next
distinct start or logical file end: an upper bound, not a proven texture size.
The mip nibble stays raw; no generic cube/mip formula is asserted.

`Reader(root, relative)` has no machine-specific defaults. It checks containment
after symlink resolution, opens `rb`, bounds every read, and records merged byte
ranges. Returned-byte totals include repeated reads; distinct coverage does not.
It is not an OS sandbox against concurrent file replacement. Use caller-controlled
directories and stable files. Python parsers have not undergone a security audit;
use process/OS isolation for hostile input.

Policy limits are reported separately from malformed input. CLI PE/DXBC input cap:
32 MiB. Reader budget: 64 MiB. RDS5 slots/ETPF descriptors: 131072 each; block
decompression: 8 MiB. RPF scan: default 1 MiB, maximum 8 MiB, 4096 candidates,
1024 streams. RPF rejected candidates may be incomplete, oversized or false
headers; absence of a result does not prove absence of resources. No percentages
of semantic understanding are emitted.

## Comparison with the earlier research tools

This is a smaller release, not a replacement for the private research suite.
Differential checks used owned synthetic data, never game resources.

| Earlier capability | v0.1 difference |
| --- | --- |
| PE preflight: PE32/PE32+, exports, delay imports, Rich/PDB, version/certificate hints, host dependency classification | Only the documented PE32+ subset remains. On the shared fixture, entry/base, ordinary imports and TLS callback RVAs agree. No host probing or compatibility verdict. |
| RDS5 logical-range reads spanning blocks; 32 MiB block decode cap | Single-block decoding only, 8 MiB cap. Shared raw/zlib block contents and boundaries agree. No evidence database. |
| ETPF tight-size checks, known formats and tiled top-level reconstruction | Raw descriptor fields and upper bounds only. No format-validity, mip, cube layout or reconstruction claim. Shared descriptor fields agree. |
| RPF cross-window discovery, larger streams, checkpoints, page grouping, PSIN/XML classification | Bounded scan only. Shared page offsets/lengths/hashes agree. A 65536-byte incompressible fixture compresses to 65562 bytes: the earlier reader finds it; v0.1's 65536-byte compressed-input cap intentionally excludes it. |
| DXBC/PS09 shader-resource bindings and pipeline contracts | Only DXBC parts and HASH metadata remain; duplicate part tags are preserved. No PSV0/PS09 interpretation. |
| Hardware tile patterns, BC7, shader translation, GPU validation | Excluded. The small tiling utility handles a caller-supplied abstract bit pattern only. |

One unintended rejection was corrected: bytes after a valid RDS5 zero terminator
are reported as uninterpreted, not required to be zero. Final block ends are
still checked. This does not establish a meaning for the trailing bytes.
Reader changes are deliberate: an explicit root, strict EOF bounds and an 8 MiB
inflate cap replace machine defaults, clamped EOF reads and larger limits.

No original implementation or third-party code was added during this review.
Common field masks/offsets and bit-interleaving arithmetic follow the referenced
formats and algorithms; matching these facts is not evidence of an independent
clean-room process. The TTPF Viewer MIT notice remains included. No GPL XCL,
PS09 research code, Xbox tile tables or extracted resources are distributed.
