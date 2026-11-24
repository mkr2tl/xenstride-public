# Selected development milestones

These are local engineering phases, not fabricated public release dates.

- v0.1 / initial `16765e7d`: standalone format tools. This new repository exports selected source without inheriting the original Git history.
- Later tools work, through `e7e4024`: owned PE entry execution and restricted import resolution.
- P9: local input instrumentation and lab profile. Historical menu observations are not gameplay.
- P10 / P10.1: experimental non-UMA mirrors and measured regressions. Removed from stable P11.
- P11 / P11.1: updated XCL integration, memory/input checks and independent WARP validation.
- P12: diagnostic attribution of long pipeline waits; much of the runtime remains upstream XCL.
- P12.1: worker-priority correction and integration-test WARP guards.
- P13 (`00bc53e`): state-sensitive prebuild keys, consistent bind status, activity counts and a synthetic pacing fixture.
- P14 (`fe6a013`): compute pipelines built ahead within the free commit (port of XCL #889), a
  build-ahead starvation fix, pacing metrics. First public runtime source: `runtime/`, version 0.1.0.

Results and limitations are listed in STATUS.md.
