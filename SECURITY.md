# Security Policy

## Supported versions

`fulgur-chart` is pre-1.0 and developed as a side project. Security fixes are applied to
the latest released version and to the `main` branch. Older versions are not maintained.

## Reporting a vulnerability

**Please do not report security vulnerabilities through public GitHub issues, pull
requests, or discussions.**

Instead, report them privately through GitHub's private vulnerability reporting:

1. Go to the repository's **Security** tab.
2. Click **Report a vulnerability** (under "Advisories").
3. Fill in the form with the details below.

> [!NOTE]
> This requires *Private vulnerability reporting* to be enabled for the repository
> (Settings → Code security and analysis). If the "Report a vulnerability" button is not
> visible, the feature has not been enabled yet — see the maintainer note at the bottom.

Please include, where possible:

- A description of the vulnerability and its impact.
- A minimal spec or input that triggers it, and the exact command used.
- The affected version (`fulgur-chart --version`) or commit hash.
- Any suggested mitigation.

Note that this tool processes **untrusted input specs** to produce images; reports about
how malformed or adversarial input is handled are in scope.

## What to expect

As a side project, responses are best-effort. We aim to acknowledge a report within a
reasonable time, confirm the issue, and coordinate a fix and disclosure timeline with
you. Please give us a chance to release a fix before any public disclosure.

---

<!-- Maintainer note: Enable "Private vulnerability reporting" in
     Settings → Code security and analysis so the "Report a vulnerability" button above
     works. Until then, the instructions in this file point at a button that does not
     exist. -->
