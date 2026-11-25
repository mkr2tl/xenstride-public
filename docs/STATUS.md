# Runtime v0.2.0 status

GTA VI gameplay has not been reached. This experimental release is a selected set
of changes on top of the public v0.1.0 source. The P29 laboratory executable is excluded.

## Evidence

| Area | Observed result | Scope |
| --- | --- | --- |
| Compute PSO cache | P19 synthetic AMD runs accepted driver blobs produced by the offline path on native and split compute. Readbacks matched cold runs. | No production compilation time or gameplay gain established. Cache blobs depend on a compatible driver and matching pipeline inputs. |
| Shader scratch | In a bounded RX 9060 XT fixture, 128 proven dwords per lane matched a 1,024-dword reference. Peak process commit was 47?51 MiB with the bound and 303?304 MiB at full size. | A workload-specific result. Unproven addresses retain the full array. |
| Split resource barriers | The regression covers BEGIN_ONLY, END_ONLY and complete transitions. | State tracking; no title performance claim. |
| Pipeline counters | Build-ahead jobs count toward started, running and build-work time. | Diagnostic correction. |

## Release candidate checks

- Windows x64, Rust 1.100.0/MSVC: locked release workspace build completed.
- WARP library suite: 773 passed, 0 failed, 26 ignored. GPU capability
  skips are not coverage. The suite runs serially because some pipeline tests
  share process-wide state.
- RX 9060 XT: the packaged executable generated a compute driver blob. The
  integration test loaded it in a distinct runtime process, checked the GPU
  readback, and rebuilt a deliberately invalid blob successfully.
- MIT tools: 84 passed, 1 skipped on each of Python 3.12 and 3.14 on Windows.

## Why the P29 binary is excluded

Two development captures verified by executable hash reported peak private
process commit of 32.5 GiB for P28 and 46.8 GiB for P29 (+44%). Available system
RAM fell to 489 MiB in the P29 capture. The captures did not control all cache
and background conditions and do not establish causality. Both stopped
progressing after the same last logged presentation milestone. A separate
synthetic test showed a shorter initial bind under injected compiler contention,
but does not resolve the memory finding.

The public candidate omits that scheduling change, the lab BCPack port, ASD,
private captures, and all compiled shader and driver caches. Graphics PSO cache
changes from the laboratory build are also outside this release. Only the
changes listed in the changelog are included. The earlier public runtime's
limitations remain relevant to this experimental build.

## Reproduce

See [runtime build and tests](RUNTIME.md) and [tester notes](TESTERS.md). Use
only authorized workloads. A passing build or synthetic test does not establish
compatibility with commercial titles. No commercial workload was run for this
release review.
