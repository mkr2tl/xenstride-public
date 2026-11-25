# Changelog

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
See [STATUS.md](docs/STATUS.md).

## Runtime v0.1.0

- First buildable Xenstride XCL runtime source export and experimental Windows
  binary. See [runtime provenance](runtime/XENSTRIDE_CHANGES.md) for the P14 base.
