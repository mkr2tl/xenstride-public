# Status at the P13 boundary

P13 source boundary: local `00bc53eea2047d17c4a4f6f77d4f51f76d0c8d22`.
P14 is excluded. No GPU tests, title runs or heavy builds were performed for this publication.

| Area | Evidence available | Scope / limit |
| --- | --- | --- |
| Static tools and PE prototype | Existing local validation: 84 tests, 83 pass, 1 skipped | Reported prior run; not rerun here. Current GitHub main still describes static tools only |
| Pipeline worker priority | P12.1 synthetic CPU benchmark: waits 39896/40961/40795 ms before; 3038/3223/3370 ms after | Artificial CPU contention; not a gameplay FPS result |
| State-aware prebuild keys | P13 regression: one shader set in 64 states yields 64 keys | Does not establish the cause of real GPU memory growth |
| P13 runtime library suite | Existing report/log: 763 pass, 0 fail, 24 ignored | WARP; runtime skips are not passes for GPU coverage |
| Integration | Two persistent e2e wrapper failures | WARP sparse-window limitations; suite is not fully green |
| Synthetic cadence | P13 fixture supports 33333 and 16667 microsecond intervals | Clear/present and clock tests, not GTA VI gameplay |
| Historical title observations | Internal P11/P12.1 reports describe first-run screens, intro/loading and menu intervals around 30 fps | Self-reported, one machine; no approved public recording in this package |
| Audio | XCL upstream Opus path integrated and a synthetic packet test recorded in local history | No public, release-scoped claim of complete title audio |
| Gameplay / player control | No validated evidence in this release | Not demonstrated |

The P13 report's pacing table and one saved JSON differ in their numeric details. We do
not publish a single p99 figure until the exact run selection is resolved. Historical menu
measurements must not be extrapolated to world rendering. Long pipeline waits, incomplete
graphics paths, storage/format gaps and loading stalls remain. P13 has no verified post-fix
title run. No new claim is made that the old title crash has been fixed.

## Reproduction

For the MIT tools follow tools-mit/README.md and its validation notes. For source-patch
verification follow xcl-gpl/README.md. The original P13 WARP tests require the full pinned
local runtime source: `cargo test -p xcl --lib --profile lab --locked --offline`, with
`XCL_ADAPTER=warp` and no hardware-GPU opt-in. Do not treat these instructions as a complete
runtime build procedure from this limited source package. Synthetic evidence is separable
from any game material. No instructions for acquiring or running an unauthorised title
are provided. Public recordings require a separate rights/privacy review and owner approval.
