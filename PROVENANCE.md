# Provenance

Prepared 2026-11-24 from immutable local commits. No private Git history is bundled.

| Material | Origin | Licence / decision |
| --- | --- | --- |
| tools-mit | Xenstride tools commit `e7e4024c837657ec5c52bb053e7a2846f44d9fbc` | MIT; existing provenance retained |
| xcl-gpl/before | XCL-derived module at `12ae36a49f587c62f101bfd6ce167f7fb739de08` | GPL-2.0-only |
| priority patch | local `31514c52cea1eab5c7744320fcf40cf09fb43042` | GPL-2.0-only; worker priority change and tests |
| state-key patch / after snapshot | local `fffd4e9c67e85693732df52a55894005677e4ee9` | GPL-2.0-only; state-sensitive prebuild keys and bind-state fix |
| project documentation | written for this preparation | MIT |

XCL provides the compatibility architecture and substantial existing module logic.
The source snapshots deliberately retain upstream context; they are not wholly original
Xenstride code. The two patch commits were recorded locally under MKR-2TL with Claude
co-author attribution. Historical Codex work covered memory, diagnostics and synthetic
GPU fixtures; this preparation does not claim all such work remains active in P13.
P10's experimental buffer mirrors were removed from the stable P11 line.

The publishable tree does not contain personal commit email addresses, old Git objects,
raw execution logs, media, game binaries, extracted shaders, dumps, keys, SDKs or caches.
P14 and its uncommitted changes are excluded. No author identity or development history
has been manufactured. A future public commit records preparation, not the date of the
original development. Copyright and licence obligations remain with each component.

These are locally developed deltas, not claims of first invention. The P13 report
notes overlapping upstream work, including XCL PR #1007 on priority inversion.
A change already present upstream should be credited as integration when published.

The exact upstream module revision, local-base reconstruction and dated distribution
notices are specified in xcl-gpl/PROVENANCE.json and xcl-gpl/README.md. The snapshot
source bodies are unchanged; only their explicit licence/date comments were added.
