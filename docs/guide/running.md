# Running an instance

Sigillr is written in Rust. This page covers what works today and what later releases add.

## Install

Build from source with a recent stable Rust toolchain. Clone
`https://github.com/startr-trade/sigillr`, then from the checkout:

```sh
cargo build --release --locked
./target/release/sigillr --help
```

## What later releases add

The current build is the workspace skeleton: it has the command-line entry point and nothing that
serves requests yet. Later releases add, in order, the registry repository and index, mutual-TLS
login and key registration, public verification, revocation checks (CRL and OCSP), and the OpenID
Connect provider. Each release's notes are under [Release notes](releases/README.md).

The settings an instance will take are listed in [Configuration](configuration.md).

## Publishing this guide

The guide is built with [mdBook](https://rust-lang.github.io/mdBook/) (`make docs`, or
`mdbook build docs/guide`) and published to GitHub Pages by `.github/workflows/docs.yml`. GitHub
Pages must be enabled once for the repository: Settings, Pages, Source "GitHub Actions". That
switch belongs to the repository owner.
