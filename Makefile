# Copyright (c) 2026 M.W.R. Hoiting

# Local versions of the CI steps. CI calls these same targets, so the two can't drift apart.
#
# make ci         Everything that runs on the stable toolchain.
# make ci-all     Also Miri, which needs the nightly toolchain.
# make msrv       Checks the oldest supported Rust version, which needs that toolchain.
# make mutants    Mutation testing, which needs cargo-mutants.
# make mutants-diff DIFF=path  Test mutations overlapping a diff.
# make mutants-test  Test incremental mutation reporting.
# make doc-serve  Serves the docs on localhost, which needs python3.
# make setup-dev  Installs the developer tools and the git hooks. Run it once after cloning.
# make hooks      Installs only the git hooks in this clone.
# make protect    Applies the GitHub rulesets, which needs gh, python3 and admin access.
#                 make protect RULESETS=path applies only the given files.
# make metrics    Reports PR lead time and issue throughput, which needs gh and python3.
# make report-miri RUN_URL=url  Opens or updates the issue for a failed nightly Miri run, which needs gh.
# make actionlint Checks the GitHub workflows, which needs actionlint.
# make demo-expected  Updates the expected demo output that the tests compare with.
# make perf       Measures the VM, and stores the result under target/perf, which needs python3.
# make perf-report   Shows the stored development of the measurements.
# make perf-compare  Measures another commit too, such as make perf-compare BASE=origin/main.
# make perf-pad-check  Checks that gated workloads don't depend on the size of the dictionary.
# make perf-test  Runs the unit tests of the measuring script.
# make perf-ci    Compares two commits as CI does, such as make perf-ci BASE=HEAD^1.
# make perf-record  Appends a CI result to the history of main, which the perf workflow does.

# Stops rustup from installing a missing toolchain when a target asks for it.
export RUSTUP_AUTO_INSTALL := 0

