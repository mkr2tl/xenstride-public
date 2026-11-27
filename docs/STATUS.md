# Runtime v0.3.0 Experimental

Current release: [runtime-v0.4.0](https://github.com/mkr2tl/xenstride-public/releases/tag/runtime-v0.4.0). See the [V21 release notes](releases/runtime-v0.4.0.md) for public-rebuild validation and separately attributed historical observations.

| Area | Verified result |
| --- | --- |
| Streaming | In the original 960 × 540 crowd scene on RX 9060 XT, peak staging fell from 32 MiB to 8 MiB with FP16 enabled. Median frame time was 5.786 ms with fixed blocks and 5.815 ms with smaller initial blocks. P95 was 7.014 ms and 6.901 ms respectively. |
| Fidelity | Across 32 measured runs in four configurations, all 1,152 frames matched the reference sequence by frame index. Hardware counters agreed. The scene uses native PC HLSL. |
| Shader translation | A separate RDNA2-to-DXIL test checked 10 directed FP16 rounding inputs against exact output bits on Radeon and WARP. |
| Streaming soak | The same runtime configuration completed 6,000 synthetic frames in 51.3 seconds. |
| Profiles and capture | The release executable reopened the fixture's V8 profile, updated saved data, and finalized video, audio, metrics and capture hashes. All tracked processes exited. |

The release attaches `BENCHMARKS.md` through the evidence archive, with the
factorial harness, frame records and measurement patch. Timing results apply
to that off-screen workload. Commercial titles, gameplay, translated-shader
performance, and broader driver and hardware coverage remain unverified.
BCPack streams require separately obtained, authorized tables.

[Build instructions](RUNTIME.md) · [Historical XCL v0.2.0 status](https://github.com/mkr2tl/xenstride-public/blob/runtime-v0.2.0/docs/STATUS.md)
