//! Instrumented companion to model_bench_equipment. Stderr contains existing
//! model-build stage diagnostics. Its timings include instrumentation overhead
//! and must not be pooled with the uninstrumented benchmark baseline.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args_os().len() != 1 {
        return Err(
            "model_profile_equipment accepts no arguments; its workload is versioned".into(),
        );
    }
    std::env::set_var("TOR_3D_PROFILE_BUILD", "1");
    eprintln!(
        "equipment_profile instrumented=true; use model_bench_equipment for baseline timings"
    );
    tor_client_3d::model_benchmark::run_equipment()
}
