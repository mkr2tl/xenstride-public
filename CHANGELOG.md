# Changelog

## Runtime v0.3.0 Experimental

First experimental release of the VIctory-based Xenstride runtime. The
older XCL releases retain their own source, notices and build instructions.

- Pack partial texture uploads and reclaim cold staging buffers. Use
  smaller initial staging blocks on light streaming workloads.
- Pack partial color readbacks and share small readback allocations.
  Read back compatible whole depth and stencil planes.
- Derive root-signature heap flags from shader requirements and restore
  implicit typed color and sRGB array views.
- Preserve depth bounds via [VIctory PR #158](https://github.com/Marvisak/VIctory/pull/158)
  by r4tedx. Xenstride adds regression coverage and an explicit unsupported
  path for host adapters without depth bounds.
- Deduplicate upload preparation and avoid repeated bindless buffer scans
  within a submission. Honor directed FP16 rounding in translated shaders.
- Add the procedural crowd benchmark and persistent capture profiles.
  The independent collector finalizes captures at timeout or interruption.

See [status and scope](docs/STATUS.md) and the release benchmark evidence.

## Runtime v0.2.0 (experimental)

- Align offline and runtime compute PSO cache inputs, including memory windows,
  and write generated blobs atomically. Credits: XCL #744 and #1469.
- Track split D3D12 barriers as completed when the matching END_ONLY is recorded.
  Credit: Alp Pesen, XCL #1654.
- Bound translated shader scratch when the reachable addresses can be proven.
  Fix the GFX10.3 immediate-only ST scratch addressing mode.
- Include speculative PSO build work in compilation diagnostics.

GTA VI gameplay has not been reached. The P29 laboratory executable and its
demand scheduling change are excluded after review of the observed memory rise.
See the [v0.2.0 status](https://github.com/mkr2tl/xenstride-public/blob/runtime-v0.2.0/docs/STATUS.md).

## Runtime v0.1.0

- First buildable Xenstride XCL runtime source export and experimental Windows
  binary. See [runtime provenance](runtime/XENSTRIDE_CHANGES.md) for the P14 base.
