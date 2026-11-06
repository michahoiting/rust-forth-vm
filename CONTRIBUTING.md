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
`git` and `rustup`, and tells you if `python3` or `gh` is missing, which only `make doc-serve`
and `make protect` need.

The hooks catch problems before CI does:

| Hook         | Refuses                  | Runs                  |
|--------------|--------------------------|-----------------------|
| `pre-commit` | Commits on `main`        | `make fmt`            |
| `pre-push`   | Pushes to `main`         | `make clippy test`    |

If a hook itself gets in the way, `git commit --no-verify` and `git push --no-verify` skip
it. CI still runs the same checks.

## Making a change

1. Find or open a [GitHub issue](https://github.com/michahoiting/rust-forth-vm/issues)
   that states the problem and the acceptance criteria. For anything beyond a small fix,
   agree on the approach there first.
2. Work on a branch. If you don't have write access, work on a branch in your fork.
3. Follow the conventions in [AGENTS.md](AGENTS.md). New Forth words have their own
   checklist there.
4. Add an entry under `[Unreleased]` in [CHANGELOG.md](CHANGELOG.md) for anything a user
   would notice. Start breaking changes with `**Breaking:**`.
5. Run `make ci`.
6. Open a PR that closes the issue. The project owner reviews and merges.

## Why `main` refuses pushes

A GitHub ruleset protects `main`, for everyone including the project owner. Changes land
through a PR once the `build-and-test`, `msrv` and `miri` checks pass and the branch is up
to date with `main`. Force-pushing to `main` and deleting it are blocked.

The ruleset is kept in [`.github/rulesets/protect-main.json`](.github/rulesets/protect-main.json).
After changing it, a repository admin applies it with `make protect`, which needs the `gh`
CLI. If you rename a CI job, update the file too, or PRs will wait for a check that no
longer exists.

## Versioning

Versions follow [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html). The public
API that the version number covers is:

- The public items of the Rust library.
- The Forth words and their documented behaviour.
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
3. Set `version` in `Cargo.toml`, and run `cargo build` to update `Cargo.lock`.
4. After merging, tag the merge commit and push the tag:

   ```sh
   git switch main
   git pull --ff-only
   git tag -a v0.2.0 -m "Version 0.2.0"
   git push origin v0.2.0
   ```
