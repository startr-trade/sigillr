# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-11-22

### Added

- The `sigillr` command with `--version` and `--help`, published on crates.io.
- Release archives for Linux, macOS and Windows with SHA-256 checksums, a CycloneDX SBOM, build-provenance attestations, and the `install.sh` / `install.ps1` installers.
- The user guide (mdBook): introduction, running an instance, configuration reference.
- The Rust workspace on a pinned toolchain, with one crate per component.
- `make ci` and the GitHub Actions workflows: CI on three systems, no-C build proof, supply-chain checks (cargo-deny), CodeQL, docs and release.
- Repository checks for the toolchain pin, pinned actions, licences, brand assets, README, guide links, release workflow and agent-neutral wording.
- Brand assets: mark, light and dark lockups with the Latin origin word *sigillum*, favicon, app icon and social preview.
- MIT OR Apache-2.0 licence, CONTRIBUTING, SECURITY, CODE_OF_CONDUCT and CONTRIBUTORS.
