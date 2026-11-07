# Copyright (c) 2026 M.W.R. Hoiting

"""Measures the Forth workloads in examples/perf. Run it with `make perf`.

It counts instructions instead of time, because a count hardly depends on the machine or what
else it's doing. See ADR-0002 in docs/adr for why, and for what the limits below are based on.

Results go to a SQLite database under the target directory, so `cargo clean` removes them. Only
runs with the same backend, target, machine and compiler are compared, and a workload also has
to be unchanged.
"""

import argparse
import datetime
import hashlib
import json
import os
import platform
import re
import resource
import shutil
import sqlite3
import statistics
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TARGET = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
DATABASE = TARGET / "perf" / "results.sqlite"

HARNESS = Path("examples/perf/main.rs")
WORKLOADS = Path("examples/perf/workloads")
BUILD = ["--profile", "perf", "--example", "perf", "--locked", "--no-default-features"]

# A slowdown below the warning limit passes silently. Inlining decisions alone moved unrelated
# workloads by up to 2.7% in the measurements for ADR-0002, so failing starts above that.
WARN_PERCENT = 2.0
FAIL_PERCENT = 5.0

# The added words of `pad-check` must change a gated workload by less than this.
PAD_LIMIT_PERCENT = 1.0
PAD_WORDS = 50

# Counters that vary between runs are repeated, and the median is used.
REPEATS = 5

# How many of the most expensive functions to keep for each workload.
TOP_FUNCTIONS = 20

# How many runs a sparkline shows.
HISTORY_RUNS = 20

# The metric that decides on a slowdown, for each backend. Time is too noisy to decide on, so
# `cpu-time` shows a change but never gives a warning or a failure.
MAIN_METRIC = {"cachegrind": "ir", "macos-pmu": "instructions", "cpu-time": "user_seconds"}
DECIDES = {"cachegrind", "macos-pmu"}

SPARKS = "▁▂▃▄▅▆▇█"

SCHEMA_VERSION = 1

SCHEMA = """
CREATE TABLE schema_version (version INTEGER NOT NULL);

CREATE TABLE run (
    id           INTEGER PRIMARY KEY,
    started_at   TEXT NOT NULL,     -- UTC, ISO 8601
    git_sha      TEXT NOT NULL,
    git_ref      TEXT,
    git_dirty    INTEGER NOT NULL,  -- Uncommitted changes, so the sha doesn't describe it
    note         TEXT,              -- Free text, from --note
    kind         TEXT NOT NULL      -- A comparison isn't part of the development
        CHECK (kind IN ('run', 'compare-base', 'compare-head')),
    backend      TEXT NOT NULL CHECK (backend IN ('cachegrind', 'macos-pmu', 'cpu-time')),
    tool_version TEXT,
    rustc        TEXT NOT NULL,
    target       TEXT NOT NULL,
    machine      TEXT NOT NULL      -- Hash of the CPU model and OS, not the hostname
);

CREATE TABLE workload_run (
    id            INTEGER PRIMARY KEY,
    run_id        INTEGER NOT NULL REFERENCES run (id) ON DELETE CASCADE,
    workload      TEXT NOT NULL,
    category      TEXT NOT NULL,
    gated         INTEGER NOT NULL,
    source_sha256 TEXT NOT NULL,
    UNIQUE (run_id, workload)
);

CREATE TABLE measurement (
    workload_run_id INTEGER NOT NULL REFERENCES workload_run (id) ON DELETE CASCADE,
    metric          TEXT NOT NULL,  -- ir, bi, bim, instructions, cycles, user_seconds
    value           REAL NOT NULL,
    PRIMARY KEY (workload_run_id, metric)
);

-- The most expensive functions per workload, only with the cachegrind backend.
CREATE TABLE function_cost (
    workload_run_id INTEGER NOT NULL REFERENCES workload_run (id) ON DELETE CASCADE,
    function        TEXT NOT NULL,
    file            TEXT NOT NULL,
    ir              INTEGER NOT NULL,
    PRIMARY KEY (workload_run_id, function, file)
);

CREATE INDEX run_series ON run (backend, target, machine, rustc, started_at);

CREATE VIEW series AS
    SELECT r.id AS run_id, r.kind, r.started_at, r.git_sha, r.git_dirty, r.backend, r.target,
           r.machine, r.rustc, w.workload, w.category, w.gated, w.source_sha256, m.metric, m.value
    FROM measurement m
    JOIN workload_run w ON w.id = m.workload_run_id
    JOIN run r ON r.id = w.run_id;
"""


