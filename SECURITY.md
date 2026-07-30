# Security policy

ClipAsm is pre-release software that processes local media through FFmpeg and
FFprobe. It can also execute explicitly declared external programs. Keep
security reports private until a fix or disclosure plan is ready.

## Supported versions

Security fixes target the current `main` branch and the latest published
release, when one exists. Older commits and releases may not receive fixes.

## Report a vulnerability

Do not include exploit details, private media, credentials, or other sensitive
information in a public issue.

1. If GitHub offers its
   [private vulnerability reporting form](https://github.com/NikitaMGrimm/clipasm/security/advisories/new),
   use it.
2. If GitHub does not offer the private form, open a public issue that contains
   only a private-contact request and a non-sensitive description.

Include enough information to reproduce and assess the report:

- the affected ClipAsm commit or release
- the operating system and relevant FFmpeg and FFprobe versions
- the smallest safe reproducer you can provide
- the expected security boundary and the observed behavior
- the likely impact and any known workarounds

There is currently no guaranteed response or remediation time. Please allow a
reasonable opportunity to investigate before public disclosure.

## Expected trust boundaries

External programs are trusted native code. Rendering a reachable external
program runs its declared executable with the current user's permissions.
ClipAsm does not sandbox it.

Executing an untrusted external program is not by itself a vulnerability.
Unexpected execution and boundary bypasses are in scope. Argument-handling,
path-handling, and artifact-substitution flaws are also in scope. Violations of
the documented verification contract are in scope.

Reports that affect FFmpeg, FFprobe, the operating system, or a third-party
dependency independently of ClipAsm should also go to the appropriate
upstream project. A ClipAsm report is still useful when its integration makes
an upstream issue reachable in an unexpected or insufficiently documented way.
