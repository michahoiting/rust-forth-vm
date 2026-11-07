# Copyright (c) 2026 M.W.R. Hoiting

# Local versions of the CI steps. CI calls these same targets, so the two can't drift apart.
#
# make ci         Everything that runs on the stable toolchain.
# make ci-all     Also Miri, which needs the nightly toolchain.
# make msrv       Checks the oldest supported Rust version, which needs that toolchain.
# make mutants    Mutation testing, which needs cargo-mutants.
# make doc-serve  Serves the docs on localhost, which needs python3.
# make setup-dev  Installs the developer tools and the git hooks. Run it once after cloning.
# make hooks      Installs only the git hooks in this clone.
# make protect    Applies the GitHub ruleset for main, which needs gh and admin access.
# make metrics    Reports PR lead time and issue throughput, which needs gh and python3.
# make demo-expected  Updates the expected demo output that the tests compare with.

# Stops rustup from installing a missing toolchain when a target asks for it.
export RUSTUP_AUTO_INSTALL := 0

MIRI_32_BIT_TARGET := i686-unknown-linux-gnu
DOC_PORT ?= 8000
RULESET := .github/rulesets/protect-main.json
MSRV := $(shell sed -n 's/^rust-version = "\(.*\)"/\1/p' Cargo.toml)

.DEFAULT_GOAL := ci

.PHONY: ci ci-all fmt clippy build test test-release test-minimal doc doc-serve miri miri-32 check-miri msrv msrv-version code-changed mutants demo demo-expected setup-dev hooks protect metrics

ci: fmt clippy build test test-release test-minimal doc

ci-all: ci msrv miri miri-32

fmt:
	cargo fmt --all --check

clippy:
	cargo clippy --all-targets --locked -- -D warnings
	cargo clippy --all-targets --locked --no-default-features -- -D warnings

build:
	cargo build --locked

test:
	cargo test --locked

test-release:
	cargo test --release --locked

# Without line editing, which is what the library's users can choose.
test-minimal:
	cargo test --locked --no-default-features

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

# Prints false only if BASE..HEAD changes nothing but Markdown. Renames count as code, so a
# file renamed to Markdown isn't missed.
code-changed:
	@files=$$(git diff --no-renames --name-only $(BASE) HEAD) && \
	if [ -z "$$files" ] || printf '%s\n' "$$files" | grep -qv '\.md$$'; then echo true; else echo false; fi

# Pass options through ARGS, such as make metrics ARGS="--since 2026-12-01 --prs".
metrics:
	@for tool in gh python3; do \
		command -v $$tool >/dev/null 2>&1 || { echo "$$tool is not installed."; exit 1; }; \
	done
	@python3 scripts/metrics.py $(ARGS)

mutants:
	@cargo mutants --version >/dev/null 2>&1 || { \
		echo "cargo-mutants is not installed. Install it with:"; \
		echo "  cargo install cargo-mutants --locked"; \
		exit 1; \
	}
	cargo mutants

demo:
	cargo run --locked -- --demo

# Answers the menu with 0 for all sections, then Q to quit.
demo-expected:
	printf '0\nq\n' | cargo run --quiet --locked -- --demo > tests/demo_output.txt

# Everything the other targets need, except what depends on the OS. Those tools are only
# checked for: git and rustup are required, while python3 and gh are for a few targets.
setup-dev:
	@for tool in git rustup; do \
		command -v $$tool >/dev/null 2>&1 || { \
			echo "$$tool is not installed. Install it first, then run this again. rustup is at https://rustup.rs"; \
			exit 1; \
		}; \
	done
	rustup toolchain install stable --no-self-update --profile minimal --component rustfmt,clippy
	rustup toolchain install nightly --no-self-update --profile minimal --component miri,rust-src
	rustup toolchain install $(MSRV) --no-self-update --profile minimal
	@rustup default >/dev/null 2>&1 || rustup default stable
	@cargo +stable mutants --version >/dev/null 2>&1 || cargo +stable install cargo-mutants --locked
	@$(MAKE) --no-print-directory hooks
	@command -v python3 >/dev/null 2>&1 || echo "Note: python3 is not installed. make doc-serve and make metrics need it."
	@command -v gh >/dev/null 2>&1 || echo "Note: gh is not installed. make protect and make metrics need it."

hooks:
	git config core.hooksPath .githooks

# Updates the ruleset if it already exists, so that this can run again after a change.
protect:
	@set -e; \
	id=$$(gh api 'repos/{owner}/{repo}/rulesets' --jq '.[] | select(.name == "Protect main") | .id'); \
	if [ -n "$$id" ]; then \
		gh api --method PUT "repos/{owner}/{repo}/rulesets/$$id" --input $(RULESET) --silent; \
		echo "Updated the ruleset."; \
	else \
		gh api --method POST 'repos/{owner}/{repo}/rulesets' --input $(RULESET) --silent; \
		echo "Created the ruleset."; \
	fi