def command_output(*args, cwd=ROOT):
    """Runs a command, and returns what it printed. Stops the script if it fails."""
    done = subprocess.run(args, cwd=cwd, capture_output=True, text=True)
    if done.returncode != 0:
        sys.exit(f"{args[0]} failed: {done.stderr.strip()}")
    return done.stdout.strip()


# --- The figures ---


def parse_time_output(text):
    """Returns the instructions and cycles that macOS's `time -l` reports."""
    instructions = re.search(r"(\d+)\s+instructions retired", text)
    cycles = re.search(r"(\d+)\s+cycles elapsed", text)
    if instructions is None or cycles is None:
        sys.exit("/usr/bin/time -l reported no instruction count.")
    return {"instructions": int(instructions.group(1)), "cycles": int(cycles.group(1))}


def parse_cachegrind(text):
    """Returns the totals and the cost per function of a Cachegrind output file.

    The totals are a dict by event name. The functions are a dict from (function, file) to `Ir`.
    """
    events = []
    totals = None
    functions = {}
    file = function = None
    for line in text.splitlines():
        if line.startswith("events:"):
            events = line.split()[1:]
        elif line.startswith("fl="):
            file = line[3:]
        elif line.startswith("fn="):
            function = line[3:]
        elif line.startswith("summary:"):
            totals = dict(zip(events, map(int, line.split()[1:])))
        elif line[:1].isdigit() and function is not None:
            key = (function, file)
            functions[key] = functions.get(key, 0) + int(line.split()[1])
    if totals is None:
        sys.exit("The Cachegrind output has no summary.")
    return totals, functions


def top_functions(functions, count=TOP_FUNCTIONS):
    """Returns the `count` most expensive items of the functions of parse_cachegrind."""
    return sorted(functions.items(), key=lambda item: (-item[1], item[0]))[:count]


def median_of(rows):
    """Returns the median of each key in a list of dicts."""
    return {key: statistics.median(row[key] for row in rows) for key in rows[0]}


def failure_text(stderr):
    """Returns what a workload printed, without the counters that `time -l` adds after it."""
    return re.split(r"^\s*[\d.]+ real", stderr, flags=re.MULTILINE)[0].strip()


def check_finished(done, workload):
    """Stops the script if a workload failed, since its figures would mean nothing."""
    if done.returncode != 0:
        sys.exit(f"Workload {workload} failed:\n{failure_text(done.stderr)}")


def fixed_environment():
    """Returns the environment for a measured process. Its size changes the count a little."""
    return {"PATH": os.environ.get("PATH", "")}


def measure_cachegrind(binary, workload, extra):
    """Returns the metrics and the functions of one run under Cachegrind."""
    with tempfile.TemporaryDirectory() as directory:
        out_file = Path(directory) / "cachegrind.out"
        done = subprocess.run(
            [
                "valgrind",
                "--tool=cachegrind",
                "--cache-sim=no",
                "--branch-sim=yes",
                f"--cachegrind-out-file={out_file}",
                "--log-file=/dev/null",
                binary,
                workload,
                *extra,
            ],
            env=fixed_environment(),
            capture_output=True,
            text=True,
        )
        check_finished(done, workload)
        totals, functions = parse_cachegrind(out_file.read_text())
    return {"ir": totals["Ir"], "bi": totals["Bi"], "bim": totals["Bim"]}, functions


def measure_macos(binary, workload, extra):
    """Returns the median counters of a workload, from macOS's `time -l`."""
    rows = []
    for _ in range(REPEATS):
        done = subprocess.run(
            ["/usr/bin/time", "-l", binary, workload, *extra],
            env=fixed_environment(),
            capture_output=True,
            text=True,
        )
        check_finished(done, workload)
        rows.append(parse_time_output(done.stderr))
    return median_of(rows), {}


