# Local versions of the CI steps. CI calls these same targets, so the two can't drift apart.
#
# make ci       Everything that runs on the stable toolchain.
# make ci-all   Also Miri, which needs the nightly toolchain.
# make mutants  Mutation testing, which needs cargo-mutants.

MIRI_32_BIT_TARGET := i686-unknown-linux-gnu

.DEFAULT_GOAL := ci

.PHONY: ci ci-all fmt clippy build test test-release doc miri miri-32 check-miri mutants demo

ci: fmt clippy build test test-release doc

ci-all: ci miri miri-32

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

doc:
	RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --locked

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

mutants:
	@cargo mutants --version >/dev/null 2>&1 || { \
		echo "cargo-mutants is not installed. Install it with:"; \
		echo "  cargo install cargo-mutants --locked"; \
		exit 1; \
	}
	cargo mutants

demo:
	cargo run --locked
