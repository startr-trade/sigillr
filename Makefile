# sigillr: run what CI runs, before pushing.
#
#   make          list the targets
#   make ci       everything .github/workflows/ci.yml runs, cheapest first, plus the Windows and
#                 macOS compile checks (clippy over every test for each target)
#
# The toolchain and targets are whatever rust-toolchain.toml pins (the version CI installs), so a
# lint CI would raise is raised here too. Heavy targets run their cargo under the shared `heavy`
# lock when `heavy` is on PATH and CI is unset; override with `make ci HEAVY=`.

SHELL := /bin/bash
.DEFAULT_GOAL := help
.NOTPARALLEL:

NPROC := $(shell nproc 2>/dev/null || echo 4)
CARGO_JOBS ?= $(shell n=$$(( $(NPROC) - 2 )); [ $$n -ge 1 ] || n=1; echo $$n)
TARGETS := x86_64-pc-windows-msvc x86_64-apple-darwin aarch64-apple-darwin

ifeq ($(origin HEAVY),undefined)
ifeq ($(CI),)
HEAVY := $(if $(shell command -v heavy 2>/dev/null),heavy compile)
else
HEAVY :=
endif
endif

.PHONY: help ci fmt lint deny cross test actionlint docs no-c

help: ## List the targets.
	@grep -hE '^[a-z-]+:.*## ' $(MAKEFILE_LIST) | awk -F':.*## ' '{printf "  make %-10s %s\n", $$1, $$2}'

ci: ## Everything CI runs on Linux plus the cross-target checks, cheapest first (no-c only on a dependency change).
	@$(MAKE) --no-print-directory fmt
	@$(MAKE) --no-print-directory lint
	@$(MAKE) --no-print-directory deny
	@$(MAKE) --no-print-directory cross
	@$(MAKE) --no-print-directory test
	@if [ -n "$(NO_C_CHANGED)" ]; then $(MAKE) --no-print-directory no-c; \
	 else echo "no-c: skipped (no dependency or toolchain change since $(BASE); run make no-c to force)"; fi
	@echo "make ci: all steps passed"

fmt: ## cargo fmt --check.
	cargo fmt --all -- --check

lint: ## cargo clippy over the workspace, warnings denied.
	$(HEAVY) cargo clippy --workspace --all-targets --locked -j$(CARGO_JOBS) -- -D warnings

deny: ## cargo-deny: advisories, licences, bans and sources (needs the advisory database).
	cargo deny --locked check advisories licenses bans sources

cross: ## Clippy over every test for Windows and macOS (compiles, does not link or run).
	@for t in $(TARGETS); do \
		echo "== $$t"; \
		$(HEAVY) cargo clippy --workspace --all-targets --locked -j$(CARGO_JOBS) --target $$t -- -D warnings || exit 1; \
	done

NEXTEST := $(shell cargo nextest --version >/dev/null 2>&1 && echo yes)

test: ## Workspace tests (cargo-nextest and doctests when nextest is installed).
ifeq ($(NEXTEST),yes)
	$(HEAVY) cargo nextest run --workspace --locked --build-jobs $(CARGO_JOBS) --no-fail-fast
	$(HEAVY) cargo test --workspace --doc --locked -j$(CARGO_JOBS)
else
	$(HEAVY) cargo test --workspace --locked -j$(CARGO_JOBS) --no-fail-fast
endif

actionlint: ## Lint the workflow files.
	actionlint .github/workflows/*.yml

docs: ## Build the user guide (mdBook).
	mdbook build docs/guide

# The no-C proof only changes when dependencies or the toolchain do, so `make ci` runs it only
# when this branch changes those files against BASE (CI also runs it nightly).
BASE ?= origin/main
NO_C_PATHS := Cargo.toml Cargo.lock rust-toolchain.toml
NO_C_CHANGED = $(shell { git diff --name-only $(BASE)...HEAD -- $(NO_C_PATHS); git diff --name-only HEAD -- $(NO_C_PATHS); } 2>/dev/null | head -1)

# Its own target directory: cargo does not rebuild for a changed CC, so reusing target/ would pass
# on artifacts a C compiler built.
no-c: ## Build everything, tests included, with no C compiler available (the build is the proof).
	CC=/bin/false CXX=/bin/false CARGO_TARGET_DIR=target/no-c $(HEAVY) cargo build --workspace --all-targets --locked -j$(CARGO_JOBS)
