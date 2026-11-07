# ADR 0002: Measure performance with instruction counts

- **Status:** Accepted, see [#125](https://github.com/michahoiting/rust-forth-vm/issues/125), which also has the full measurements and the later steps.
- **Date:** 2026-11-07

## Context

A slowdown should show up on the PR that causes it, with a result that doesn't depend on the machine, in about 2.5 minutes of CI time. We measured on GitHub runners and on an Intel Mac.

| Measurement | Value | What it means |
|---|---|---|
| Time of one binary across 3 CI jobs | Up to 60% apart | The jobs got different CPUs, so time can't decide anything |
| Cachegrind `Ir` across the same jobs | At most 0.013% apart | A count is stable enough to gate on |
| `perf_event_open` on a runner | Fails, also after `sudo sysctl` | Hardware counters aren't available in CI |
| Cachegrind on one small extra operation in `pop` | +1.5% on `stack`, 0.00–0.01% for cold code | It sees small changes, without false alarms |
| Inlining side effect of a change elsewhere | Up to 2.7% with `codegen-units=1`, 0.03% with the default | A real shift, with no real cause, so the failure limit must be above this |
| 10 extra words | `interpret` +15%, `threading` 0% | Lookup walks the list, so new words slow down lookups |
| `/usr/bin/time -l` on macOS | Varies 0.04–0.38% | Good enough locally, and needs no root |

## Decision

- **Count instructions.** Cachegrind where Valgrind exists, and `/usr/bin/time -l` on macOS. Time is only a last resort, and never decides anything.
- **Don't add a dependency.** Workloads are an example in `examples/perf/`, and a script in the standard library of Python runs them. Gungraun would add a dev-dependency that `make msrv` has to build, plus a runner binary.
- **Measure the settings users get.** The `perf` profile is `release` with line tables.
- **Warn from +2%, fail from +5%,** for each gated workload. A workload with a changed source isn't compared.
- **Gated workloads must not depend on the size of the dictionary,** which `make perf-pad-check` tests. Others are reported, and never fail, or every PR with new words would fail.
- **Only compare like with like:** the same backend, target, machine and compiler. In CI, base and head run in one job, so a new compiler can't fail a PR.

## Consequences

- A cost that instructions don't show, such as cache misses or mispredicted branches, doesn't fail anything. `Bim` is stored to make branch costs visible.
- Each new primitive shows up as a slowdown in the workloads that aren't gated. That cost is real, and it's visible in the report.
- `boot` varies more than the other workloads under `/usr/bin/time -l`, so a local warning on it means less than one in CI.
