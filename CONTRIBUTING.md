# Contributing

Thanks for your interest in rust-forth-vm. This guide covers setting up, making a change,
and how versions and releases work. The code conventions are in [AGENTS.md](AGENTS.md),
which applies to people and coding agents alike.

## Setting up

Follow [Getting started](README.md#getting-started) first, then run this once in your
clone:

```sh
make setup-dev
```

It installs or updates the stable and nightly toolchains, the oldest supported Rust version,
Miri, and `cargo-mutants`, and installs the git hooks. It's safe to run again. It needs
`git` and `rustup`, and tells you if `python3` or `gh` is missing, which only `make doc-serve`,
`make protect`, `make metrics` and the `make perf` targets need.

The hooks catch problems before CI does:

| Hook         | Refuses                  | Runs                  |
|--------------|--------------------------|-----------------------|
| `pre-commit` | Commits on `main`        | `make fmt`            |
| `pre-push`   | Pushes to `main`         | `make clippy test`    |

If a hook itself gets in the way, `git commit --no-verify` and `git push --no-verify` skip
it. CI still runs the same checks.

## Making a change

1. Find or open a [GitHub issue](https://github.com/michahoiting/rust-forth-vm/issues)
   that states the problem, the acceptance criteria, and the files it expects to touch. For
   anything beyond a small fix, agree on the approach there first. The `solo` label marks
   changes throughout the project, which run with no other PR open.
2. Work on a branch. If you don't have write access, work on a branch in your fork.
3. Follow the conventions in [AGENTS.md](AGENTS.md). New Forth words have their own
   checklist there.
4. Add an entry under `[Unreleased]` in [CHANGELOG.md](CHANGELOG.md) for anything a user
   would notice. Everything is part of the unreleased `0.1.0` until the project owner
   releases it. Until then, all entries go under `### Added`, including fixes and changes,
   and nothing is marked as breaking. After the first release, this project adheres to
   Semantic Versioning, and breaking changes start with `**Breaking:**`.
5. Run `make ci`. If you changed the demo on purpose, run `make demo-expected` and check the
   diff of `tests/demo_output.txt`. A test also runs the Forth example in the README, and
   checks the results that its comments state.
6. Open a PR that closes the issue. The project owner reviews and merges.

If you use the Claude CLI, see [Working with Claude Code](README.md#working-with-claude-code)
for how these steps look in practice.

If `main` moves on, use **Update branch** on the PR to bring it up to date. After reviewing,
the project owner can arm auto-merge, which merges once the checks pass. Only the owner arms
it, because the ruleset requires no approvals, so nothing else stops an unreviewed merge.

`CHANGELOG.md` uses git's `union` merge driver, so entries added on two branches don't
conflict in a local merge or rebase. Check the order of the entries afterwards.

## Mutation testing

`make mutants` runs the full mutation suite, which also runs weekly in GitHub Actions. To
test only mutations overlapping changes in a diff whose new side matches the working tree:

```sh
git diff origin/main HEAD > /tmp/pr.diff
make mutants-diff DIFF=/tmp/pr.diff
```

Pull requests run the incremental suite as a non-required check. The Actions summary reports
its mutation-testing time and results; same-repository PRs also get a comment for each run.
Fork PRs don't get a comment. A failed advisory check doesn't block merging. The incremental
suite doesn't replace the weekly run: it ignores changes outside the diff and can miss lost
coverage caused by changes to tests. The configured exclusions still apply, and cargo-mutants
skips `unsafe fn`, including the primitives.

## Measuring performance

`make perf` measures the VM on a set of Forth workloads in `examples/perf/`, and compares the
result with your previous run. It counts instructions instead of time, because a count hardly
depends on the machine or what else it's doing. [ADR-0002](docs/adr/0002-measure-performance-with-instruction-counts.md)
explains the approach.

```sh
make perf                        # Measures the working tree, and stores the result
make perf ARGS="--note 'Before'" # Stores a note with the run
make perf-report                 # Shows the development per workload, without measuring
make perf-compare BASE=origin/main  # Measures that commit too, in a temporary worktree
```

`make perf` and `make perf-compare` exit with an error when a gated workload is 5% or more
slower, and show a warning from 2%. Workloads with a changed source aren't compared.

The counting depends on the machine:

| Backend      | Used when                 | Metric                                        |
|--------------|---------------------------|-----------------------------------------------|
| `cachegrind` | Valgrind is installed     | Instructions and branches, counted in software |
| `macos-pmu`  | On macOS without Valgrind | Instructions from `/usr/bin/time -l`, median of 5 runs |
| `cpu-time`   | Anywhere else             | User time, median of 5 runs. It's too noisy to warn or fail on |

Results go to `target/perf/results.sqlite`, so `cargo clean` removes them, and each worktree has
its own. Only runs with the same backend, target, machine and compiler are compared.

## Why `main` refuses pushes

A GitHub ruleset protects `main`, for everyone including the project owner. Changes land
through a PR once the `lint`, `build-and-test`, `msrv`, `miri` and `miri-32` checks pass and
the branch is up to date with `main`. PRs that only change Markdown skip the two Miri jobs,
which then count as passed. Force-pushing to `main` and deleting it are blocked.

The ruleset is kept in [`.github/rulesets/protect-main.json`](.github/rulesets/protect-main.json).
After changing it, a repository admin applies it with `make protect`, which needs the `gh`
CLI. If you rename a CI job, update the file too, or PRs will wait for a check that no
longer exists.

## Versioning

After the first release, versions follow
[Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html). The public API that the
version number covers is:

- The public items of the Rust library.
- The Forth words and their documented behavior.
- The messages of `ForthError`.

Demo output and private items aren't part of it.

The project is in initial development, on `0.y.z`, so the API isn't stable yet. Until
`1.0.0`, which is the project owner's decision:

| Change                                  | Bump    |
|-----------------------------------------|---------|
| Breaking change                         | `MINOR` |
| Raising `rust-version`                  | `MINOR` |
| Anything else that gets released        | `PATCH` |

From `1.0.0` on, breaking changes bump `MAJOR`, and new features bump `MINOR`. Raising
`rust-version` stays a `MINOR` bump.

## Releasing

The project owner makes releases:

1. In a PR, rename `[Unreleased]` in `CHANGELOG.md` to the new version and today's date,
   such as `## [0.2.0] - 2026-12-01`, and add a new empty `[Unreleased]` above it.
2. Update the links at the bottom of `CHANGELOG.md`: `[Unreleased]` compares the new tag
   with `HEAD`, and the new version compares the previous tag with the new one. The first
   release links to its tag instead.
3. Set `version` in `Cargo.toml`, and run `cargo build` to update `Cargo.lock`. Also update
   the current version in the [Versioning](README.md#versioning) section of the README,
   and the sentence about the unreleased version in the `CHANGELOG.md` intro.
4. After merging, tag the merge commit and push the tag:

   ```sh
   git switch main
   git pull --ff-only
   git tag -a 0.2.0 -m "Version 0.2.0"
   git push origin 0.2.0
   ```
