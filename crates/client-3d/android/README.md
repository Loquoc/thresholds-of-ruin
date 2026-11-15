# Experimental Android client

This crate packages the 3D client for Android. It is an **experiment**: it has
run on one tablet (an Amazon Fire HD 10 with a Mali-G52 GPU), it is not built or
tested by CI, and it is not a supported way to play. It exists to show that the
client can reach a playable frame rate on hardware far below a desktop GPU, and
to exercise the [render profiles](../README.md#render-profiles) that make it so.

It is a separate Cargo workspace because the Android entry point needs
`no_mangle` and a small `unsafe` block for thread pinning, which the client
crate's `unsafe_code = "forbid"` lint rejects.

## What works

- The offline review modes, such as `--bestiary`.
- Live play against a server reached over the network, with `--connect`.
- One-finger touch: a touch moves the pointer and holds the left mouse button,
  so the interface buttons and tap-to-move work.

## What does not

- No private server: the client cannot start `tor-server` on the device, so
  `--connect` and `TOR_SERVER_TOKEN` are required for live play.
- No on-screen replacements for keyboard-only actions, and no multi-touch.
- The first launch shows a black screen for about half a minute while shaders
  compile.
- Each touch logs one "could not set cursor position" error from the window
  library, which has no pointer to move on Android.

## Measured

On the Fire HD 10, with the `low` profile and a server on another machine:

| Scene | Frame rate |
| --- | --- |
| Ordinary play, up to four creatures in view | about 33 fps, 96% of frames within 34 ms |
| Eight large creatures close to the camera | about 23 fps at 40% scale |
| The 34-creature bestiary | 17 to 22 fps |

Without the `low` profile the same tablet draws the bestiary at about one frame
a second.

## Workarounds to revisit

- **Surface size.** A smaller picture is drawn by shrinking the window's
  surface, which the system compositor stretches for free. The window library
  restores the screen's size every frame, so the profile sets the smaller size
  again after it, every frame. A clean version needs the window library to accept
  a requested size on Android.
- **Thread pinning.** The game and render threads are kept on the fastest
  cores through `sched_setaffinity`. Without it, about one frame in six misses
  its deadline instead of one in twenty-five.
- **Startup splash.** The desktop client shows its splash from a second
  process, which Android cannot start; the splash is skipped there.

## Build

Install the Android SDK and NDK, the Rust target and `cargo-apk`:

```sh
rustup target add aarch64-linux-android
cargo install cargo-apk
```

Set `ANDROID_HOME` and `ANDROID_NDK_ROOT`, then from this directory:

```sh
cargo apk build
```

`cargo-apk` signs only debug builds automatically, so this crate optimizes its
dev profile. Install the resulting package with `adb install -r`.

## Run

Android has no command line or environment for an app, so the client reads both
from `args.txt` in its private files directory: whitespace-separated arguments,
where a word of the form `TOR_NAME=value` sets an environment variable instead.
Without the file it runs `--bestiary`. Write it through `adb`:

```sh
adb shell "run-as org.example.torprobe sh -c 'echo \"--bestiary\" > files/args.txt'"
adb shell am start -n org.example.torprobe/android.app.NativeActivity
```

For live play, start `tor-server` on a development machine, forward its port to
the device with `adb reverse tcp:PORT tcp:PORT`, and write
`--connect 127.0.0.1:PORT TOR_SERVER_TOKEN=...` instead. Use a throwaway token:
the file is readable by anyone with debugging access to the device.

A startup error is written to `error.txt` beside `args.txt`, since Android
discards the client's standard error. `TOR_RENDER_REPORT=5` logs frame timing
every five seconds, readable with `adb logcat`.
