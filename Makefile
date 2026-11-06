# (C) Copyright 2026 M.W.R. Hoiting

# Local versions of the CI steps. CI calls these same targets, so the two can't drift apart.
#
# make ci         Everything that runs on the stable toolchain.
# make ci-all     Also Miri, which needs the nightly toolchain.
# make msrv       Checks the oldest supported Rust version, which needs that toolchain.
# make mutants    Mutation testing, which needs cargo-mutants.
# make doc-serve  Serves the docs on localhost, which needs python3.

# Stops rustup from installing a missing toolchain when a target asks for it.
export RUSTUP_AUTO_INSTALL := 0

MIRI_32_BIT_TARGET := i686-unknown-linux-gnu
DOC_PORT ?= 8000
MSRV := $(shell sed -n 's/^rust-version = "\(.*\)"/\1/p' Cargo.toml)

.DEFAULT_GOAL := ci

.PHONY: ci ci-all fmt clippy build test test-release doc doc-serve miri miri-32 check-miri msrv msrv-version mutants demo

ci: fmt clippy build test test-release doc

ci-all: ci msrv miri miri-32

fmt:
	cargo fmt --all --check

clippy:
	cargo clippy --all-targets --locked -- -D warnings

build:
	cargo build --locked

test:
	cargo test --locked

test-release:
	cargo test --release --locked

# Both views, because each one can have broken links that the other doesn't render.
doc:
	RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --locked
	RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --locked --document-private-items

doc-serve:
	cargo doc --no-deps --locked --document-private-items
	@echo "Serving the docs at http://localhost:$(DOC_PORT)/rust_forth_vm/ (Ctrl-C to stop)"
	python3 -m http.server $(DOC_PORT) --bind 127.0.0.1 --directory target/doc

miri: check-miri
	cargo +nightly miri test --locked

miri-32: check-miri
	cargo +nightly miri test --locked --target $(MIRI_32_BIT_TARGET)

check-miri:
	@cargo +nightly miri --version >/dev/null 2>&1 || { \
		echo "Miri is not installed. Install it with:"; \
		echo "  rustup toolchain install nightly --profile minimal --component miri,rust-src"; \
		exit 1; \
	}

msrv:
	@cargo +$(MSRV) --version >/dev/null 2>&1 || { \
		echo "Rust $(MSRV) is not installed. Install it with:"; \
		echo "  rustup toolchain install $(MSRV) --profile minimal"; \
		exit 1; \
	}
	cargo +$(MSRV) check --all-targets --locked

msrv-version:
	@echo $(MSRV)

mutants:
	@cargo mutants --version >/dev/null 2>&1 || { \
		echo "cargo-mutants is not installed. Install it with:"; \
		echo "  cargo install cargo-mutants --locked"; \
		exit 1; \
	}
	cargo mutants

demo:
	cargo run --locked
