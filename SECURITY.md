# Security policy

ClipAsm is pre-release software that processes local media through FFmpeg and
FFprobe and can execute explicitly declared external programs. Security reports
are welcome and should be handled privately until a fix or disclosure plan is
ready.

## Supported versions

Security fixes target the current `main` branch and the latest published
release, when one exists. Older commits and releases may not receive fixes.

## Report a vulnerability

Do not include exploit details, private media, credentials, or other sensitive
information in a public issue.

Use GitHub's
[private vulnerability reporting form](https://github.com/NikitaMGrimm/clipasm/security/advisories/new)
when it is available. If GitHub does not offer the private form, open a public
issue containing only a request for a private contact channel and a
non-sensitive description of the affected area.

Include enough information to reproduce and assess the report:

- the affected ClipAsm commit or release;
- the operating system and relevant FFmpeg and FFprobe versions;
- the smallest safe reproducer you can provide;
- the expected security boundary and the observed behavior;
- the likely impact and any known workarounds.

There is currently no guaranteed response or remediation time. Please allow a
reasonable opportunity to investigate before public disclosure.

## Expected trust boundaries

External programs are trusted native code. Rendering a reachable external
program intentionally executes its declared executable with the current user's
permissions; ClipAsm does not sandbox it. Executing an untrusted external
program is therefore not by itself a vulnerability. Unexpected execution,
boundary bypasses, argument or path handling flaws, artifact-substitution
issues, and violations of the documented verification contract are in scope.

Reports that affect FFmpeg, FFprobe, the operating system, or a third-party
dependency independently of ClipAsm should also be reported to the appropriate
upstream project. A ClipAsm report is still useful when its integration makes
an upstream issue reachable in an unexpected or insufficiently documented way.
