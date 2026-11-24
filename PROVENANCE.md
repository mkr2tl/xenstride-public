# Provenance

Prepared 2026-11-24 from immutable local commits. No private Git history is bundled.

| Material | Origin | Licence / decision |
| --- | --- | --- |
| runtime | public ivall/xcl `9aec53df7004bd9018ef3df67532f85d66ee261c` + local P14 commit `fe6a01375f6a5542d77674f7e965960898acc32f` (53 local commits) | GPL-2.0-only; every changed file carries a dated notice; see runtime/XENSTRIDE_CHANGES.md |
| tools-mit | Xenstride tools commit `e7e4024c837657ec5c52bb053e7a2846f44d9fbc` | MIT; existing provenance retained |
| xcl-gpl/before | XCL-derived module at `12ae36a49f587c62f101bfd6ce167f7fb739de08` | GPL-2.0-only |
| priority patch | local `31514c52cea1eab5c7744320fcf40cf09fb43042` | GPL-2.0-only; worker priority change and tests |
| state-key patch / after snapshot | local `fffd4e9c67e85693732df52a55894005677e4ee9` | GPL-2.0-only; state-sensitive prebuild keys and bind-state fix |
| project documentation | written for this preparation | MIT |

XCL provides the compatibility architecture and most of the runtime's code. The runtime keeps
upstream's own files (README, HANDOFF, CONTRIBUTING, AGENTS, CLAUDE, docs) unchanged as
upstream context; they describe XCL, not Xenstride. The local commits were recorded under
MKR-2TL, a lab identity and one upstream contributor's cherry-picked change (see
runtime/XENSTRIDE_CHANGES.md); work was assisted by Claude Code and OpenAI Codex.

The runtime export (`runtime/XENSTRIDE_EXPORT.txt`) applies only these changes to the local
commit: comments naming the private development lab read `Xenstride`; two byte-string literals
use `\0` escapes instead of raw NUL bytes; one 8-byte binary test fixture is written by its test
harness instead of being stored; a dated notice comment heads each changed file. The exported
tree was built (`cargo build --workspace --release --locked --offline`) and tested on WARP
before the notice comments were added; the release binary is built from the committed tree.

The publishable tree does not contain personal commit email addresses, old Git objects,
raw execution logs, media, game binaries, extracted shaders, dumps, keys, SDKs or caches.
Reviewed false positives of the privacy patterns are listed, bound to exact file contents,
in scripts/publication_allowlist.json. No author identity or development history has been
manufactured. A future public commit records preparation, not the date of the original
development. Copyright and licence obligations remain with each component.

These are locally developed deltas, not claims of first invention. Overlapping upstream work
is credited: the compute build-ahead is a port of XCL PR #889, the CopyFromSubresource fix a
port of XCL PR #684, and XCL PR #1007 addresses the same priority inversion.

The exact upstream module revision, local-base reconstruction and dated distribution
notices of the earlier module package are specified in xcl-gpl/PROVENANCE.json and
xcl-gpl/README.md.