def measure_cpu_time(binary, workload, extra):
    """Returns the median user time of a workload."""
    rows = []
    for _ in range(REPEATS):
        before = resource.getrusage(resource.RUSAGE_CHILDREN).ru_utime
        done = subprocess.run(
            [binary, workload, *extra], env=fixed_environment(), capture_output=True, text=True
        )
        check_finished(done, workload)
        after = resource.getrusage(resource.RUSAGE_CHILDREN).ru_utime
        rows.append({"user_seconds": after - before})
    return median_of(rows), {}


MEASURE = {
    "cachegrind": measure_cachegrind,
    "macos-pmu": measure_macos,
    "cpu-time": measure_cpu_time,
}


def choose_backend():
    """Returns the best backend that this machine has."""
    if shutil.which("valgrind"):
        return "cachegrind"
    if sys.platform == "darwin" and Path("/usr/bin/time").exists():
        return "macos-pmu"
    return "cpu-time"


def tool_version(backend):
    """Returns the version of what does the counting, if it has one."""
    if backend == "cachegrind":
        return command_output("valgrind", "--version")
    return platform.mac_ver()[0] if backend == "macos-pmu" else None


def source_hash(tree, workload):
    """Hashes all that defines a workload, so that a changed one isn't compared."""
    digest = hashlib.sha256()
    for path in (HARNESS, WORKLOADS / "tower.fth", WORKLOADS / f"{workload}.fth"):
        digest.update((tree / path).read_bytes())
    return digest.hexdigest()


def measure_tree(tree, backend, pad_words=0):
    """Builds the harness in a source tree, and measures every workload in it."""
    print(f"Building {tree}", file=sys.stderr)
    env = dict(os.environ, CARGO_TARGET_DIR=str(TARGET))
    subprocess.run(["cargo", "build", "--quiet", *BUILD], cwd=tree, env=env, check=True)
    binary = str(TARGET / "perf" / "examples" / "perf")
    extra = ["--pad-words", str(pad_words)] if pad_words else []

    results = []
    workloads = json.loads(command_output(binary, "--list"))
    for workload in workloads:
        print(f"Measuring {workload['name']}", file=sys.stderr)
        metrics, functions = MEASURE[backend](binary, workload["name"], extra)
        results.append(
            {
                **workload,
                "source_sha256": source_hash(tree, workload["name"]),
                "metrics": metrics,
                "functions": top_functions(functions),
            }
        )
    return results


# --- The comparison ---


def percent_change(old, new):
    """Returns how much larger `new` is than `old`, in percent."""
    return 100 * (new / old - 1)


def status_of(change, gated, decides=True):
    """Says what a change in percent means for a workload."""
    if not gated or not decides or change < WARN_PERCENT:
        return "ok"
    return "fail" if change >= FAIL_PERCENT else "warn"


def compare(base, head, metric, decides=True):
    """Compares two lists of workload results, and returns a row for each workload.

    A workload only counts when it's on both sides and unchanged. A row has `name`, `base`,
    `head`, `change` and `status`, which can also be `new`, `changed` or `removed`.
    """
    old = {workload["name"]: workload for workload in base}
    rows = []
    for workload in head:
        name = workload["name"]
        new_value = workload["metrics"].get(metric)
        if name not in old:
            rows.append(row_of(name, None, new_value, None, "new"))
        elif old[name]["source_sha256"] != workload["source_sha256"]:
            rows.append(row_of(name, old[name]["metrics"].get(metric), new_value, None, "changed"))
        else:
            old_value = old[name]["metrics"][metric]
            change = percent_change(old_value, new_value)
            rows.append(row_of(name, old_value, new_value, change, status_of(change, workload["gated"], decides)))
    names = {workload["name"] for workload in head}
    for name, workload in old.items():
        if name not in names:
            rows.append(row_of(name, workload["metrics"].get(metric), None, None, "removed"))
    return rows


def row_of(name, base, head, change, status):
    """Returns one row of a comparison."""
    return {"name": name, "base": base, "head": head, "change": change, "status": status}


