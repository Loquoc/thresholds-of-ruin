# Reviewed CI test owners

This source/assertion review closes the 53-class migration backlog. Fifty owners
use data, source files or mocked peers without Rust/native application execution;
three require the real compiler, server or clients. No tests, assertions or
existing owner registrations are removed. The [testing policy](testing.md) owns
selection and qualification; this page records why these registrations are safe.

The [independent review catalog](../../scripts/testdata/ci-ownership/review-20261110.json)
lists every reviewed class, its resource kind and all direct witness methods.
Production selection never reads that catalog: the registry remains explicit
source policy. New classes, unknown/deleted source, verification controls and
incomplete discovery retain exhaustive fallback. Adding a future class is not
automatically approved by this review. New methods in an already reviewed class
retain the existing complete-current-class selection contract.

## Assertion and resource mapping

Tooling can include Python/Git metadata subprocesses; it means no Rust/native
application requirement, not that the test has no file I/O. Mocked process
fixtures do not replace their separate real process/native acceptance owners.
Recorder factory tests inspect IDs and run fake lifecycle hooks, not the real
recording cases. Content tests inspect TOML; they do not prove compiler or
gameplay acceptance. Item-model tests construct scene JSON and do not render it.

| Owner | Resource | Independent assertion/boundary evidence |
| --- | --- | --- |
| [test_active_mob_performance_report.DemandDrivenActiveMobReportTests](../../scripts/test_active_mob_performance_report.py) | tooling | Explicit current/legacy observer counts, missing-field mutations and independent replay/activity rejection; synthetic report rows. |
| [test_ci_workflow_contracts.LinuxDebugShardWorkflow](../../scripts/test_ci_workflow_contracts.py) | tooling | Inspects shard caps, attempt-bound names and exact-union validation commands; never runs the workflow. |
| [test_cloak_sweep_audit.SweptFaceAudit](../../scripts/test_cloak_sweep_audit.py) | tooling | Independent convex-contact examples, conservative degeneracies and corrupt/missing-trace rejection; mathematical fixtures only. |
| [test_creature_process.ArenaPreparationSynchronization](../../scripts/test_creature_process.py) | tooling | Fake Driver supplies early Ready/old snapshot/ability-start frames; only the causal start satisfies the barrier. |
| [test_creature_wizard_ordering.OwnedHitDieWizardOrdering](../../scripts/test_creature_wizard_ordering.py) | tooling | Mocks server, Text mutator, observer and compiler; checks each mutation syncs its own socket and refused sync stops. |
| [test_equipment_performance_report.EquipmentReport](../../scripts/test_equipment_performance_report.py) | tooling | Synthetic four-size report matrix, malformed sample rejection and example routing; no benchmark execution. |
| [test_model_performance_report.ModelReport](../../scripts/test_model_performance_report.py) | tooling | Synthetic model counts/timings and hair phase containment; parse/extract routing never builds or renders models. |
| [test_perf_compare.BuildRouting](../../scripts/test_perf_compare.py) | tooling | Mocks subprocess.run; checks per-worktree manifest routing and rejects colliding outputs before a build call. |
| [test_performance_driver.DisclosedStateComparison](../../scripts/test_performance_driver.py) | tooling | Constructed owner/spectator frames check exact public fields, private redaction, causal wait receipts and nonmutation. |
| [test_performance_driver.PlacePictureDemand](../../scripts/test_performance_driver.py) | tooling | Constructed histories and fake Client test public/private wait demands and separate causal/current-picture metrics. |
| [test_performance_report.BoundaryProofWorkTests](../../scripts/test_performance_report.py) | tooling | Synthetic work counters require atomic proof fields, exact observer accounting, door allowance and zero-work waits. |
| [test_performance_report.DemandDrivenObserverWorkTests](../../scripts/test_performance_report.py) | tooling | Synthetic version/marker profiles require each observer stage exactly once and reject eager display work. |
| [test_performance_report.SaveStatusMeasurementTests](../../scripts/test_performance_report.py) | tooling | Constructed save-status rows distinguish missing/null/zero deferral and reject invalid count values. |
| [test_process_fixture_identities.ProcessFixtureIdentities](../../scripts/test_process_fixture_identities.py) | tooling | Builds authoring dictionaries and reads TOML; compiler subprocesses are mocked, with explicit identity/appearance assertions. |
| [test_process_harness.CausalPictureCompletion](../../scripts/test_process_harness.py) | tooling | Fake frame queue exercises real completion predicates; checks lifecycle ordering, one observation demand and exact state comparison. |
| [test_process_harness.CompactDetectionMemory](../../scripts/test_process_harness.py) | tooling | Mocked detection-memory frame/command returns verify compact hydration uses inspection without losing memory assertions. |
| [test_process_harness.InspectionGenerationComparison](../../scripts/test_process_harness.py) | tooling | Constructed inspection dictionaries allow only attachment-local revision drift and reject authority/content differences. |
| [test_process_harness.NativeCommandLetterFixture](../../scripts/test_process_harness.py) | tooling | Mocks native key/text/frame injection on both OS paths; verifies exactly one letter and release on duplicate/error. |
| [test_process_harness.SpectatorStateComparison](../../scripts/test_process_harness.py) | tooling | Constructed directional spectator projection rejects private leaks, missing keys and public/position differences without mutation. |
| [test_projectile_process.ProjectileSettle](../../scripts/test_projectile_process.py) | tooling | Mocks snapshot/request/act; checks peer readiness, one checked wait, rejection and finite settlement bounds; no server. |
| [test_record_wire_samples.DrainRefusalRecordingContracts](../../scripts/test_record_wire_samples.py) | tooling | Checks actual factory identity without running it, then fake lifecycle cleanup/publication and atomic failure preservation. |
| [test_record_wire_samples.FirstContactRecordingContracts](../../scripts/test_record_wire_samples.py) | tooling | Checks actual factory identity without running it; synthetic reach/effect artifacts require cleanup and valid discriminators. |
| [test_record_wire_samples.GrowthRecordingContracts](../../scripts/test_record_wire_samples.py) | tooling | Fake lifecycle and synthetic acquisition/credit diagnostics preserve both results atomically after cleanup. |
| [test_record_wire_samples.HallucinationRecordingContracts](../../scripts/test_record_wire_samples.py) | tooling | Fake lifecycle/raw snapshots enforce visual/extension/cure, every body occurrence, privacy and inclusive duration endpoints. |
| [test_record_wire_samples.HasteRecordingContracts](../../scripts/test_record_wire_samples.py) | tooling | Fake lifecycle and synthetic haste/overdose diagnostics preserve both branches and reject bad rest/cleanup. |
| [test_record_wire_samples.RecorderCaptureStorageTests](../../scripts/test_record_wire_samples.py) | tooling | Mocks ProcessTestCase.launch and all nested lifecycle setup; verifies per-launch capture options and restoration after failure. |
| [test_record_wire_samples.SelfDeviceRecordingContracts](../../scripts/test_record_wire_samples.py) | tooling | Factory identity inspection plus fake self-device lifecycle; failed cleanup cannot publish a partial sample. |
| [test_record_wire_samples.TorchRecordingContracts](../../scripts/test_record_wire_samples.py) | tooling | Factory identity inspection plus fake torch states; validates explicit on/off commands, fuel states and cleanup atomicity. |
| [test_retention_measure.RetentionMeasurement](../../scripts/test_retention_measure.py) | tooling | Synthetic eight-case measurement rows test decoding, identity/count recovery and completeness; enforces local-only driver entry points. |
| [test_rogue_acceptance.Acceptance](../../scripts/test_rogue_acceptance.py) | tooling | Constructed campaign/species summaries validate depth, death cause, food bands and explicitly retained waiver outcomes. |
| [test_rogue_detection_content.RogueDetectionContent](../../scripts/test_rogue_detection_content.py) | tooling | Reads authored TOML and identity tables; checks exact detection effects, labels, species tuples and unchanged stock/kits. |
| [test_rogue_drain_content.RogueDrainContent](../../scripts/test_rogue_drain_content.py) | tooling | Reads authored TOML; independently checks paid self-device drain definition and unchanged live stock/kits. |
| [test_rogue_earned_route_process_support.RestoredPictureIdentity](../../scripts/test_rogue_earned_route_process_support.py) | tooling | Constructed restored pictures allow only snapshot/revision drift; tick, observation, memory and history differences reject. |
| [test_rogue_first_contact_content.RogueFirstContactContent](../../scripts/test_rogue_first_contact_content.py) | tooling | Reads authored/shared TOML; independently checks cancellation/missile effects, exact cancellable-source scope and preserved kits. |
| [test_rogue_generation_probe.Pooling](../../scripts/test_rogue_generation_probe.py) | tooling | Constructed generation rows verify pooled rates/z score and seed range expansion; no probe process runs. |
| [test_rogue_growth_content.RogueGrowthContent](../../scripts/test_rogue_growth_content.py) | tooling | Reads TOML and explicit identity oracle; validates dedicated growth effects, cosmetic capacity and preserved restoration/stock. |
| [test_rogue_hallucination_content.RogueHallucinationContent](../../scripts/test_rogue_hallucination_content.py) | tooling | Reads TOML; exact hallucination duration/condition/cure/label assertions preserve old stock and kit rules. |
| [test_rogue_haste_content.RogueHasteContent](../../scripts/test_rogue_haste_content.py) | tooling | Reads TOML; exact haste/rest timing/lifetime assertions preserve other condition policies and kits. |
| [test_rogue_item_model_coverage.RogueItemModelCoverage](../../scripts/test_rogue_item_model_coverage.py) | tooling | Reads committed JSON/TOML/Rust source and invokes a data-only scene generator; validates identity/form/appearance mapping, never renders. |
| [test_rogue_item_stages_process.RogueItemStageProcesses](../../scripts/test_rogue_item_stages_process.py) | application | ProcessTestCase runs the real tor-scenario compiler on positive and mutated copied packages; requires binary producer. |
| [test_rogue_light_content.RogueLightContent](../../scripts/test_rogue_light_content.py) | tooling | Reads TOML; exact paid room-light self-device definition and unchanged live stock/kits. |
| [test_rogue_mapping_content.RogueMappingContent](../../scripts/test_rogue_mapping_content.py) | tooling | Reads TOML; exact topology-map operation, scroll appearance capacity and unchanged starting kit. |
| [test_rogue_nothing_content.RogueNothingContent](../../scripts/test_rogue_nothing_content.py) | tooling | Reads TOML; exact paid no-op definition, forms/appearance capacity and unchanged eight-device stock. |
| [test_rogue_pacing_model.PacingModel](../../scripts/test_rogue_pacing_model.py) | tooling | Loads authored growth/population and computes bounded analytical expectations against independent tier/level/rate assertions. |
| [test_rogue_presentation_vectors.RoguePresentationVectors](../../scripts/test_rogue_presentation_vectors.py) | tooling | Reads rosters and computes an independent unsigned selector with fixed vectors and distinct visual/monster-sense channels. |
| [test_rogue_profile_content.RogueProfileContent](../../scripts/test_rogue_profile_content.py) | tooling | Reads TOML; independently checks locked schedule, capability flags, kit, exact source differences and item/rider/healing rules. |
| [test_rogue_reference_model.ReferenceModel](../../scripts/test_rogue_reference_model.py) | tooling | Pure Fraction/dice/reference calculations and in-process JSON CLI; hand-derived XP, food, trap and healing expectations. |
| [test_rogue_reference_targets.ReferenceTargets](../../scripts/test_rogue_reference_targets.py) | tooling | Pure hit/save/damage distributions and in-process JSON CLI; independent probability and actions-to-kill expectations. |
| [test_rogue_species_tuning.SpeciesTuning](../../scripts/test_rogue_species_tuning.py) | tooling | Synthetic TOML and study rows test bounded tuning proposals, comment-preserving edits, natural accuracy and zero removal. |
| [test_rogue_torch_content.RogueTorchContent](../../scripts/test_rogue_torch_content.py) | tooling | Reads TOML; exact finite held-light/tool socket definitions, primary socket preservation and no live stock insertion. |
| [test_save_backpressure_process.SaveBackpressure](../../scripts/test_save_backpressure_process.py) | application | ProcessTestCase launches real server/headless clients, holds SQLite exclusively and checks queued-save/held-command release and durability. |
| [test_scenario_capabilities_process.ScenarioCapabilityProcesses](../../scripts/test_scenario_capabilities_process.py) | application | ProcessTestCase runs real compiler/server/clients; checks authored growth, no corpses, live kit and rejected capability/schedule mutations. |
| [test_strike_sequences_process.InitialSequencePeerTests](../../scripts/test_strike_sequences_process.py) | tooling | Mocks package/server/client/request/act/frame and stops before save/UI; peer refresh then one checked wait, rejection prevents prefix wait. |

