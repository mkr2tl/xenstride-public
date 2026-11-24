# XCL pipeline scheduling contribution

GPL-2.0-only. Derived from [ivall/xcl](https://github.com/ivall/xcl).
Xenstride's local code was developed with Claude Code assistance; see ../PROVENANCE.md.

`before/` contains the full module at local base `12ae36a49f587c62f101bfd6ce167f7fb739de08`. Apply `31514c5.patch`, then
`fffd4e9.patch` to a copy of that tree. The resulting module must equal `after/` byte for
byte (use `git -c core.autocrlf=false apply` to keep LF on Windows). Patch headers contain source paths, not private author email addresses.

Changes: raise a build-ahead worker to the waiting bind thread's priority; distinguish
pipelines sharing shaders but using different graphics state; read completed/result state
under one lock. Tests are included in the module. Nothing is claimed about fixing all
GPU stalls or saturated VRAM. Unknown root-signature differences remain a review concern
because pipeline keys omit root-signature pointers; the complete runtime context matters.

This is a source review package, not a complete buildable fork. The exact local parent is
not asserted to be a publicly fetchable commit. Full runtime reproduction requires a
separately reviewed source release, Cargo.lock, build scripts and notices. Do not point
reviewers at a nonexistent public commit or pretend these files alone produce xcl.exe.
No XCL binaries are distributed here. Selected source changes are dated 2026-11-24.

## Exact upstream origin and modification notices

The upstream basis is ivall/xcl commit `9aec53df7004bd9018ef3df67532f85d66ee261c`,
module blob `b303cf55e43b42c001e2c72b82cac363380be269` (checked against GitHub).
The local base is not pristine upstream: `local-base-reconstruction.patch` records
all module changes between that upstream commit and the local base. This bridge
is provenance material, separate from the two selected feature patches.

Both modified snapshots carry a prominent GPL notice dated 2026-11-24. Those
three comment lines were added for distribution; the bodies still match their
recorded local Git blobs. The two feature patch files are unchanged. Starting
from the public upstream module, apply the base reconstruction, prepend the
three-line notice from before/, then apply the two feature patches in order.
The result equals after/. Use core.autocrlf=false. This does not reconstruct
the full private runtime or assert that it builds in a pristine upstream checkout.
See PROVENANCE.json for full commits and hashes. The upstream module itself has
substantial earlier XCL authorship; neither the bridge nor the snapshots are a
claim that Xenstride wrote the entire module.
