# The client App is boxed from load: start-up fits Windows' 1 MB main stack

## Failure
After the app.rs split, `launch.rs::a_startup_failure_tells_the_player_and_points_at_the_logs`
overflowed the main thread's stack on Windows (1 MB) before the error dialog.

## Cause
`App` is about 67 KB, and `App::load` returned it by value. In debug builds
every move keeps its own stack copy, so the start-up path stacked
`main` → `game` (270 KB, the `--check` branch's App) → `run` (400 KB:
the `Result<App>`, the App and its `Box::new` copy) → `load_with_audio`
(266 KB on main, 303 KB after the field grouping added nested-struct
temporaries). Measured with `ulimit -s` on Linux: main already needed
768-1024 KB, so the split's 37 KB tipped it over; it was not the split's
alone. Frame sizes are the `sub $N,%r11` stack probes in the debug binary.

## Fix
`App::load` and `App::load_with_audio` return `Box<App>`; the App is
boxed where it is built, and `run` hands the box straight to the platform.
Start-up failure now runs in under 384 KB (it needed over 1 MB).

## Guard
`launch.rs::a_startup_failure_fits_in_the_windows_main_thread_stack`
(Unix) runs the same start-up failure under `ulimit -s 1024`. It fails on
the split without the box.
