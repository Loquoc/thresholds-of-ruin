"""Independent arithmetic fixtures for offline Actions cost accounting."""
import copy
import json
import subprocess
import sys
from pathlib import Path
import tempfile
import unittest

import ci_cost_report as cost


def job(identifier, name="Rust (ubuntu-latest)", start="00:00:10", end="00:01:10", **extra):
    return {"id": identifier, "name": name, "started_at": f"2026-01-01T{start}Z" if start else None,
            "completed_at": f"2026-01-01T{end}Z" if end else None,
            "conclusion": "success", "run_attempt": 1, **extra}


def bundle(jobs, **extra):
    return {"runs": [{"run": {"id": 42, "created_at": "2026-01-01T00:00:00Z",
                              "updated_at": "2026-01-01T00:05:00Z", "status": "completed",
                              "run_attempt": 2}, "jobs": jobs, "total_count": len(jobs),
                      "jobs_scope": "all", **extra}]}


class CostReport(unittest.TestCase):
    def test_documented_input_example_is_an_explicit_lower_bound(self):
        guide = Path(cost.__file__).resolve().parents[1] / "docs/development/ci-cost-baseline.md"
        example = json.loads(guide.read_text(encoding="utf-8").split("## Reusable offline measurement", 1)[1]
                             .split("```json\n", 1)[1].split("```", 1)[0])
        result = cost.report([example])
        self.assertEqual(result["runner_seconds"], 0)
        self.assertTrue(result["is_lower_bound"])
        self.assertEqual(result["runs"][0]["issues"], ["job_pages_incomplete"])
        self.assertEqual(result["runs"][0]["github_terminal_span_seconds"], 300)

    def test_verified_empty_run_still_requires_workflow_chronology(self):
        source = bundle([])
        self.assertFalse(cost.report([source])["is_lower_bound"])
        del source["runs"][0]["run"]["updated_at"]
        with self.assertRaisesRegex(ValueError, "workflow updated_at"):
            cost.report([source])

    def progress_snapshots(self):
        old = bundle([job(1, end=None, conclusion=None)])
        old["runs"][0]["run"].update(updated_at="2026-01-01T00:00:20Z",
                                      status="in_progress", conclusion=None)
        new = bundle([job(1)])
        new["runs"][0]["run"].update(updated_at="2026-01-01T00:02:00Z", conclusion="success")
        return old, new

    def test_workflow_progress_uses_latest_snapshot_in_either_order(self):
        old, new = self.progress_snapshots()
        new["runs"][0]["run"]["updated_at"] = "2025-12-31T19:02:00-05:00"
        forward, reverse = cost.report([old, new]), cost.report([new, old])
        self.assertEqual(forward, reverse)
        row = forward["runs"][0]
        self.assertEqual((row["runner_seconds"], row["conclusion"], row["github_terminal_span_seconds"]),
                         (60, "success", 120))
        self.assertEqual(row["issues"], [])
        self.assertFalse(forward["is_lower_bound"])

    def test_lower_completed_attempt_never_supersedes_highest_in_progress(self):
        old, new = self.progress_snapshots()
        new["runs"][0]["run"]["run_attempt"] = 1
        for inputs in ([old, new], [new, old]):
            row = cost.report(inputs)["runs"][0]
            self.assertEqual((row["attempts"], row["conclusion"], row["github_terminal_span_seconds"]),
                             (2, None, 20))
            self.assertEqual(row["runner_seconds"], 60)
            self.assertEqual(row["issues"], ["workflow_not_completed"])

    def test_missing_attempt_defaults_to_one_and_lower_chronology_is_irrelevant(self):
        _, source = self.progress_snapshots()
        del source["runs"][0]["run"]["run_attempt"]
        self.assertEqual(cost.report([source])["runs"][0]["attempts"], 1)
        later = copy.deepcopy(source)
        later["runs"][0]["run"]["run_attempt"] = 2
        source["runs"][0]["run"]["updated_at"] = None
        for inputs in ([source, later], [later, source]):
            self.assertEqual(cost.report(inputs)["runs"][0]["attempts"], 2)

    def test_equal_timestamp_timezones_and_missing_metadata_agree(self):
        _, first = self.progress_snapshots()
        second = copy.deepcopy(first)
        second["runs"][0]["run"].update(updated_at="2025-12-31T19:02:00-05:00",
                                         created_at="2025-12-31T19:00:00-05:00",
                                         name="CI", html_url="https://example.test/run/42")
        before = copy.deepcopy([first, second])
        forward, reverse = cost.report([first, second]), cost.report([second, first])
        self.assertEqual(forward, reverse)
        self.assertEqual(forward["runs"][0]["workflow"], "CI")
        self.assertEqual(forward["runs"][0]["github_terminal_span_seconds"], 120)
        self.assertEqual([first, second], before, "Reconciliation must not mutate input snapshots")

    def test_equal_timestamp_conflicting_workflow_fields_are_rejected(self):
        _, first = self.progress_snapshots()
        first["runs"][0]["run"].update(name="CI", event="push", html_url="https://example.test/run/42")
        for field, value in (("status", "in_progress"), ("conclusion", "failure"), ("name", "other"),
                             ("event", "pull_request"), ("html_url", "https://example.test/other"),
                             ("created_at", "2026-01-01T00:00:01Z")):
            second = copy.deepcopy(first)
            second["runs"][0]["run"][field] = value
            for inputs in ([first, second], [second, first]):
                with self.subTest(field=field), self.assertRaisesRegex(ValueError, "conflicting workflow snapshots"):
                    cost.report(inputs)

    def test_unknown_workflow_chronology_and_invalid_attempts_are_rejected(self):
        old, new = self.progress_snapshots()
        for value in (None, "invalid", "2026-01-01T00:00:20"):
            old["runs"][0]["run"]["updated_at"] = value
            for inputs in ([old, new], [new, old], [old]):
                with self.subTest(value=value), self.assertRaisesRegex(ValueError, "workflow updated_at"):
                    cost.report(inputs)
        for value in (None, True, 0, -1, "2"):
            source = copy.deepcopy(new)
            source["runs"][0]["run"]["run_attempt"] = value
            with self.subTest(attempt=value), self.assertRaisesRegex(ValueError, "workflow run_attempt"):
                cost.report([source])

    def test_actual_cli_progress_order_and_ambiguity_without_writes(self):
        old, new = self.progress_snapshots()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            paths = [root / "old.json", root / "new.json"]
            for path, value in zip(paths, (old, new)):
                path.write_text(json.dumps(value), encoding="utf-8")
            output, human = root / "out.json", root / "out.txt"
            results = []
            for order in (paths, list(reversed(paths))):
                command = [sys.executable, cost.__file__, "--input", str(order[0]), "--input", str(order[1]),
                           "--json-output", str(output), "--text-output", str(human)]
                run = subprocess.run(command, capture_output=True, text=True)
                self.assertEqual(run.returncode, 0, run.stderr)
                results.append((json.loads(output.read_text()), human.read_text()))
            self.assertEqual(results[0], results[1])
            self.assertEqual(results[0][0]["runner_seconds"], 60)
            self.assertIn("Runner-hours: 0.0167", results[0][1])
            self.assertNotIn("workflow_not_completed", results[0][1])
            original = output.read_bytes(), human.read_bytes()
            old["runs"][0]["run"]["updated_at"] = None
            paths[0].write_text(json.dumps(old), encoding="utf-8")
            run = subprocess.run(command, capture_output=True, text=True)
            self.assertEqual(run.returncode, 2)
            self.assertIn("workflow updated_at", run.stderr)
            self.assertEqual((output.read_bytes(), human.read_bytes()), original)

    def test_parallel_runner_time_is_not_wall_span_or_billing(self):
        result = cost.report([bundle([job(1), job(2, name="Rust (windows-latest)", end="00:02:10")])])
        row = result["runs"][0]
        self.assertEqual(result["runner_seconds"], 180)
        self.assertEqual(result["runner_hours"], 0.05)
        self.assertEqual(row["execution_span_seconds"], 120)
        self.assertEqual(row["created_to_execution_end_seconds"], 130)
        self.assertEqual(row["first_job_delay_seconds"], 10)
        self.assertEqual(row["github_terminal_span_seconds"], 300)
        self.assertEqual(row["os_runner_seconds"], {"linux": 60, "windows": 120, "macos": 0, "unknown": 0})
        self.assertFalse(result["is_lower_bound"])

    def test_new_ids_carried_forward_are_not_new_execution(self):
        result = cost.report([bundle([job(1), job(2, run_attempt=2),
                                     job(3, start="00:03:00", end="00:04:00", run_attempt=2)])])
        self.assertEqual(result["runner_seconds"], 120)
        self.assertEqual(result["runs"][0]["duplicate_records_removed"], 1)
        self.assertEqual(result["runs"][0]["created_to_execution_end_seconds"], 240)

    def test_failed_and_canceled_executions_count(self):
        result = cost.report([bundle([job(1, conclusion="failure"),
                                     job(2, start="00:02:00", end="00:02:30", conclusion="cancelled")])])
        self.assertEqual(result["runner_seconds"], 90)

    def test_skipped_bad_interval_is_zero_not_negative(self):
        result = cost.report([bundle([job(1, start="00:02:00", end="00:01:00", conclusion="skipped")])])
        self.assertEqual(result["runner_seconds"], 0)
        self.assertFalse(result["is_lower_bound"])
        self.assertIsNone(result["runs"][0]["execution_span_seconds"])

    def test_null_end_is_lower_bound_even_if_workflow_says_complete(self):
        result = cost.report([bundle([job(1), job(2, end=None, conclusion=None)])])
        self.assertEqual(result["runner_seconds"], 60)
        self.assertTrue(result["is_lower_bound"])
        self.assertEqual(result["runs"][0]["incomplete_jobs"], 1)
        self.assertIn("at least", cost.human_readable(result))

    def test_invalid_dates_and_negative_executed_intervals_are_incomplete(self):
        for change in ({"started_at": "not a date"}, {"started_at": "2026-01-01T00:00:00"},
                       {"completed_at": "2025-01-01T00:00:00Z"}):
            with self.subTest(change=change):
                result = cost.report([bundle([job(1, **change)])])
                self.assertTrue(result["is_lower_bound"])
                self.assertEqual(result["runner_seconds"], 0)

    def test_missing_pages_or_attempt_declaration_is_lower_bound(self):
        for extra in ({"total_count": 2}, {"jobs_scope": "latest"}):
            result = cost.report([bundle([job(1)], **extra)])
            self.assertTrue(result["is_lower_bound"])

    def test_overlapping_input_pages_deduplicate_but_distinct_runs_do_not(self):
        first = bundle([job(1)])
        second = bundle([job(1)])
        self.assertEqual(cost.report([first, second])["runner_seconds"], 60)
        second["runs"][0]["run"]["id"] = 43
        self.assertEqual(cost.report([first, second])["runner_seconds"], 120)

    def test_labels_classify_without_guessing_unlabeled_gate(self):
        result = cost.report([bundle([job(1, name="gate", labels=["macOS"]), job(2, name="other")])])
        self.assertEqual(result["runs"][0]["os_runner_seconds"]["macos"], 60)
        self.assertEqual(result["runs"][0]["os_runner_seconds"]["unknown"], 60)

    def test_summary_is_not_double_charged_and_only_projects_public_fields(self):
        result = cost.report([bundle([job(1)])], [{"steps": [{"step": "build", "seconds": 20, "environment": "private"}]}])
        self.assertEqual(result["runner_seconds"], 60)
        self.assertEqual(result["optional_stage_timings"], [{"step": "build", "seconds": 20}])
        self.assertNotIn("private", json.dumps(result))
        for value in (-1, float("nan"), True):
            with self.assertRaises(ValueError):
                cost.report([], [{"steps": [{"step": "build", "seconds": value}]}])

    def test_same_interval_different_jobs_and_later_attempts_are_charged(self):
        result = cost.report([bundle([job(1, name="first"), job(2, name="second"),
                                     job(3, name="first", start="00:01:10", end="00:02:10", run_attempt=2)])])
        self.assertEqual(result["runner_seconds"], 180)
        self.assertEqual(result["runs"][0]["duplicate_records_removed"], 0)

    def test_malformed_cli_input_fails_without_writing_report(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, output = root / "bad.json", root / "out.json"
            source.write_text("{bad json", encoding="utf-8")
            with self.assertRaises(SystemExit) as raised:
                cost.main(["--input", str(source), "--json-output", str(output)])
            self.assertEqual(raised.exception.code, 2)
            self.assertFalse(output.exists())

    def test_completed_snapshot_supersedes_same_id_incomplete_snapshot(self):
        first, second = bundle([job(1, end=None, conclusion=None)]), bundle([job(1)])
        for inputs in ([first, second], [second, first]):
            result = cost.report(inputs)
            self.assertEqual(result["runner_seconds"], 60)
            self.assertFalse(result["is_lower_bound"])
            self.assertEqual(result["runs"][0]["incomplete_jobs"], 0)

    def test_same_id_conflicting_completed_end_or_name_fails_closed(self):
        for changed in (job(1, end="00:02:10"), job(1, name="different")):
            with self.subTest(changed=changed), self.assertRaises(ValueError):
                cost.report([bundle([job(1)]), bundle([changed])])

    def test_conflicting_incomplete_identity_fails_closed(self):
        with self.assertRaises(ValueError):
            cost.report([bundle([job(1, name="different", end=None)]), bundle([job(1)])])

    def test_empty_sample_is_not_exact_zero_but_verified_empty_run_is(self):
        for inputs in ([], [{"runs": []}]):
            result = cost.report(inputs)
            self.assertTrue(result["is_lower_bound"])
        result = cost.report([bundle([])])
        self.assertFalse(result["is_lower_bound"])
        self.assertEqual(result["runner_seconds"], 0)

    def test_missing_completeness_count_is_uncertain(self):
        for jobs in ([], [job(1)]):
            source = bundle(jobs)
            del source["runs"][0]["total_count"]
            self.assertTrue(cost.report([source])["is_lower_bound"])

    def test_cli_outputs_json_and_human_text(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, output, text = (root / name for name in ("in.json", "out.json", "out.txt"))
            source.write_text(json.dumps(bundle([job(1)])), encoding="utf-8")
            self.assertEqual(cost.main(["--input", str(source), "--json-output", str(output), "--text-output", str(text)]), 0)
            self.assertEqual(json.loads(output.read_text())["runner_seconds"], 60)
            self.assertIn("42 |", text.read_text())


if __name__ == "__main__":
    unittest.main()
