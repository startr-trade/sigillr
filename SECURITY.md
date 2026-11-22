# Security Policy

## Reporting a vulnerability

**Please do not open a public issue or pull request for security problems.**

Report it privately through GitHub's private vulnerability reporting: open the repository's
Security tab and choose "Report a vulnerability"
(<https://github.com/startr-trade/sigillr/security/advisories/new>).

Please include: the affected version or commit, steps to reproduce, the impact, and any suggested
fix.

### What to expect

- Acknowledgement on a best-effort basis; this is a small project with no guaranteed response
  time.
- A fix or mitigation in a later release, with credit in the release notes unless you prefer to
  stay anonymous.

## Supported versions

| Version | Supported |
|---|---|
| Latest minor release | yes |
| Older releases | no |

Please reproduce an issue against the newest release (or `main`) before reporting.

## In scope

- The sigillr service: registration, mTLS login, verification, revocation and expiry.
- The integrity of the registry repository (the git repository of identity records and public keys).
- The OpenID Connect provider: token issuance, authorisations and their revocation.

## Out of scope

- The security of your own certificate authority, git host or operating system.
- Vulnerabilities in third-party dependencies that do not affect sigillr's use of them. Report
  those upstream.
- Malformed input that only makes sigillr report an error and carry on.