def sparkline(values):
    """Returns a small chart of a list of numbers."""
    low, high = min(values), max(values)
    if low == high:
        return SPARKS[0] * len(values)
    scale = (len(SPARKS) - 1) / (high - low)
    return "".join(SPARKS[round((value - low) * scale)] for value in values)


def format_value(value):
    """Formats a metric, in millions when it's a count."""
    if value is None:
        return "-"
    return f"{value / 1e6:.2f} M" if value >= 1000 else f"{value:.4f}"


def print_table(rows, developments=None):
    """Prints a comparison, with a sparkline by workload name if there are any."""
    history = "History" if developments else ""
    print(f"{'Workload':<15}{'Base':>13}{'Head':>13}{'Change':>10}  {'Status':<9}{history}".rstrip())
    for row in rows:
        change = "-" if row["change"] is None else f"{row['change']:+.2f}%"
        chart = developments.get(row["name"], "") if developments else ""
        base, head = format_value(row["base"]), format_value(row["head"])
        line = f"{row['name']:<15}{base:>13}{head:>13}{change:>10}  {row['status']:<9}{chart}"
        print(line.rstrip())


# --- The database ---


def open_database(path=None):
    """Opens the database, and creates it when it doesn't exist yet."""
    path = Path(path or DATABASE)
    if str(path) != ":memory:":
        path.parent.mkdir(parents=True, exist_ok=True)
    database = sqlite3.connect(str(path))
    database.execute("PRAGMA foreign_keys = ON")
    known = database.execute(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'schema_version'"
    ).fetchone()
    if known is None:
        database.executescript(SCHEMA)
        database.execute("INSERT INTO schema_version VALUES (?)", (SCHEMA_VERSION,))
        database.commit()
    else:
        version = database.execute("SELECT version FROM schema_version").fetchone()[0]
        if version != SCHEMA_VERSION:
            sys.exit(f"{path} has schema {version}, not {SCHEMA_VERSION}. Delete it to start over.")
    return database


def store_run(database, info, results, kind="run"):
    """Stores a run and its results, and returns the id of the run."""
    run_id = database.execute(
        "INSERT INTO run (started_at, git_sha, git_ref, git_dirty, note, kind, backend,"
        " tool_version, rustc, target, machine) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        (
            info["started_at"],
            info["git_sha"],
            info["git_ref"],
            info["git_dirty"],
            info["note"],
            kind,
            info["backend"],
            info["tool_version"],
            info["rustc"],
            info["target"],
            info["machine"],
        ),
    ).lastrowid
    for result in results:
        workload_id = database.execute(
            "INSERT INTO workload_run (run_id, workload, category, gated, source_sha256)"
            " VALUES (?, ?, ?, ?, ?)",
            (run_id, result["name"], result["category"], result["gated"], result["source_sha256"]),
        ).lastrowid
        database.executemany(
            "INSERT INTO measurement VALUES (?, ?, ?)",
            [(workload_id, metric, value) for metric, value in result["metrics"].items()],
        )
        database.executemany(
            "INSERT INTO function_cost VALUES (?, ?, ?, ?)",
            [(workload_id, key[0], key[1], ir) for key, ir in result["functions"]],
        )
    database.commit()
    return run_id


def load_results(database, run_id):
    """Returns a stored run as workload results, as measure_tree gives them."""
    results = {}
    for name, category, gated, source_sha256, metric, value in database.execute(
        "SELECT workload, category, gated, source_sha256, metric, value FROM series"
        " WHERE run_id = ? ORDER BY workload",
        (run_id,),
    ):
        result = results.setdefault(
            name,
            {
                "name": name,
                "category": category,
                "gated": bool(gated),
                "source_sha256": source_sha256,
                "metrics": {},
            },
        )
        result["metrics"][metric] = value
    return list(results.values())


def previous_run(database, run_id):
    """Returns the id of the last run before `run_id` that it can be compared with."""
    found = database.execute(
        "SELECT older.id FROM run newer JOIN run older ON older.backend = newer.backend"
        " AND older.target = newer.target AND older.machine = newer.machine"
        " AND older.rustc = newer.rustc AND older.id < newer.id AND older.kind = 'run'"
        " WHERE newer.id = ? ORDER BY older.id DESC LIMIT 1",
        (run_id,),
    ).fetchone()
    return found[0] if found else None


