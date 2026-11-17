fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args_os().len() != 1 {
        return Err("model_bench_equipment accepts no arguments; its workload is versioned".into());
    }
    std::env::remove_var("TOR_3D_PROFILE_BUILD");
    tor_client_3d::model_benchmark::run_equipment()
}
