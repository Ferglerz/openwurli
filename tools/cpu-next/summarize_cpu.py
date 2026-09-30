#!/usr/bin/env python3
"""Aggregate exactly three cpu-next runs without building or measuring anything.

Usage: summarize_cpu.py RUN1 RUN2 RUN3 --output NEW_DIR [--quiet-metadata FILE]
Quiet metadata is an independently supplied JSON object with schema=1 and runs:
[{report_sha256, provenance_sha256, independent: true, status: "passed",
  reviewer: "...", evidence: "..."}, ...]. It must attest all three exact inputs.
This script validates the attestations' presence and binding, not their truth.
No attestation is inferred from benchmark times, run timestamps, or CPU usage.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path


METRICS = ("render_ns_per_sample", "callback_ns_per_sample")
TARGET_PATTERNS = {"hold", "repeated_chord", "pedal_bursts", "depth_switch", "lifecycle"}


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def require(condition, message):
    if not condition:
        raise ValueError(message)


def number(value, label):
    require(isinstance(value, (float, int)) and not isinstance(value, bool)
            and math.isfinite(value) and value > 0, f"Invalid positive timing: {label}")
    return float(value)


def median(values):
    # Match the runner's nearest-rank implementation, including even repeat counts.
    return sorted(values)[math.ceil((len(values) - 1) * 0.5)]


def load_run(directory):
    report_path, provenance_path = directory / "report.json", directory / "provenance.json"
    report = json.loads(report_path.read_text())
    provenance = json.loads(provenance_path.read_text())
    require(report.get("schema") == 2, "Unsupported report schema")
    require(report.get("repeats", 0) >= 11, "At least 11 repeats per run required")
    require(provenance.get("source_unchanged_during_run") is True, "Source changed or not verified")
    require(provenance.get("exit_code") == 0, "Benchmark did not exit successfully")
    require(report.get("cpu_finite_guards_clear") is True, "CPU finite/guard checks failed")
    for key in ("root_cargo_lock_sha256", "harness_cargo_lock_sha256"):
        require(provenance.get(key) and provenance[key] == provenance.get(key + "_after"),
                f"Lock unchanged check failed: {key}")
    sources = provenance.get("source_sha256")
    require(isinstance(sources, dict) and sources, "Missing source hashes")
    require(all(isinstance(v, str) and len(v) == 64 and all(c in "0123456789abcdef" for c in v)
                for v in sources.values()), "Invalid source hash")
    for prefix in ("crates/openwurli-dsp/", "tools/cpu-next/src/", "tools/cpu-next/reference/"):
        require(any(k.startswith(prefix) for k in sources), f"Missing source scope: {prefix}")
    for key in ("candidate_features", "candidate_runtime_modes", "reference_runtime_modes", "candidate_initial_mode"):
        require(key in report and key in provenance and report[key] == provenance[key],
                f"Report/provenance disagreement or missing: {key}")
    require(provenance["engine"] in ("fast", "heavy") and
            report["engine"].startswith(provenance["engine"] + ":") and
            report["candidate_initial_mode"] == provenance["engine"], "Engine mismatch")
    require(provenance["reference_runtime_modes"] is False, "Reference must use fixed backend")
    require(isinstance(provenance["candidate_runtime_modes"], bool), "Missing runtime-mode boolean")
    cases = {}
    for case in report.get("cpu", []):
        name = case["scenario"]["name"]
        require(name not in cases, f"Duplicate CPU case: {name}")
        require(case["order"] == [[0, 1] if n % 2 == 0 else [1, 0]
                                   for n in range(report["repeats"])], f"Pair order mismatch: {name}")
        variants = {v["variant"]: v for v in case["variants"]}
        require(set(variants) == {"reference", "candidate"} and len(case["variants"]) == 2,
                f"Expected two variants: {name}")
        per_variant = {}
        for variant, data in variants.items():
            raw = data["raw"]
            require(len(raw) == report["repeats"] and
                    [r["repeat"] for r in raw] == list(range(report["repeats"])),
                    f"Raw repeats incomplete: {name}/{variant}")
            require(all(r["nan_guards"] == 0 and r["nonfinite_samples"] == 0 for r in raw),
                    f"Raw finite/guard failure: {name}/{variant}")
            medians = {}
            for metric in METRICS:
                values = [number(r[metric], name + "/" + metric) for r in raw]
                total_key = metric.removesuffix("_per_sample")
                require(all(math.isclose(value, number(r[total_key], name + "/" + total_key) /
                                         number(r["samples"], name + "/samples"), rel_tol=1e-12)
                            for value, r in zip(values, raw)), f"Raw timing totals disagree: {name}/{metric}")
                medians[metric] = median(values)
                require(math.isclose(medians[metric], data[metric]["median"], rel_tol=1e-12),
                        f"Stored median mismatch: {name}/{variant}/{metric}")
            per_variant[variant] = {
                "medians": medians,
                "event_ns_per_event": data.get("event_ns_per_event"),
                "per_repeat_callback_p99_us": data.get("per_repeat_callback_p99_us"),
                "raw_repeat_statistics": [{k: v for k, v in r.items() if k != "callbacks"} for r in raw],
                "raw_callbacks_location": f"report.json:cpu[{name}].variants[{variant}].raw[].callbacks",
            }
        require([r["samples"] for r in variants["reference"]["raw"]] ==
                [r["samples"] for r in variants["candidate"]["raw"]], f"Sample counts differ: {name}")
        reductions = {m: 100 * (per_variant["reference"]["medians"][m] -
                                per_variant["candidate"]["medians"][m]) /
                                per_variant["reference"]["medians"][m] for m in METRICS}
        cases[name] = {"scenario": case["scenario"], "events": case["events"],
                       "reductions_percent": reductions, "variants": per_variant}
    require(cases and len(cases) == report.get("cpu_scenarios"), "No CPU cases or count mismatch")
    audio_count = report.get("audio_scenarios", 0)
    require(audio_count == len(report.get("audio", [])), "Audio count mismatch")
    if audio_count:
        require(report.get("audio_verified") is True, "Present audio marked unverified")
        audio = {"status": "recorded", "scenarios": audio_count,
                 **{k: report.get(k) for k in ("all_audio_exact", "all_audio_within_bounds",
                     "audio_finite_guards_clear", "power_amp_diagnostics_exact",
                     "power_amp_no_counter_increase", "power_amp_diagnostics_review_required")}}
    else:
        require(report.get("audio_verified") is False, "CPU-only run claims audio verification")
        require(report.get("all_audio_exact") is None and report.get("all_audio_within_bounds") is None,
                "CPU-only audio outcomes must be null")
        audio = {"status": "not_evaluated_cpu_only", "scenarios": 0}
    compatibility_keys = ("engine", "candidate_features", "candidate_runtime_modes", "reference_runtime_modes",
                          "candidate_initial_mode", "source_sha256", "reference_manifest", "rustc", "architecture",
                          "platform", "build_environment", "root_cargo_lock_sha256", "harness_cargo_lock_sha256")
    require(all(k in provenance for k in compatibility_keys), "Incomplete compatibility provenance")
    compatibility = {k: provenance[k] for k in compatibility_keys}
    compatibility.update({k: provenance.get(k) for k in ("hardware", "os_build", "seed_protocol")})
    return {"directory": str(directory), "report_sha256": digest(report_path),
            "provenance_sha256": digest(provenance_path), "repeats": report["repeats"],
            "started_utc": provenance.get("started_utc"), "finished_utc": provenance.get("finished_utc"),
            "head": provenance.get("head"), "audio": audio, "cases": cases,
            "compatibility": compatibility}


def quiet_status(path, runs):
    if path is None:
        return {"status": "pending", "reason": "No independent quiet-period metadata supplied."}
    metadata = json.loads(path.read_text())
    require(metadata.get("schema") == 1 and isinstance(metadata.get("runs"), list), "Invalid quiet metadata schema")
    attestations = metadata["runs"]
    for run in runs:
        matches = [a for a in attestations if a.get("report_sha256") == run["report_sha256"] and
                   a.get("provenance_sha256") == run["provenance_sha256"]]
        require(len(matches) == 1, "Quiet metadata must bind each exact run once")
        a = matches[0]
        require(a.get("independent") is True and a.get("status") == "passed" and
                all(isinstance(a.get(k), str) and a[k].strip() for k in ("reviewer", "evidence")),
                "Quiet qualification requires independent passed attestation, reviewer, and evidence")
    return {"status": "passed_by_independent_attestation", "metadata_path": str(path.resolve()),
            "metadata_sha256": digest(path), "attestations": attestations,
            "limitation": "Attestation binding is validated; quiet conditions are not inferred or independently measured by this script."}


def summarize(runs, quiet):
    for run in runs[1:]:
        require(run["compatibility"] == runs[0]["compatibility"], "Incompatible engine/features/runtime/source/toolchain/build provenance")
        require(run["cases"].keys() == runs[0]["cases"].keys(), "CPU case sets differ")
    cases = []
    for name, base in runs[0]["cases"].items():
        require(all(run["cases"][name]["scenario"] == base["scenario"] and
                    run["cases"][name]["events"] == base["events"] for run in runs), f"Case definition differs: {name}")
        scenario = base["scenario"]
        target = len(scenario["notes"]) in (1, 6, 12) and scenario["pattern"] in TARGET_PATTERNS
        reductions = {m: [run["cases"][name]["reductions_percent"][m] for run in runs] for m in METRICS}
        gains = {m: target and all(v >= 3.0 or math.isclose(v, 3.0, abs_tol=1e-10, rel_tol=0)
                                  for v in values) for m, values in reductions.items()}
        regressions = {m: all(v < -2.0 and not math.isclose(v, -2.0, abs_tol=1e-10, rel_tol=0)
                             for v in values) for m, values in reductions.items()}
        cases.append({"name": name, "scenario": scenario, "target_1_6_12_note_musical_case": target,
                      "reductions_percent_by_run": reductions, "gain_at_least_3_percent_each_run": gains,
                      "reproducible_regression_over_2_percent_each_run": regressions})
    gains = [c["name"] for c in cases if all(c["gain_at_least_3_percent_each_run"].values())]
    regressions = [c["name"] for c in cases if any(c["reproducible_regression_over_2_percent_each_run"].values())]
    measured = bool(gains) and not regressions
    return {"schema": 1, "compatibility_verified": True, "quiet_qualification": quiet,
            "timing_qualification": "pending" if quiet["status"] == "pending" else ("passed" if measured else "failed"),
            "measured_gain_and_regression_gate_passes": measured,
            "qualifying_target_cases": gains, "reproducible_regression_cases": regressions,
            "gate_definition": "Both render and callback median time must improve >=3% on the same 1/6/12-note musical case in each of three runs. Any matching case with >2% regression in either metric in all three runs blocks qualification. Event and callback p99 statistics are descriptive, not gates. Quiet attestation is additionally required.",
            "audio_qualification": "separate; CPU-only null outcomes do not demonstrate audio equivalence",
            "cases": cases, "runs": runs}


def markdown(result):
    lines = ["# Three-run CPU summary", "", f"Timing qualification: **{result['timing_qualification']}**.", "",
             result["gate_definition"], "", "Audio qualification is separate; CPU-only runs provide no audio proof.", "",
             "Event cost and callback p99 distributions, per-repeat raw statistics, input hashes, and source provenance are retained in summary.json. Full callback records remain in each input report.", "",
             "Quiet status: " + result["quiet_qualification"]["status"] + ".", "",
             "## Per-case median reductions", "", "Positive values mean less CPU time. Each triple is run 1 / run 2 / run 3.", ""]
    for case in result["cases"]:
        r = case["reductions_percent_by_run"]
        render = " / ".join(f"{v:+.3f}%" for v in r[METRICS[0]])
        callback = " / ".join(f"{v:+.3f}%" for v in r[METRICS[1]])
        flags = []
        if case["name"] in result["qualifying_target_cases"]: flags.append("measured target gain")
        if case["name"] in result["reproducible_regression_cases"]: flags.append("reproducible regression")
        lines.append(f"- `{case['name']}`: render {render}; callback {callback}" + ("; " + ", ".join(flags) if flags else "") + ".")
    lines += ["", "## Inputs", ""]
    for i, run in enumerate(result["runs"], 1):
        lines.append(f"- Run {i}: `{run['directory']}`; {run['repeats']} repeats; audio: {run['audio']['status']}; report SHA-256 `{run['report_sha256']}`.")
    return "\n".join(lines) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("runs", nargs=3, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--quiet-metadata", type=Path)
    args = parser.parse_args()
    try:
        directories = [p.resolve() for p in args.runs]
        require(len(set(directories)) == 3, "Three distinct run directories required")
        require(not args.output.exists() or not any(args.output.iterdir()), "Output directory must be empty")
        runs = [load_run(p) for p in directories]
        require(len({r["report_sha256"] for r in runs}) == 3, "Duplicated report content is not three independent runs")
        result = summarize(runs, quiet_status(args.quiet_metadata, runs))
        result["summarizer_sha256"] = digest(Path(__file__))
        args.output.mkdir(parents=True, exist_ok=True)
        (args.output / "summary.json").write_text(json.dumps(result, indent=2, allow_nan=False) + "\n")
        (args.output / "SUMMARY.md").write_text(markdown(result))
        print(json.dumps({k: result[k] for k in ("timing_qualification", "qualifying_target_cases", "reproducible_regression_cases")}))
    except (ValueError, KeyError, TypeError, OSError) as error:
        parser.exit(2, f"Invalid evidence: {error}\n")


if __name__ == "__main__":
    main()
