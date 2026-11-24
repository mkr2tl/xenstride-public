# Status at the P14 boundary (runtime 0.1.0)

Runtime source: ivall/xcl `9aec53d` + local P14 commit `fe6a013`, exported to `runtime/`.
All results below are on WARP (Microsoft's software renderer) or synthetic fixtures unless
stated. No game material is part of any test.

| Area | Evidence | Scope / limit |
| --- | --- | --- |
| Runtime build | `cargo build --workspace --release --locked --offline` of the exported tree succeeds (Rust 1.100, MSVC) | Windows x64 only |
| Runtime library suite | 765 pass, 0 fail, 25 ignored (release, WARP) | Runtime skips of GPU-only paths are not GPU coverage |
| Integration suites | All pass except 2 end-to-end wrappers (3 checks) | WARP cannot create the 4 GiB sparse windows: coherent-window headroom, AS SRV descriptor and (intermittently) descriptor heaps in window 0 |
| Physical GPU use by tests | None: the test guards force WARP; a loaded-driver audit found only `d3d10warp.dll` | `XCL_TEST_HARDWARE_GPU=1` opts in |
| Compute build-ahead (port of XCL #889) | 24 compute pipelines of ~300 ms bound 4 s after creation, 4 busy threads: bind waits 6.0–7.1 s before, 0 ms after (3/3) | With every core busy: 6.1–14.3 s, longest bind 2.85 s (0.5 s without); `XCL_PSO_PRELOAD_COMPUTE=0` turns it off |
| Pipeline worker priority | Synthetic: 0.3 s of build work waited 40 s before, ~3 s after under full load; a starvation window (up to 50 s) closed | Artificial CPU contention; not a gameplay FPS result |
| State-aware prebuild keys | One shader set in 64 states yields 64 keys | Does not establish the cause of real GPU memory growth |
| Frame pacing (test title) | 30 / 60 FPS: average 30.00 / 60.00, 1 % low 29.3 / 59.0, p99 33.4 / 16.7 ms (idle or 4 busy threads) | WARP; with WARP rendering on a saturated CPU the 1 % low falls to ~4 FPS |
| Graphics regressions | 4 synthetic title cases × 3 runs: no check passing in P13 fails in P14; RDNA2 triangle pixels identical to the reference | Synthetic coverage only |

## Known limitations (unresolved)

- A newer retail title's world loading still ends in the title's own no-frame watchdog: compute
  pipelines first used during loading are compiled by the driver at their first bind (up to ~20 s).
  Nothing in this release skips GPU work to avoid it.
- High memory use while a world loads: in one 16 GB-GPU run, ~9.7 GB of render-target pages and
  ~29 GB of process commit. No leak was found; no reduction is implemented.
- BCPack-compressed textures are refused (upstream decoders are not integrated: their texel
  output cannot yet be validated against independent fixtures); mesh pipelines with a mesh
  shader in the amplification slot are refused; buffers outside the coherent window get empty
  views; several console-only D3D12 extensions are stubs.
- A graphics pipeline still being built draws nothing for those draws (stand-in).
- Physical-GPU behaviour of these changes has not been validated by an automated suite.

## Reproduction

Follow docs/RUNTIME.md. Library tests: `cargo test -p xcl --lib --release --locked`; integration
suites: `cargo build --workspace --release --locked` first, then `cargo test -p xcl --release
--locked --tests`. Synthetic benchmarks are `#[ignore]` tests run with `--ignored --nocapture`
(`p14_bench_compute_pipelines_made_while_loading_and_bound_later`, with `P14_BENCH_BUSY` and
`XCL_PSO_PRELOAD_COMPUTE`). No instructions for acquiring or running an unauthorised title are
provided. Public recordings require a separate rights/privacy review and owner approval.
