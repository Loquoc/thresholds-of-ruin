//! Experimental Android entry point for the 3D client; see the README beside
//! this crate.
//!
//! Arguments come from `args.txt` in the app's files directory (whitespace
//! separated), defaulting to the offline bestiary. A startup error is written
//! to `error.txt` beside it, since Android discards stderr.
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
    for word in text.as_deref().unwrap_or("--bestiary").split_whitespace() {
        // `TOR_NAME=value` words set the environment the client reads.
        match word.split_once('=') {
            Some((name, value)) if name.starts_with("TOR_") => std::env::set_var(name, value),
            _ => arguments.push(word.to_owned()),
        }
    }
    let _ = bevy::window::ANDROID_APP.set(app);
    let _ = tor_client_3d::THREAD_PINNER.set(pin);
    if let Err(error) = tor_client_3d::run_with(arguments) {
        if let Some(directory) = directories.iter().flatten().next() {
            let _ = std::fs::write(directory.join("error.txt"), error.to_string());
        }
    }
}
