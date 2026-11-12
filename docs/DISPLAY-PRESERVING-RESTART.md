# Experimental display-preserving KernelSU-Next restart

Status: candidate for SM-F966B F966BXXSDCZJ1. Ten host tests pass. Android builds
and phone validation are separate requirements; neither host tests nor a successful
build establish that the cover display or Vector works on the phone.

## What changes

In this experimental build, plain `ksud soft-reboot` selects the display-preserving
path. The existing app call therefore needs no changes or app fork. Explicit
`ksud soft-reboot --full-restart` selects the original full service restart.
The display-preserving path runs the emulated-soft-reboot,
post-fs-data, services and boot-completed stages, stops only the running Zygotes,
and starts them again without a bare Android `stop` or `start`. It does not explicitly
restart SurfaceFlinger, composer, audioserver or netd. Device init dependencies and
module scripts may still restart services and must be checked on the phone.

Before the deliberate replacement system_server starts, each present modern/legacy
Rescue Party boot counter is reduced by one, flooring at zero. It does not clear
the entire failure history, change mitigation levels, or disable Rescue Party.
The counter adjustment is a candidate response to the supplied logs, not a claim
that Samsung's complete recovery logic has been established.

Preflight refuses incomplete boot or missing Zygote/SurfaceFlinger, malformed boot
counters are rejected before teardown, Zygote shutdown has a bounded wait, and
failure recovery attempts each affected service. Existing unbounded boot-completion
waiting is retained. Arbitrary module lifecycle side effects cannot be rolled back.

## Build for dfroot

The dfroot path uses a KMI-generic Samsung-patched daemon/module. It does not
need an exploit payload tailored to an exact firmware build. The supplied phone
uses KMI `android15-6.6`; this is a kernel branch identifier, not the Android OS
version (the supplied logs show Android 17).

The **Build and publish display-preserving payload** workflow validates pull
requests without publishing. Pushes to `fix/fold7-preserve-display-restart` build
and publish the candidate into that same branch. Manual dispatch offers a
`publish` checkbox, enabled by default, and publishes into the selected branch. It first applies the existing Samsung patch and this candidate
to the pinned v3.4.2 source and runs the host harness. It then calls the existing
KernelSU workflow with:

- flavor: `kernelsu-next`
- ksu_ref: `v3.4.2`
- generic_kmis: `["android15-6.6"]`
- preserve_display: `true`
- publish: enabled for candidate-branch pushes and manual dispatch; disabled for PR validation

This rebuilds the matching generic kernel modules and embeds the no-patch-text
module into the candidate generic daemon. The relevant artifact is
`generic-ksud-kernelsu-next-preserve-display`. The separate `ksud-...-aarch64-preserve-display`
artifact is a compile check without an embedded KMI module and must not be confused
with the generic output. When publishing is enabled, the generic KernelSU-Next feed entry and its binary
are committed to the run branch in this fork. The feed records the built KMI set,
size, digest and version. Device-specific feed entries are unchanged.

The optional patch is limited to the pinned KernelSU-Next v3.4.2 source.
Normal builds default to `preserve_display: false`. The restart patch lives outside
the Samsung patch collection so its userspace-only contents do not enter the RKP
compatibility checks or automatic Samsung rebase process.

The existing app command selects this path once it uses this candidate daemon;
no new app argument is required. The app still needs to obtain the candidate
binary through its supported payload selection mechanism. Add this fork and the published branch to Payload Sources and select the
KernelSU-Next generic tier. Other enabled sources may take precedence, so disable
other sources for this test. Verify the selected source and build in the run log.

## Validation

Host tests compile the actual patched restart source with mocked Android service
and property adapters. They cover orchestration, absent/invalid counters, retaining
nonzero failure history, and injected mount/property/start failures. They do not
validate Samsung HAL behavior, actual property permissions, or module injection.

    KSUD_RESTART_SOURCE=/absolute/path/to/KernelSU/userspace/ksud/src/soft_reboot.rs \
      cargo test --locked --manifest-path tests/preserve-display/Cargo.toml -- --test-threads=1

After an Android build and review, acceptance on the phone requires a completed
boot with ports 147 and 148 present, unchanged kernel boot ID and SF/composer PIDs
across one planned restart, both screens working through fold/unfold, functioning
Vector injection, and no Rescue Party escalation. Losing a port, restarting SF or
composer, injection failure, an accumulating planned boot count, or a full reset
fails the test. The earlier composer/quick-reload failures must not be considered
recovered just because this source patch compiles.

## Evidence

The reported ordinary soft reboot removes port 148 from SurfaceFlinger while DRM
DSI-2 remains connected and the vendor composer retains its secondary display.
Quick reload requested Zygote restart at 19:34:57; system_server started at 19:34:59
and selected Rescue Party WARM_REBOOT at 19:35:04. Boot counters changed from 1
to 2. The supplied bundle also contains an earlier vectord stack-overflow crash
and a StartingSurfaceController hook failure; this patch does not claim to fix them.

- KernelSU-Next v3.4.2: https://github.com/KernelSU-Next/KernelSU-Next/tree/4c706b5938665866ebe16944a481a7b91e3396af
- Android stop/start service list: https://android.googlesource.com/platform/system/core/+/refs/heads/main/toolbox/start.cpp
- CrashRecovery boot counters: https://android.googlesource.com/platform/system/libsysprop/+/c93b595d47ed6a9e72e1e3eeeb216f36243244fb/srcs/android/sysprop/CrashRecoveryProperties.sysprop
- CrashRecovery startup: https://android.googlesource.com/platform/frameworks/base/+/a490443936a25ef95af50511a092f35c0a42b70c/services/core/java/com/android/server/crashrecovery/CrashRecoveryModule.java
