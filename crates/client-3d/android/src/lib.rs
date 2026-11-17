//! Experimental Android entry point for the 3D client; see the README beside
//! this crate.
//!
//! By default it plays alone: the shipped scenarios are installed into the
//! app's private storage and the client opens its menu, which hosts each game
//! in this process. Arguments in `args.txt` in the app's files directory
//! (whitespace separated) replace that, for example for profiling. A startup
//! error is written to `error.txt` beside it, since Android discards stderr.
#![cfg(target_os = "android")]

use bevy::window::android_activity::AndroidApp;

/// Restricts the calling thread to the given cores.
fn pin(cores: &[usize]) {
    // SAFETY: `set` is a plain bit set that is fully initialized by zeroing,
    // and thread 0 names the calling thread.
    unsafe {
        let mut set: libc::cpu_set_t = std::mem::zeroed();
        for core in cores {
            libc::CPU_SET(*core, &mut set);
        }
        libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &set);
    }
}

#[no_mangle]
fn android_main(app: AndroidApp) {
    // The private directory is writable through `adb shell run-as`.
    let directories = [app.internal_data_path(), app.external_data_path()];
    let text = directories
        .iter()
        .flatten()
        .find_map(|directory| std::fs::read_to_string(directory.join("args.txt")).ok());
    let mut arguments = Vec::new();
    for word in text.as_deref().unwrap_or("").split_whitespace() {
        // `TOR_NAME=value` words set the environment the client reads.
        match word.split_once('=') {
            Some((name, value)) if name.starts_with("TOR_") => std::env::set_var(name, value),
            _ => arguments.push(word.to_owned()),
        }
    }
    let private = app.internal_data_path();
    let _ = bevy::window::ANDROID_APP.set(app);
    let _ = tor_client_3d::THREAD_PINNER.set(pin);
    let result = match (arguments.is_empty(), private) {
        (true, Some(private)) => standalone(&private),
        (true, None) => Err("Android gave the app no private storage".into()),
        (false, _) => tor_client_3d::run_with(arguments),
    };
    if let Err(error) = result {
        if let Some(directory) = directories.iter().flatten().next() {
            let _ = std::fs::write(directory.join("error.txt"), error.to_string());
        }
    }
    // Android keeps the process after the activity finishes and starts the
    // next launch in it, where the window library refuses a second event
    // loop. The client has already saved and stopped its private server
    // before returning, so the process ends here and each launch is fresh.
    bevy::log::info!("Client returned; ending the process");
    // SAFETY: `_exit` takes no pointers. It skips exit-time handlers, which
    // can wait on graphics threads that are still running.
    unsafe { libc::_exit(0) }
}

/// Plays alone: scenarios and saves live in the app's private storage.
fn standalone(private: &std::path::Path) -> Result<(), tor_client_3d::Error> {
    let scenarios = private.join("scenarios");
    tor_client_3d::install_scenarios(&scenarios)?;
    tor_client_3d::run_with(vec![
        "--menu".into(),
        "--scenario-root".into(),
        scenarios.display().to_string(),
        "--save-root".into(),
        private.join("saves").display().to_string(),
    ])
}