def latest_run(database):
    """Returns the id of the newest run, if there is one."""
    found = database.execute(
        "SELECT id FROM run WHERE kind = 'run' ORDER BY id DESC LIMIT 1"
    ).fetchone()
    return found[0] if found else None


def history(database, run_id, metric, limit):
    """Returns, by workload, the comparable values up to `run_id`, oldest first.

    Only the unbroken stretch with the current source of each workload counts.
    """
    newest = database.execute(
        "SELECT backend, target, machine, rustc FROM run WHERE id = ?", (run_id,)
    ).fetchone()
    series = {}
    for workload, value, source_sha256 in database.execute(
        "SELECT workload, value, source_sha256 FROM series WHERE backend = ? AND target = ?"
        " AND machine = ? AND rustc = ? AND metric = ? AND run_id <= ? AND kind = 'run'"
        " ORDER BY run_id",
        (*newest, metric, run_id),
    ):
        series.setdefault(workload, []).append((value, source_sha256))
    trimmed = {}
    for workload, points in series.items():
        latest_source = points[-1][1]
        stretch = []
        for value, source_sha256 in reversed(points):
            if source_sha256 != latest_source:
                break
            stretch.append(value)
        trimmed[workload] = stretch[:limit][::-1]
    return trimmed


def backend_of(database, run_id):
    """Returns the backend of a stored run."""
    return database.execute("SELECT backend FROM run WHERE id = ?", (run_id,)).fetchone()[0]


# --- The commands ---


def run_info(tree, backend, note):
    """Collects what identifies a run of a source tree."""
    version = command_output("rustc", "-vV")
    host = re.search(r"^host: (.+)$", version, re.MULTILINE).group(1)
    cpu = platform.processor()
    if sys.platform == "darwin":
        cpu = command_output("sysctl", "-n", "machdep.cpu.brand_string")
    elif Path("/proc/cpuinfo").exists():
        models = re.findall(r"^model name\s*: (.+)$", Path("/proc/cpuinfo").read_text(), re.MULTILINE)
        cpu = models[0] if models else cpu
    machine = hashlib.sha256(f"{cpu}|{platform.system()}|{platform.machine()}".encode()).hexdigest()
    return {
        "started_at": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "git_sha": command_output("git", "rev-parse", "HEAD", cwd=tree),
        "git_ref": command_output("git", "rev-parse", "--abbrev-ref", "HEAD", cwd=tree),
        "git_dirty": int(bool(command_output("git", "status", "--porcelain", cwd=tree))),
        "note": note,
        "backend": backend,
        "tool_version": tool_version(backend),
        "rustc": version.splitlines()[0],
        "target": host,
        "machine": machine[:12],
    }


def worst_exit_code(rows):
    """Returns what the script should exit with after a comparison."""
    return 1 if any(row["status"] == "fail" for row in rows) else 0


def compare_runs(database, base_id, run_id, metric):
    """Compares two stored runs."""
    base, head = load_results(database, base_id), load_results(database, run_id)
    return compare(base, head, metric, backend_of(database, run_id) in DECIDES)


def developments(database, run_id, metric):
    """Returns a sparkline by workload, up to `run_id`."""
    series = history(database, run_id, metric, HISTORY_RUNS)
    return {name: sparkline(values) for name, values in series.items()}


def command_run(args):
    """Measures the working tree, stores the result, and compares it with the previous run."""
    backend = args.backend or choose_backend()
    results = measure_tree(ROOT, backend)
    database = open_database()
    run_id = store_run(database, run_info(ROOT, backend, args.note), results)
    metric = MAIN_METRIC[backend]
    print(f"Backend {backend}, stored as run {run_id} in {DATABASE}")
    previous = previous_run(database, run_id)
    if previous is None:
        print("No earlier run to compare with, on this machine with this compiler.")
        for result in results:
            print(f"{result['name']:<15}{format_value(result['metrics'][metric]):>13}")
        return 0
    rows = compare_runs(database, previous, run_id, metric)
    print_table(rows, developments(database, run_id, metric))
    return worst_exit_code(rows)