MIRI_32_BIT_TARGET := i686-unknown-linux-gnu
DOC_PORT ?= 8000
RULESETS ?= $(wildcard .github/rulesets/*.json)
MSRV := $(shell sed -n 's/^rust-version = "\(.*\)"/\1/p' Cargo.toml)

.DEFAULT_GOAL := ci

.PHONY: ci ci-all fmt clippy build test test-release test-minimal doc doc-serve miri miri-32 check-miri report-miri actionlint msrv msrv-version code-changed mutants mutants-diff mutants-test check-mutants demo demo-expected setup-dev hooks protect metrics perf perf-report perf-compare perf-pad-check perf-test perf-ci perf-record check-python

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

# Without line editing, which is what the library's users can choose. It has its own target
# directory, because the tests build the program, which would replace the one in target/debug.
test-minimal:
	cargo test --locked --no-default-features --target-dir target/minimal

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

# Opens an issue for a failed nightly Miri run, or comments on the open one. The workflow
# passes RUN_URL, the link to the run.
report-miri:
	@test -n "$(RUN_URL)" || { echo "Set RUN_URL to the link of the failed run."; exit 1; }
	@gh label create miri-failure --color B60205 --description "A nightly Miri run failed" >/dev/null 2>&1 || true
	@number=$$(gh issue list --state open --label miri-failure --limit 1 --json number --jq '.[0].number') && \
	if [ -n "$$number" ]; then \
		gh issue comment "$$number" --body "Nightly Miri run failed: $(RUN_URL)"; \
	else \
		gh issue create --title "Nightly Miri run fails" --label miri-failure --body "Nightly Miri run failed: $(RUN_URL)"; \
	fi

# Checks the GitHub workflows with actionlint. CI runs the same target in the lint job.
actionlint:
	@command -v actionlint >/dev/null 2>&1 || { \
		echo "actionlint is not installed. Install it with brew install actionlint, or see https://github.com/rhysd/actionlint"; \
		exit 1; \
	}
	actionlint

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

check-python:
	@command -v python3 >/dev/null 2>&1 || { echo "python3 is not installed."; exit 1; }

# Pass options through ARGS, such as make perf ARGS="--note 'Before the rewrite'".
perf: check-python
	@python3 scripts/perf.py run $(ARGS)

perf-report: check-python
	@python3 scripts/perf.py report $(ARGS)

perf-compare: check-python
	@python3 scripts/perf.py compare $(or $(BASE),main) $(ARGS)

perf-pad-check: check-python
	@python3 scripts/perf.py pad-check $(ARGS)

perf-test: check-python
	@cd scripts && python3 -m unittest test_perf

# What the perf workflow runs: the unit tests, then a comparison of the commit BASE with HEAD.
# The report and Cachegrind's files go to OUT, by default target/perf/ci. With HISTORY, a
# history.jsonl of main, the report also shows the development and drift of each workload.
perf-ci: perf-test
	@python3 scripts/perf.py ci $(or $(BASE),main) $(if $(OUT),--out "$(OUT)") \
		$(if $(HISTORY),--history "$(HISTORY)") $(ARGS)

# Appends the head of the summary.json that perf-ci wrote, SUMMARY, to the history HISTORY. It
# does nothing when nothing was measured. The perf workflow runs it on each push to main.
perf-record: check-python
	@test -n "$(SUMMARY)" -a -n "$(HISTORY)" || { echo "Usage: make perf-record SUMMARY=... HISTORY=..."; exit 2; }
	@python3 scripts/perf.py record "$(SUMMARY)" "$(HISTORY)" $(ARGS)

check-mutants:
	@cargo mutants --version >/dev/null 2>&1 || { \
		echo "cargo-mutants is not installed. Install it with:"; \
		echo "  cargo install cargo-mutants --locked"; \
		exit 1; \
	}

mutants: check-mutants
	cargo mutants

export DIFF
mutants-diff: check-mutants
	@test -n "$$DIFF" && test -f "$$DIFF" && test -r "$$DIFF" || { \
		echo "Set DIFF to a readable diff file, for example make mutants-diff DIFF=/tmp/pr.diff"; \
		exit 1; \
	}
	@cargo mutants --in-diff "$$DIFF"; status=$$?; \
	if [ -n "$$MUTANTS_STATUS_FILE" ]; then \
		printf '%s\n' "$$status" > "$$MUTANTS_STATUS_FILE" || exit 1; \
	fi; \
	exit $$status

mutants-test: check-python
	@cd scripts && python3 -m unittest test_mutants_report test_mutants_workflow

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
	@command -v python3 >/dev/null 2>&1 || echo "Note: python3 is not installed. make doc-serve, make metrics and make perf need it."
	@command -v gh >/dev/null 2>&1 || echo "Note: gh is not installed. make protect and make metrics need it."
	@command -v actionlint >/dev/null 2>&1 || echo "Note: actionlint is not installed. It checks the GitHub workflows. Install it with brew install actionlint."
	@command -v shellcheck >/dev/null 2>&1 || echo "Note: shellcheck is not installed. actionlint uses it to check the run: scripts. Install it with brew install shellcheck."

hooks:
	git config core.hooksPath .githooks

# Applies each ruleset file. A ruleset with the file's name is updated, so that this can run
# again after a change, and a missing one is created.
protect: check-python
	@test -n "$(RULESETS)" || { echo "No ruleset files to apply."; exit 1; }
	@set -e; \
	for file in $(RULESETS); do \
		name=$$(python3 -c 'import json, sys; print(json.load(open(sys.argv[1]))["name"])' "$$file"); \
		id=$$(gh api 'repos/{owner}/{repo}/rulesets' --jq ".[] | select(.name == \"$$name\") | .id"); \
		if [ -n "$$id" ]; then \
			gh api --method PUT "repos/{owner}/{repo}/rulesets/$$id" --input "$$file" --silent; \
			echo "Updated the ruleset $$name."; \
		else \
			gh api --method POST 'repos/{owner}/{repo}/rulesets' --input "$$file" --silent; \
			echo "Created the ruleset $$name."; \
		fi; \
	done
