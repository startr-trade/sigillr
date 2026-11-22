<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/startr-trade/sigillr/main/assets/brand/sigillr-lockup-dark.svg">
    <img src="https://raw.githubusercontent.com/startr-trade/sigillr/main/assets/brand/sigillr-lockup-light.svg" alt="Sigillr" width="360">
  </picture>
</p>

<p align="center">
  <b>A seal you can check, with nothing personal behind it.</b>
</p>

Sigillr is a public identity registry. People and organisations register the public keys of their certificates under a pseudonymous user_id, log in with the certificate they belong to, and let anyone verify which identity a key or certificate is bound to. It is also an OpenID Connect provider whose ID tokens carry only its issuer and the user_id. It holds no personally identifiable data.

Documentation: <https://startr-trade.github.io/sigillr/> (the user guide sources are in [docs/guide/](docs/guide/)). To contribute, see [CONTRIBUTING](CONTRIBUTING.md); to report a vulnerability, see [SECURITY](SECURITY.md).

## Status

Early. Version 0.1.0 claims the name and answers `--version` and `--help`; the registry itself is still being built.

## Install

```sh
cargo install sigillr
```

Or a prebuilt binary from [GitHub Releases](https://github.com/startr-trade/sigillr/releases/latest).

## Quick start

```sh
sigillr --help
sigillr --version
```

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