def command_report(args):
    """Prints the stored development of each workload, without measuring."""
    database = open_database()
    run_id = latest_run(database)
    if run_id is None:
        print("No runs yet. Run make perf first.")
        return 0
    backend = backend_of(database, run_id)
    metric = MAIN_METRIC[backend]
    series = history(database, run_id, metric, args.last)
    print(f"Run {run_id}, backend {backend}, metric {metric}, oldest value to newest:")
    print(f"{'Workload':<15}{'Runs':>5}{'First':>13}{'Last':>13}{'Change':>10}  History")
    for name, values in series.items():
        change = percent_change(values[0], values[-1])
        print(
            f"{name:<15}{len(values):>5}{format_value(values[0]):>13}"
            f"{format_value(values[-1]):>13}{change:>+9.2f}%  {sparkline(values)}"
        )
    return 0


def command_compare(args):
    """Measures another commit in a temporary worktree too, and compares it with this tree."""
    backend = args.backend or choose_backend()
    sha = command_output("git", "rev-parse", "--verify", f"{args.base}^{{commit}}")
    with tempfile.TemporaryDirectory() as directory:
        checkout = Path(directory) / "base"
        command_output("git", "worktree", "add", "--quiet", "--detach", str(checkout), sha)
        try:
            if not (checkout / HARNESS).exists():
                print(f"{args.base} has no workloads, so there's nothing to compare with.")
                return 0
            base_results = measure_tree(checkout, backend)
            base_info = run_info(checkout, backend, f"Base of a comparison with {args.base}")
        finally:
            command_output("git", "worktree", "remove", "--force", str(checkout))
    head_results = measure_tree(ROOT, backend)
    database = open_database()
    base_id = store_run(database, base_info, base_results, "compare-base")
    head_info = run_info(ROOT, backend, args.note)
    run_id = store_run(database, head_info, head_results, "compare-head")
    print(f"{args.base} against the working tree, backend {backend}")
    rows = compare_runs(database, base_id, run_id, MAIN_METRIC[backend])
    print_table(rows)
    return worst_exit_code(rows)


def command_pad_check(args):
    """Shows that gated workloads don't depend on the size of the dictionary."""
    backend = args.backend or choose_backend()
    metric = MAIN_METRIC[backend]
    plain = measure_tree(ROOT, backend)
    padded = measure_tree(ROOT, backend, PAD_WORDS)
    print(f"{PAD_WORDS} extra words, backend {backend}, limit {PAD_LIMIT_PERCENT}% when gated")
    failed = False
    for before, after in zip(plain, padded):
        change = percent_change(before["metrics"][metric], after["metrics"][metric])
        too_much = before["gated"] and abs(change) >= PAD_LIMIT_PERCENT
        failed = failed or too_much
        gated = "gated" if before["gated"] else "reported"
        print(f"{before['name']:<15}{change:>+9.2f}%  {gated}{'  TOO MUCH' if too_much else ''}")
    return 1 if failed else 0


def main():
    """Parses the command line, and runs the command."""
    common = argparse.ArgumentParser(add_help=False)
    common.add_argument("--backend", choices=sorted(MEASURE), help="default: the best one here")

    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    commands = parser.add_subparsers(dest="command", required=True)

    run = commands.add_parser(
        "run", parents=[common], help="measure the working tree, and store the result"
    )
    run.add_argument("--note", help="free text to store with the run")
    run.set_defaults(handler=command_run)

    report = commands.add_parser("report", help="show the stored development")
    report.add_argument("--last", type=int, default=HISTORY_RUNS, help="how many runs to show")
    report.set_defaults(handler=command_report)

    against = commands.add_parser(
        "compare", parents=[common], help="measure another commit too"
    )
    against.add_argument("base", help="the commit to compare with")
    against.add_argument("--note", help="free text to store with the run")
    against.set_defaults(handler=command_compare)

    pad = commands.add_parser(
        "pad-check", parents=[common], help="check that gated workloads ignore dictionary size"
    )
    pad.set_defaults(handler=command_pad_check)

    args = parser.parse_args()
    sys.exit(args.handler(args))


if __name__ == "__main__":
    main()
