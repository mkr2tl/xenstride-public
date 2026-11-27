# Xenstride runtime 0.4.0 ? V21 Fast CPU

This experimental candidate moves Xenstride from its earlier XCL runtime line to fxceless/VIctory and adds targeted AMD compiler, pipeline preparation and GPU diagnosis work.

## Runtime changes



V21 brings Xenstride's work onto the fxceless/VIctory runtime. Ten Xenstride commits follow upstream base `9a9091d`; the full commit record is included in the provenance report.

- **AMD compiler compatibility:** shader optimisation now reuses a view's size within its own block and keeps separate queries separate. This addresses the invalid cross-query reuse found during AMD shader validation (`0db0334`, `b27f7d6`).
- **Compute prewarming:** the new `prewarm` command builds compute pipelines before execution, using the same configuration as the run. Existing shader caches and the pipeline library remain reusable (`1434381`). Shader-cache implementation bytes were deliberately preserved because they participate in translation keys (`766e5bc`).
- **Adaptive memory defaults:** copy budgets grow with the GPU's reported memory budget, with presentation timing added for diagnosis (`ddee3f6`).
- **GPU stall monitoring:** queue progress is watched, a stalled host queue is reported durably, and the runtime ends the run rather than waiting indefinitely (`50003e8`). This improves failure diagnosis; it does not prevent every driver hang.
- **Fast CPU configuration:** eight upstream CPU/submission settings were exercised together in the internal interactive session. Async submission and copyback mechanisms are inherited work; Xenstride's contribution here is the selected configuration and its validation.
- **Synthetic GPU coverage:** reserved-texture mapping, in-place memory probes, cached-DXIL validation and a reusable multi-thread submission lab support investigation without bundling game fixtures (`4c5c9df`, `53143ac`, `df3dc33`, `9627946`).

Fast CPU selects `ASYNC_SUBMIT=1`, `JOIN_PARTS=1`, `HEAP_WORKER=1`, `BACK_OWN_VIEW=1`, `COPY_PLAIN=1`, `MERGE_GAP=64`, `MERGE_RUNS=4096`, and `TEXTURE_NO_WAIT=1`, each prefixed with `VICTORY_`.


## Validation and observed progress

The supplied runtime SHA-256 is `8e3e6687b009f5592f13ec158796d97b153004758d9a0f2a7a282a7d1c7f27f9`, matching the internal capture report. The historical V21 test log totals 687 passed, zero failed and 42 ignored. The public candidate workspace suite passes with 691 passed, zero failed and 44 ignored. Two table-dependent tests also pass separately with private input.

The internal session used a Ryzen 7 5800X3D, RX 9060 XT 16 GB and 32 GB RAM. Its report records RT disabled, normal runtime exit, and no D3D12 device loss. The supplied summary reports 5.78 Present calls/s, rising to 6.33 in its final interval, versus 4.87 for V20 Default. Scenes and caches differ, so this is progress evidence rather than a controlled benchmark. Present calls are not verified unique rendered world frames.

## Compatibility and known limitations

Only the stated AMD configuration is covered by the internal session. Visual defects persist. The eight-minute session does not establish long-session stability. RT, other GPUs and other operating systems are not validated for this candidate. Earlier XCL releases retain their own licence and validation scope.

## Public packaging

The release candidate ships without BCPack tables. Configure `VICTORY_BCPACK_TABLES` with an authorised private file when the workload needs them. This matches the separation already used in Xenstride v0.3.0. The runtime was rebuilt from the accompanying source; the internal capture hash above identifies the measured V21 binary, not this public rebuild. See BUILD-PROVENANCE.json for the candidate hash.

The source policy now applies a bounded 640 KiB limit to host.rs and lower.rs; binary signatures, text checks and encoded-payload checks remain active. Other source files retain the 512 KiB limit. No licence text was changed.

Assets include the Windows executable, corresponding source, locked dependency sources, original licence notices and SHA256SUMS.txt. Game files, private tables, captured shaders, caches, saves and audiovisual captures are excluded. This is an experimental prerelease; the public rebuild has synthetic validation, while the short interactive performance observations belong to the original internal V21 build.
