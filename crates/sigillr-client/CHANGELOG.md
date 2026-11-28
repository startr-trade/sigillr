# Changelog

All notable changes to `sigillr-client` are documented in this file. The crate has its own version
line, apart from the `sigillr` server's.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this crate adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-11-28

The first release, ahead of the `sigillr` server's 0.2.0, so verifiers can depend on it now.

### Added

- `Client` looks up a key fingerprint (`key`) or a user_id (`identity`) in one Sigillr instance you
  pin, and returns the bound identity, its status, and the registry commit the answer came from.
  `status_at` and `is_valid` tell whether a key is valid at a given time; a status the client does
  not know counts as not valid.
- Answers are cached in memory, bounded by size and by a lifetime you can shorten. The client never
  returns an answer past its lifetime, and never a stale answer when a lookup fails.
- `ClientBuilder` sets the online service or a mirror, the cache bounds, timeouts, extra root
  certificates, and pins on the server's public keys. When a mirror is set, the client never falls
  back to the online service.
- The `testing` feature exports `FakeTransport` and `FakeClock` for your own tests; it adds no
  dependency.

### Notes

- The wire types carry a `public_key` field, but a Sigillr server does not send it yet: it reads as
  `None` until a server release adds it.
- The client supports reading from a mirror, but no public mirror exists yet.
- `ureq` is held to its 3.5 minor (`~3.5.2`): key pinning uses `ureq::unversioned`, which ureq may
  change in a minor release. The versions of `ureq` and `rustls` are not part of the public API.
- Pure Rust, no C: TLS is rustls with the graviola provider.
