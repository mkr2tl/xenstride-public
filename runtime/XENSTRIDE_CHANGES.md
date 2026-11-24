# Xenstride changes to XCL

This runtime is [XCL](https://github.com/ivall/xcl) (GPL-2.0-only) at upstream commit `9aec53df7004bd9018ef3df67532f85d66ee261c`
with the Xenstride changes below, exported from local commit `fe6a01375f6a5542d77674f7e965960898acc32f`. Every file changed or
added relative to that upstream commit starts with a dated notice pointing here (GPL-2.0 section 2a).
Development used Claude Code and OpenAI Codex; results were checked by tests on WARP.

## Commits (oldest first)

| date | author | change |
| --- | --- | --- |
| 2026-11-23 | Xenstride (lab identity) | b03f72b launch-prep: hardened loader (reloc/TLS/VirtualProtect checks, XCL_STRICT_IMPORTS) |
| 2026-11-22 | Xenstride (lab identity) | verify: replay with hardened XCL, loader E2E tests, large-image reader, resume check |
| 2026-11-23 | Xenstride (lab identity) | xcl-hardened: Scarlett imports on upstream 956a476 (migration of 54ecce5) |
| 2026-11-23 | MKR-2TL | kernelx: size/overflow invariants and sparse bounds on upstream 956a476 (migration of 51feb0d) |
| 2026-11-23 | MKR-2TL | test(runtime): force WARP for unit fixture GPU initialization |
| 2026-11-23 | MKR-2TL | kernelx: refuse unsupported tools-memory transfers instead of fake success |
| 2026-11-23 | MKR-2TL | diagnostics: keep crash callbacks opt-in and retain local interface metadata |
| 2026-11-23 | MKR-2TL | hle: COM refusals reach interface_diagnostics on upstream 956a476 (migration of c6047a8) |
| 2026-11-23 | MKR-2TL | diagnostics: GDK method failures and exception observations on upstream 956a476 (migration of 919ad9b) |
| 2026-11-23 | MKR-2TL | xuser: independent change-event registrations on upstream 956a476 (migration of 684337a) |
| 2026-11-23 | MKR-2TL | dstorage: refused IIDs reach interface_diagnostics; P0 X1 regression tests (migration of 8613130) |
| 2026-11-23 | MKR-2TL | net: testable XCL_NETWORK=0 routing and ordinal regression test (migration of 503be96) |
| 2026-11-23 | MKR-2TL | d3d12x: report sparse windows that the adapter refused (GPU VA 0 entries) |
| 2026-11-23 | MKR-2TL | test(d3d12): select WARP explicitly in memory and command fixtures |
| 2026-11-23 | MKR-2TL | fix(diagnostics): aggregate repeated interface refusals before stack capture |
| 2026-11-23 | MKR-2TL | fix(pso): finish rejected background submissions and measure scheduling work |
| 2026-11-23 | MKR-2TL | fix(d3d12): validate PSO streams and preserve rasterizer controls |
| 2026-11-23 | MKR-2TL | fix(memory): fall back to coherent GPU backing after sparse initialization failure |
| 2026-11-23 | MKR-2TL | test(depth): exhaust logical D16S8 storage and verify WARP planes views and copies |
| 2026-11-23 | MKR-2TL | fix(kernelx): a graphics MEM_COMMIT never succeeds as a bare reservation |
| 2026-11-23 | MKR-2TL | fix(diagnostics): name the real cause of dropped timestamps; report non-UMA buffer views |
| 2026-11-23 | MKR-2TL | test(kernelx): scatter NULL-unmap test on upstream 38b05b7's lazy physical pages |
| 2026-11-23 | MKR-2TL | test(d3d12): upstream D16S8-as-D24S8 mapping keeps every depth and stencil value on WARP |
| 2026-11-23 | MKR-2TL | test(d3d12): new upstream GPU tests use explicit WARP or an explicit hardware opt-in |
| 2026-11-23 | MKR-2TL | diagnostics(trace): what each traced draw's descriptor tables and root buffers hold |
| 2026-11-23 | MKR-2TL | uma: test and report how the coherent window is sized from the last run's need |
| 2026-11-23 | MKR-2TL | fix(present): the busiest-target dump no longer reads released resources; panics reach xcl.log |
| 2026-11-23 | Remington Wilcox | Physical pages: homes released under AWAY, a freed page's home given back at once |
| 2026-11-23 | MKR-2TL | diagnostics(trace): whether the buffers a traced draw reads hold data |
| 2026-11-23 | MKR-2TL | test(d3d12): the native-118 check calls the driver's own method, whatever adopted the class |
| 2026-11-23 | MKR-2TL | input: report what reached the title and why a press may not have; lab build profile |
| 2026-11-24 | MKR-2TL | vfs: a GetDiskFreeSpaceEx caller that passes no 'available' pointer is not logged as a full disk |
| 2026-11-24 | MKR-2TL | test(d3d12): drop the P5 timestamp-refusal test – upstream 4403887 now writes timestamps outside unified memory |
| 2026-11-24 | MKR-2TL | test(d3d12): upstream beaa245's new GPU tests take WARP unless XCL_TEST_HARDWARE_GPU=1 |
| 2026-11-24 | MKR-2TL | test(pso): our P3/P6 fixtures fill upstream's new Pending key / used fields |
| 2026-11-24 | MKR-2TL | uma: a launch short of 17 GiB of free commit gets the coherent window that commit allows, not 512 MB |
| 2026-11-24 | MKR-2TL | xg: CopyIntoSubresource never writes outside its slice – E_INVALIDARG instead of aborting the process |
| 2026-11-24 | MKR-2TL | d3d12x null: a fence DirectStorage will signal is not reported complete before it does |
| 2026-11-24 | MKR-2TL | input: every press and release the title reads is logged (first 400) and counted; a check-box menu fixture over the GameInput reads |
| 2026-11-24 | MKR-2TL | test(acphal): xapu decodes a title's stereo CELT Opus packets into its buffer (the retail sound path, upstream 2b4bb43) |
| 2026-11-24 | MKR-2TL | test: texture_states and heap_probe take WARP unless XCL_TEST_HARDWARE_GPU=1 |
| 2026-11-24 | MKR-2TL | fix(xg): CopyFromSubresource takes 7 arguments and checks the destination span (upstream PR #684) |
| 2026-11-24 | MKR-2TL | test: every test device is made on WARP unless XCL_TEST_HARDWARE_GPU=1 |
| 2026-11-24 | MKR-2TL | diag(p12): name the pipeline waits a stall comes from; clearer blob, BVH and DirectStorage lines |
| 2026-11-24 | MKR-2TL | fix(pso_async): a build-ahead thread a bind waits for runs at the waiter's priority |
| 2026-11-24 | MKR-2TL | test: every integration test binary runs on WARP (XCL_ADAPTER=warp before main) |
| 2026-11-24 | MKR-2TL | fix(pso_async): build ahead only the pipelines an earlier run used; a bind never takes a stand-in for a built pipeline |
| 2026-11-24 | MKR-2TL | diag(activity): the memory line counts the pipelines built and those built ahead |
| 2026-11-24 | MKR-2TL | test(testtitle): --fixture=frame-pacing:<µs>:<frames>[:<busy>[:<gpu 0/1>]] |
| 2026-11-24 | MKR-2TL | fix(pso_async): a bind never waits for a build-ahead thread it cannot raise |
| 2026-11-24 | MKR-2TL | perf(pso_async): compute pipelines are built ahead of their first bind, within the commit left (port of ivall/xcl#889) |
| 2026-11-24 | MKR-2TL | test(testtitle): frame-pacing prints average FPS and the 1 % low |
| 2026-11-24 | MKR-2TL | fix(pso_async): build-ahead checks the commit left when it takes a job |

## Files

| file | | dates |
| --- | --- | --- |
| `Cargo.toml` | changed | 2026-11-23 |
| `crates/xcl/src/diagnostics.rs` | added | 2026-11-23 |
| `crates/xcl/src/diagnostics_repeat.rs` | added | 2026-11-23 |
| `crates/xcl/src/eh.rs` | changed | 2026-11-23 |
| `crates/xcl/src/gpu/agility.rs` | changed | 2026-11-24 |
| `crates/xcl/src/gpu/recomp/runner.rs` | changed | 2026-11-24 |
| `crates/xcl/src/gpu/tiling.rs` | changed | 2026-11-24 |
| `crates/xcl/src/gpu/uma.rs` | changed | 2026-11-23, 2026-11-24 |
| `crates/xcl/src/hle/acphal.rs` | changed | 2026-11-24 |
| `crates/xcl/src/hle/d3d12x/bind.rs` | changed | 2026-11-23 |
| `crates/xcl/src/hle/d3d12x/console_heap.rs` | changed | 2026-11-23 |
| `crates/xcl/src/hle/d3d12x/cpu_writes.rs` | changed | 2026-11-23, 2026-11-24 |
| `crates/xcl/src/hle/d3d12x/depth118_tests.rs` | added | 2026-11-23 |
| `crates/xcl/src/hle/d3d12x/fence_x.rs` | changed | 2026-11-23 |
| `crates/xcl/src/hle/d3d12x/fixture_gpu.rs` | added | 2026-11-23 |
| `crates/xcl/src/hle/d3d12x/list_x.rs` | changed | 2026-11-23, 2026-11-24 |
| `crates/xcl/src/hle/d3d12x/mod.rs` | changed | 2026-11-23 |
| `crates/xcl/src/hle/d3d12x/null.rs` | changed | 2026-11-23, 2026-11-24 |
| `crates/xcl/src/hle/d3d12x/present.rs` | changed | 2026-11-23 |
| `crates/xcl/src/hle/d3d12x/pso_async.rs` | changed | 2026-11-23, 2026-11-24 |
| `crates/xcl/src/hle/d3d12x/pso_diagnostics.rs` | added | 2026-11-23 |
| `crates/xcl/src/hle/d3d12x/real.rs` | changed | 2026-11-23, 2026-11-24 |
| `crates/xcl/src/hle/d3d12x/sparse.rs` | changed | 2026-11-23, 2026-11-24 |
| `crates/xcl/src/hle/d3d12x/title_textures.rs` | changed | 2026-11-24 |
| `crates/xcl/src/hle/d3d12x/vram.rs` | changed | 2026-11-23 |
| `crates/xcl/src/hle/dstorage.rs` | changed | 2026-11-23, 2026-11-24 |
| `crates/xcl/src/hle/dstorage_p0_tests.rs` | added | 2026-11-23 |
| `crates/xcl/src/hle/gameinput.rs` | changed | 2026-11-23, 2026-11-24 |
| `crates/xcl/src/hle/kernelx.rs` | changed | 2026-11-23 |
| `crates/xcl/src/hle/mod.rs` | changed | 2026-11-23 |
| `crates/xcl/src/hle/opus.rs` | changed | 2026-11-24 |
| `crates/xcl/src/hle/xg.rs` | changed | 2026-11-24 |
| `crates/xcl/src/hle/xgameruntime/api.rs` | changed | 2026-11-23 |
| `crates/xcl/src/hle/xgameruntime/mod.rs` | changed | 2026-11-23 |
| `crates/xcl/src/host.rs` | changed | 2026-11-24 |
| `crates/xcl/src/lib.rs` | changed | 2026-11-23 |
| `crates/xcl/src/loader.rs` | changed | 2026-11-22, 2026-11-23 |
| `crates/xcl/src/report.rs` | changed | 2026-11-23 |
| `crates/xcl/src/resolver.rs` | changed | 2026-11-23 |
| `crates/xcl/src/state.rs` | changed | 2026-11-23 |
| `crates/xcl/src/vfs_hooks.rs` | changed | 2026-11-24 |
| `crates/xcl/tests/console_dlls_e2e.rs` | changed | 2026-11-24 |
| `crates/xcl/tests/console_shaders.rs` | changed | 2026-11-24 |
| `crates/xcl/tests/disasm_e2e.rs` | changed | 2026-11-24 |
| `crates/xcl/tests/e2e.rs` | changed | 2026-11-24 |
| `crates/xcl/tests/gpu_compute_e2e.rs` | changed | 2026-11-24 |
| `crates/xcl/tests/gpu_pso_e2e.rs` | changed | 2026-11-24 |
| `crates/xcl/tests/heap_probe.rs` | changed | 2026-11-24 |
| `crates/xcl/tests/pls_quota.rs` | changed | 2026-11-24 |
| `crates/xcl/tests/predication_e2e.rs` | changed | 2026-11-24 |
| `crates/xcl/tests/rpf_e2e.rs` | changed | 2026-11-24 |
| `crates/xcl/tests/sparse_reinit.rs` | changed | 2026-11-24 |
| `crates/xcl/tests/texture_states.rs` | changed | 2026-11-24 |
| `crates/xcl/tests/warp_guard/mod.rs` | added | 2026-11-24 |
| `crates/xcl/tests/xg_oracle.rs` | changed | 2026-11-24 |
| `tests/testtitle/src/gpu.rs` | changed | 2026-11-24 |
| `tests/testtitle/src/main.rs` | changed | 2026-11-23, 2026-11-24 |

## Export transformations (public tree only)

- Comments naming the private development lab read `Xenstride`; no code changed.
- Two byte-string literals hold `\0` escapes instead of raw NUL bytes (same compiled bytes).
- `tests/data/testgame/data/sub/nested.bin` (bytes 0..7) is written by `crates/xcl/tests/e2e.rs`
  instead of being stored, so the public tree holds text files only.
- The notice comments above were added to each changed file.
