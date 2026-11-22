# Configuration

Every setting below is **planned**: no code reads any of them yet. The table fixes the intended
behaviour and defaults so that operators and later releases agree. It mirrors the architecture
view of configuration; a value marked "open question" is not yet decided.

| Setting | Default | Status |
|---|---|---|
| accepted certificate issuers | open question | planned |
| registry repository location | none; required | planned |
| index database | none; required, disposable | planned |
| waiting window for recovery and disable requests | 48 hours | planned |
| authorisation code lifetime | 60 seconds, single use, PKCE (S256) required | planned |
| ID token lifetime | 5 minutes | planned |
| access token lifetime | 10 minutes, sender-constrained (RFC 8705 mTLS or RFC 9449 DPoP) | planned |
| refresh token | rotated on every use; idle limit 7 days; absolute limit 30 days, never past the login certificate's expiry | planned |
| CA revocation re-check | at every login and every token refresh | planned |
| cached OCSP/CRL answer reuse | until its nextUpdate, at most 1 hour; no fresh answer means refusal | planned |

The token values follow the OAuth 2.0 Security Best Current Practice (RFC 9700). The index database
can be deleted at any time: it is rebuilt from the registry repository.