## Verification and limits

Regression coverage uses the complete independently discovered inventory rather
than a filtered owner fixture. Ordinary tuning-tool and report-test changes must produce Linux and
Windows tooling lanes without application/Rust builds. Shared simulation changes
must still require Rust/application lanes and fixed native/release witnesses.
Control changes, deleted/unsafe source and a newly injected unknown class must
still require exhaustive coverage. A real Python child process exercises the
planner API; this is local integration evidence, not GitHub event qualification.

The reviewed tooling owners are also exercised with the existing no-build guard
and an empty application directory. The three application classes retain their
actual-process assertions. No-build auditing covers the documented Python/shim
launch forms, not arbitrary native or shell descendants; source review remains
necessary. This change itself edits verification controls, so its eventual PR
still requires exhaustive CI. There is no claim of a fresh release/platform
matrix or measured hosted runner savings while Actions remains disabled.

The complete tooling probe also exposed nested guard self-test assumptions.
Those tests now launch fixture shims in a child using the fixture environment
and cover both dispatch forwarding branches with mocks. Version/audit, offline
metadata and forbidden-command assertions remain; production guard code is
unchanged. See the [test-effectiveness ledger](test-effectiveness.md) for the
failed probe and repair scope.

Two literal validator launches from `perf_compare` (equipment/model report
scripts) also have explicit reviewed dependency edges. Other unresolved
policy/fixture references in a Python dependency closure can still select full
coverage; this review does not waive those checks. CI cost-accounting scripts
remain verification controls, not ordinary tooling examples. Likewise the dirty
migration checkout has control changes and untracked source: these local probes
demonstrate future supported scopes, not narrow qualification for this import.
