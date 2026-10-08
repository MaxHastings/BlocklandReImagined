//! The Linux desktop libraries the window and GPU load at run time. The
//! window system loads some of them with a panic when one is absent, and a
//! missing Vulkan loader only shows up as a GPU failure, so the game checks
//! them first and names what to install.

/// A library loaded at run time, with the package that provides it on the
/// common distribution families.
struct Library {
    soname: &'static str,
    debian: &'static str,
    fedora: &'static str,
    arch: &'static str,
}

const fn library(
    soname: &'static str,
    debian: &'static str,
    fedora: &'static str,
    arch: &'static str,
) -> Library {
    Library {
        soname,
        debian,
        fedora,
        arch,
    }
}

/// Keyboard layouts, for both window systems.
const XKBCOMMON: Library = library(
    "libxkbcommon.so.0",
    "libxkbcommon0",
    "libxkbcommon",
    "libxkbcommon",
);
/// The Vulkan loader the GPU opens.
const VULKAN: Library = library(
    "libvulkan.so.1",
    "libvulkan1",
    "vulkan-loader",
    "vulkan-icd-loader",
);
/// What winit opens for an X11 window (its x11 module and xkbcommon-dl).
const X11: [Library; 5] = [
    library("libX11.so.6", "libx11-6", "libX11", "libx11"),
    library("libX11-xcb.so.1", "libx11-xcb1", "libX11-xcb", "libx11"),
    library("libXcursor.so.1", "libxcursor1", "libXcursor", "libxcursor"),
    library("libXi.so.6", "libxi6", "libXi", "libxi"),
    library(
        "libxkbcommon-x11.so.0",
        "libxkbcommon-x11-0",
        "libxkbcommon-x11",
        "libxkbcommon-x11",
    ),
];
/// What winit opens for a Wayland window (its cursors are drawn by the
/// pure-Rust wayland-cursor crate, which loads no library).
const WAYLAND: [Library; 1] = [library(
    "libwayland-client.so.0",
    "libwayland-client0",
    "libwayland-client",
    "wayland",
)];

/// Whether winit will open a Wayland window: it prefers Wayland whenever the
/// session names a compositor, the same test it makes, and does not fall back
/// to X11 when Wayland fails (winit 0.30 `platform_impl/linux/mod.rs`).
pub fn wayland_session() -> bool {
    let set = |name| std::env::var_os(name).is_some_and(|value| !value.is_empty());
    set("WAYLAND_DISPLAY") || set("WAYLAND_SOCKET")
}

fn needed(wayland: bool) -> Vec<&'static Library> {
    let window: &[Library] = if wayland { &WAYLAND } else { &X11 };
    window.iter().chain([&XKBCOMMON, &VULKAN]).collect()
}

fn loads(soname: &str) -> bool {
    // SAFETY: these are the system libraries the window and GPU load next
    // anyway; opening one runs only its own initializers, and it is closed
    // again when the handle drops.
    unsafe { libloading::Library::new(soname) }.is_ok()
}

/// The player-facing explanation for libraries that did not load.
fn missing_message(missing: &[&Library]) -> String {
    let packages = |name: fn(&Library) -> &'static str| {
        let mut names = Vec::new();
        for library in missing {
            if !names.contains(&name(library)) {
                names.push(name(library));
            }
        }
        names.join(" ")
    };
    format!(
        "Some system libraries the game needs are not installed: {}. Install them with your package manager, for example: Ubuntu, Debian or Mint: sudo apt install {}; Fedora: sudo dnf install {}; Arch: sudo pacman -S {}",
        missing
            .iter()
            .map(|library| library.soname)
            .collect::<Vec<_>>()
            .join(", "),
        packages(|library| library.debian),
        packages(|library| library.fedora),
        packages(|library| library.arch),
    )
}

/// Fails with what to install when a library the window or GPU needs is
/// missing.
pub fn check() -> anyhow::Result<()> {
    let missing: Vec<_> = needed(wayland_session())
        .into_iter()
        .filter(|library| !loads(library.soname))
        .collect();
    anyhow::ensure!(missing.is_empty(), missing_message(&missing));
    Ok(())
}

/// Set on a game relaunched on X11, so it never relaunches again.
const X11_RELAUNCH: &str = "BRI_X11_RELAUNCH";

/// winit has no X11 fallback once its Wayland connection fails, which it
/// reports as `ExitFailure(1)` (NVIDIA on CachyOS failed this way right
/// after the first frame). When that happened and an X server (XWayland) is
/// available, runs the game again on X11 and returns its exit code.
pub fn relaunch_on_x11(error: &anyhow::Error) -> Option<i32> {
    use winit::error::EventLoopError;
    let failed = error.chain().any(|cause| {
        matches!(
            cause.downcast_ref::<EventLoopError>(),
            Some(EventLoopError::ExitFailure(_))
        )
    });
    let set = |name| std::env::var_os(name).is_some_and(|value| !value.is_empty());
    if !failed || !wayland_session() || !set("DISPLAY") || set(X11_RELAUNCH) {
        return None;
    }
    bri_console::warn("The Wayland window connection failed; starting again on X11.");
    let status = std::process::Command::new(std::env::current_exe().ok()?)
        .args(std::env::args_os().skip(1))
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("WAYLAND_SOCKET")
        .env(X11_RELAUNCH, "1")
        .status()
        .ok()?;
    Some(status.code().unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_window_system_checks_its_own_libraries_and_the_shared_ones() {
        let names = |wayland| -> Vec<_> { needed(wayland).iter().map(|l| l.soname).collect() };
        let x11 = names(false);
        let wayland = names(true);
        assert!(x11.contains(&"libxkbcommon-x11.so.0"));
        assert!(!wayland.contains(&"libxkbcommon-x11.so.0"));
        assert!(wayland.contains(&"libwayland-client.so.0"));
        for shared in ["libxkbcommon.so.0", "libvulkan.so.1"] {
            assert!(x11.contains(&shared) && wayland.contains(&shared));
        }
    }

    #[test]
    fn the_message_names_each_missing_library_and_its_package_once() {
        let message = missing_message(&[&X11[0], &X11[1], &VULKAN]);
        assert!(message.contains("libX11.so.6, libX11-xcb.so.1, libvulkan.so.1"));
        assert!(message.contains("sudo apt install libx11-6 libx11-xcb1 libvulkan1"));
        // Arch ships both X11 libraries in one package.
        assert!(message.contains("sudo pacman -S libx11 vulkan-icd-loader"));
    }

    #[test]
    fn a_library_that_does_not_exist_does_not_load() {
        assert!(!loads("libbri-no-such-library.so.0"));
    }
}
