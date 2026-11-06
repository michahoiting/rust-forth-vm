# ADR 0001: Increase issue throughput without weakening `main`

- **Status:** Proposed. The owner has yet to decide on `strict`, see [#76](https://github.com/michahoiting/rust-forth-vm/issues/76).
- **Date:** 2026-11-06

## Context

Developers and AI agents should close issues faster, while `main` keeps meeting every quality standard. All work merges into `main`, and the concern was that its lengthy tests slow everyone down.

The measurements change that starting assumption. **The test suite is not the main bottleneck.** The bigger costs are what happens around CI.

| Measurement (from GitHub, 2026-11-06) | Value | What it means |
|---|---|---|
| CI wall time per run | About 3–5 min | Not lengthy in absolute terms |
| Miri share of that | 176–297 s across 4 sampled runs, as two sequential steps | The only slow job. `lint`, `build-and-test` and `msrv` each took under 40 s |
| Median PR lead time, last 8 merged | About 45 min (range 4–199) | CI is roughly 10–25% of a typical PR's life |
| Heaviest PRs | #52: 166 min, 5 CI runs, 22 reviews. #50: 199 min | Review rounds dominate, not test time |
| `strict_required_status_checks_policy` | `true` | Every merge makes all other open PRs out of date, so they need updating and a full CI rerun. PR #62 reran CI 24 s after #60 merged, which fits this. That link is inferred from timing |
| Merge settings | Auto-merge off, update-branch off | People and agents must come back and merge by hand |
| Merge gate | PR checks, not the `push` run on `main` | Nobody has to wait for the `main` run, and it repeats what the PR already proved |

Not measured: how long PRs wait for review versus how long agents spend fixing findings, and local hook times. This data also covers only one day.

## Decision

Keep every quality gate, and remove the waiting around them. Start with the short-term items, which cost little and don't weaken `main`. Keep `strict` for now, see [The `strict` decision](#the-strict-decision).

Merging this ADR doesn't change CI. Each item is implemented through its own issue.

## Options

Gain is an estimate unless it cites a measurement. Quality impact is the effect on what `main` guarantees.

"Not created" means the issue doesn't exist yet.

### Short term (days, mostly small config changes)

| ID | Issue | Idea | Solves | Expected gain | Effort | Quality impact |
|---|---|---|---|---|---|---|
| S1 | [#71](https://github.com/michahoiting/rust-forth-vm/issues/71) | **Enable auto-merge and the update-branch button** | Coming back to merge by hand | Removes idle time after approval, no extra CI | XS | None. All checks stay required |
| S2 | Not created | **Run the two Miri targets as parallel jobs** | Sequential Miri | CI from 3–5 min to about 2–2.5 min | S | None. Same tests. Adds a required `miri-32` check to the ruleset |
| S3 | [#72](https://github.com/michahoiting/rust-forth-vm/issues/72) | **Skip Miri on `.md`-only PRs** through a job-level condition | Doc PRs like #57 and #60 waiting on Miri | Doc PRs finish in under 1 min | S | None today, because no code includes a Markdown file. Revisit with #34, which will test README examples |
| S4 | [#73](https://github.com/michahoiting/rust-forth-vm/issues/73) | **Cancel superseded PR runs** with `concurrency: cancel-in-progress` | Runs on stale commits (#52 had 5) | Less queueing, small | XS | None. Never apply to `main` pushes |
| S5 | Not created | **Avoid `CHANGELOG.md` conflicts** with `merge=union` in a new `.gitattributes` | Every PR edits the same `[Unreleased]` lines | Fewer rebases for parallel agents | XS | Low. The order of entries still needs checking |
| S6 | [#74](https://github.com/michahoiting/rust-forth-vm/issues/74) | **Agents don't wait on CI**: open the PR, then start the next issue in a new worktree | Agents idle during CI | Directly raises parallelism | XS, `AGENTS.md` | None |
| S7 | [#75](https://github.com/michahoiting/rust-forth-vm/issues/75) | **Mark issues as parallel-safe** before assigning them, by the files they touch | Agents colliding in hotspots: `README.md`, `src/main.rs`, `src/interpreter.rs`, `src/vm.rs` | Fewer rebases and reruns | S, process | None |

### Medium term (weeks)

| ID | Issue | Idea | Solves | Expected gain | Effort | Quality impact |
|---|---|---|---|---|---|---|
| M1 | [#76](https://github.com/michahoiting/rust-forth-vm/issues/76) | **Decide how to handle `strict`** | Rerun after every merge | Largest CI saving once PRs run in parallel | S–M | Depends on the option |
| M2 | Not created | **Run Miri under cargo-nextest** | Miri runs tests one at a time | nextest reports up to 3–4x. Expect less on small runners, so measure it | M | Low. Loses race detection between tests, but the code has no threads. Adds a CI-only tool, not a crate dependency |
| M3 | [#34](https://github.com/michahoiting/rust-forth-vm/issues/34) | **Automate the demo comparison** | Manual demo diff against `main` on every change | Removes a manual step per PR | M | Positive |
| M4 | [#77](https://github.com/michahoiting/rust-forth-vm/issues/77) | **Risk-based review**: low-risk PRs merge on Claude review plus green CI, the owner reviews the rest | Review is likely the largest wait | Potentially the largest gain overall | S to set up | Medium. An owner policy change, so limit it to low-risk paths |
| M5 | [#78](https://github.com/michahoiting/rust-forth-vm/issues/78) | **Better first-pass agent PRs**: run `/code-review` locally before opening the PR, and write sharper acceptance criteria | Review rounds (#52 had 22 reviews) | Fewer CI reruns and review cycles | S | Positive |
| M6 | Not created | **Track metrics**: PR lead time, time to first review, CI reruns per PR | Decisions based on guesses | Shows whether the rest worked | S | None |

### Long term (months, or when volume requires it)

| ID | Issue | Idea | Solves | Expected gain | Effort | Quality impact |
|---|---|---|---|---|---|---|
| L1 | [#79](https://github.com/michahoiting/rust-forth-vm/issues/79) | **Move to a GitHub organization and use a merge queue** | `strict` reruns, with the same guarantee | Keeps "tested against latest `main`" with no manual rebasing | M. Needs `merge_group` in CI and a ruleset change | Positive or neutral. GitHub's docs list only organization-owned repositories, and this one is personal |
| L2 | Not created | **Reduce conflict hotspots**: split demo `SECTIONS` out of `src/main.rs`, use changelog fragments, generate the README word table | Parallel work touching the same lines | Lets parallel agents scale | M–L | None |
| L3 | [#80](https://github.com/michahoiting/rust-forth-vm/issues/80) | **Two-tier Miri**: 64-bit on PRs, 32-bit plus many seeds on `main` and nightly | Miri cost as tests grow | Only worthwhile if Miri exceeds about 10 min | M | **Negative**. 32-bit bugs could reach `main`, against the stated goal. Held in reserve |
| L4 | [#81](https://github.com/michahoiting/rust-forth-vm/issues/81) | **Issue-triggered agents** through the installed `claude.yml`, which was skipped in every recent run sampled | Manual agent start | More issues in progress at once | M | None, if PRs still pass the same gates |
| L5 | [#82](https://github.com/michahoiting/rust-forth-vm/issues/82) | **Incremental mutation testing** with `--in-diff` on PRs, non-blocking | Weekly feedback | Faster signal, but `make mutants` skips all `unsafe fn` primitives | M | Positive, limited |

### Rejected

Dependency caching and bigger runners. There are no crate dependencies, and the slow part is Miri interpreting tests rather than compiling.

## The `strict` decision

| Option | Throughput | Guarantee |
|---|---|---|
| **A. Keep `strict` for now (recommended)** | Reruns after each merge, with the cost cut by S2 and S3 | Every commit on `main` was tested as merged |
| B. Turn `strict` off, keep the `main` push run | Best without moving the repo | Two green PRs can combine into a broken `main`, found only after merging |
| C. Move to an organization with a merge queue (L1) | Best long term | Same as A |

Recommendation: A now, then C when several PRs are regularly open at once. This fits the requirement that `main` must stay qualified. With about 1–3 PRs open, the reruns cost roughly 2 min each after S2.

## Recommended roadmap

1. Measure first (M6): add time to first review to the numbers above, to confirm whether review (M4) is the top lever.
2. Do S1–S5 together. They are small, and one PR can close all five issues.
3. Update `AGENTS.md` for S6 and S7.
4. Pick up M3, M2 and M5, then M4 if the data supports it.
5. Revisit L1 once parallel PRs are routine.

## Files affected by the roadmap

- `.github/workflows/ci.yml`: split Miri (S2), the skip condition (S3), concurrency (S4).
- `.github/workflows/lint.yml`: concurrency (S4).
- `.github/rulesets/protect-main.json`: the `miri-32` check. The owner applies it with `make protect`.
- `Makefile`: keeps the `miri` and `miri-32` targets, and gets any new CI step.
- `.gitattributes` (new): `CHANGELOG.md merge=union`.
- `AGENTS.md` and `CONTRIBUTING.md`: workflow changes.
- Repository settings (S1): auto-merge and update-branch.

## Relation to other open issues

| Issue | Type | Description | Relation |
|---|---|---|---|
| #34 | enhancement | Test the demo output and the README examples | This is M3. It also changes S3, since README examples then affect tests |
| #35 | enhancement | Add common missing words | Benefits. Touches every hotspot, so split it up for S7 |
| #37 | documentation | Bring the README up to date | Benefits from S3 |
| #36 | documentation | Update text that still says line where it means a whole string | Benefits from S3 if docs-only. Rustdoc edits still run Miri |
| #13 | | Rename Registers to Vm | Wide rename that conflicts with parallel work. Schedule alone (S7) |
| #38 | enhancement | Rename one of the two execute functions | Same as #13, smaller |
| #49 | enhancement | Replace raw addresses in Cells with indexes | Wide change, schedule alone (S7). May change Miri time |
| #14 | | Audit whether the public API can be made safe | Unaffected. Keep full Miri |
| #15 | bug | Comma can write into an open definition and execute arbitrary cells | Unaffected. Keep full Miri, argues against L3 |
| #26 | bug | `@` can read the address of LIT, and EXECUTE can then crash the VM | Unaffected. Keep full Miri, argues against L3 |
| #32 | bug | The stacks have no size limit, so runaway recursion uses up memory | Unaffected |
| #39 | enhancement | EXECUTE walks the whole word list on every call | Unaffected |
| #55 | enhancement | Follow the standard for input number conversion | Unaffected |
| #67 | | Run the whole demo from a command line argument | Unaffected |
| #70 | | Add --version, with build information from git | Unaffected |

## Verification

Each item's issue has its own acceptance criteria. For the roadmap as a whole:

- Compare CI wall time before and after on the same commit, targeting about 2–2.5 min.
- Open a `.md`-only test PR: Miri skips, required checks report success, and the PR can merge. This is the main risk of S3, because a required check that never reports blocks the PR.
- Open a test PR that changes `src/`: all Miri jobs run and are required.
- Temporarily break a test on a PR: auto-merge must not merge it.
- Push twice quickly: the first PR run cancels, and `main` push runs never cancel.
- Rebase two branches that both add changelog entries: no conflict, and both entries remain.
- Measure again after two weeks: PR lead time, CI reruns per PR, issues closed per week.

## Consequences

- `main` keeps the same checks. The only item that would weaken them, L3, is held in reserve.
- Doc PRs no longer run Miri, so that rule must change when Markdown starts affecting tests.
- Agents work on more issues at once, so review load on the owner goes up until M4 or M5 reduce it.
- The recommendation rests on one day of data, so M6 could change the priorities.

## Sources

- [Managing a merge queue, GitHub Docs](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/configuring-pull-request-merges/managing-a-merge-queue)
- [Miri and nextest](https://www.nexte.st/docs/integrations/miri/)
