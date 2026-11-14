# Conformance measurements

The [report](conformance/report.md) measures two separate profiles: the FORTH-83 Required
Word Set and ISO/IEC 15145:1997 CORE, including registered general system requirements.
The [requirements register](../tests/conformance/requirements.tsv) is the source of the
report's stable IDs, provisions, and source links. Each word definition is one register unit;
general requirements are grouped by clause. This grouping is a coverage convention, not a
normative weighting of requirements. All units are mandatory within their selected profile.

## Sources and interpretation

| Profile | Source | Provisions |
| --- | --- | --- |
| FORTH-83 | [FORTH Standards Team, August 1983](https://www.complang.tuwien.ac.at/forth/fth83std/FORTH83.TXT) | Chapter 12 required words; chapter 5 data definitions; chapters 7–10 system, usage, labeling, and errors |
| ISO Forth | [ISO/IEC 15145:1997](https://www.iso.org/standard/26479.html) | CORE and applicable system requirements |
| Supporting ANS draft | [DPANS94 CORE](https://www.taygeta.com/forth/dpans6.htm#6.1), [usage](https://www.taygeta.com/forth/dpans3.htm), [documentation](https://www.taygeta.com/forth/dpans4.htm) | Corresponding sections 6.1, 3, and 4.1; register links identify individual provisions |

The ISO catalogue identifies the normative publication. The accessible supporting text used
for the ISO probes is the **DPANS94 draft**, not a copy of the published ISO standard. A final
normative-text audit remains outstanding. Forth 2012 requirements, including its additional
numeric literal syntax, are not silently substituted for ISO requirements.

The FORTH-83 word list was checked against chapter 12 (132 words), and the ISO CORE list
against supporting section 6.1 (133 words). These lists are independent of VM availability.
General requirements include executable checks, explicit dependencies, and incomplete
assessments; they are not claimed to be an exhaustive decomposition of every normative
sentence. Application-only obligations, such as FORTH-83 section 7.1.2 and ISO section 4.2,
are outside these system profiles. Tests never exercise undefined behavior to score conformity.

## Scores and evidence

Each badge shows **100 × passed mandatory register units / all mandatory register units**,
rounded half up to one decimal place. A unit passes only when all assigned checks pass.
Deviating, missing, blocked, and not-yet-assessed units stay in the denominator. Multiple
probes do not increase a requirement's weight. The report separately shows word availability,
word behavior, and general system results.

A pass records sampled evidence for the assigned checks. It does not prove every aspect of a
word or clause. Neither a green regression check nor a 100% badge certifies conformance. The
[Forth test suite](https://forth-standard.org/standard/testsuite) likewise distinguishes test
results from a complete conformance claim. A missing word differs from an implemented word
whose test prerequisites are unavailable. Documentary assessments without complete evidence
remain unassessed and cannot earn a pass.

FORTH-83's 16-bit requirement remains explicit despite the VM's native `usize` cells. Probes
retain 16-bit numeric and allocation expectations where those affect behavior; they are not
rewritten to fit the VM. ISO tests use the native Cell width and the documented floored division
choice. Some FORTH-83 words can pass their assigned behavioral probes even though the separate
cell-width requirement fails; the profile as a whole therefore remains nonconforming.

The checked-in report and README SVG badges use the canonical **64-bit little-endian native
Cell configuration**, default stacks, and an 8192-cell dictionary per scenario. They contain a
content fingerprint of library sources, test infrastructure, the register and baseline,
manifests, Makefile, and the relevant documentation. This identifies measured source content
without the circularity of putting a generated file's own Git commit into that file. It is not
a cryptographic signature. No timestamps, host paths, or compiler-dependent debug layouts
enter the artifacts. Other architectures are measured separately and never overwrite these badges.

## Running and maintaining the checks

- `make conformance-check` validates the checked-in canonical report and badges. `make test`
  and therefore the existing CI job run this target. The complete regression measurement also
  runs in debug, release, minimal-feature, and Miri tests.
- `make conformance` regenerates the report and both badges, only after the measured outcomes
  match the reviewed baseline. Review and commit all generated changes with the source change.
- `make conformance-baseline` explicitly refreshes the canonical expected outcomes and artifacts.
  Review the baseline diff before committing: additions, improvements, regressions, removed
  checks, and stale exceptions all need review. It is not an automatic acceptance mechanism.
- `make conformance-baseline-32` explicitly refreshes the independently measured 32-bit
  baseline through Miri. Review that diff too, then run `make conformance` to refresh the
  canonical source fingerprint. Changed probe definitions and observed failure evidence are
  recorded, so existing deviations cannot silently change their results.
- `make conformance-32` runs the conformance tests on the existing 32-bit Miri target and saves
  its separate measurement summary and test log to `target/conformance/32-bit.txt`. It needs
  the same nightly Miri setup as `make miri-32` and does not generate README badges.

The [probes](../tests/conformance/cases.rs) use only the public API. A sentinel below the expected
stack, exact output comparisons, and a final depth check detect changed values, extra results,
and consumption of unrelated data. Prerequisites are drawn from known standard/VM tokens in
the probe and observer. Probe IDs and failing check IDs are recorded in the
[baseline](../tests/conformance/baseline.tsv), including passes, so removing checks is visible. The [32-bit baseline](../tests/conformance/baseline-32.tsv)
records that target’s own values and probe fingerprints; it is not copied from the 64-bit run.
Do not reuse probe IDs for different behavior without reviewing their source and regenerated
report. To add a word, first add its probes; an implemented word without probes remains blocked.

## Other word sets

These sets are inventoried, not included in the base-profile denominators. Their presence or
absence is not a conformance result in this issue. Extensions already listed in
[the word table](words.md) remain available independently of this measurement.

| Standard | Additional sets / reference material | Source |
| --- | --- | --- |
| FORTH-83 | Double Number Extension; Assembler Extension; System Extension | [Chapters 13–15](https://www.complang.tuwien.ac.at/forth/fth83std/FORTH83.TXT) |
| FORTH-83 | Controlled Reference Words; uncontrolled and experimental material (not mandatory extension sets) | [Chapter 16 and appendices B–C](https://www.complang.tuwien.ac.at/forth/fth83std/FORTH83.TXT) |
| ISO / supporting ANS draft | CORE EXT; Block; Double-Number; Exception; Facility; File-Access; Floating-Point; Locals; Memory-Allocation; Programming-Tools; Search-Order; String, including their extension sets where defined | [DPANS94 contents, sections 6.2 and 7–17](https://www.taygeta.com/forth/dpans.htm) |

Missing functionality can be tracked in the existing implementation issues; see #181 for
coordination. This measurement changes no Forth behavior and does not claim those issues are
resolved.
