//! MISAKA Studio — the desktop shell.
//!
//! A native window over the same UI bundle the runtime serves, with the runtime itself supervised
//! as a child process. The window is a client of `http://127.0.0.1:<port>`; there is no private
//! bridge between them, which is what keeps the public API honest.
//!
//! # What it does before the window opens
//!
//! 1. Reads the configured port from the settings file the runtime uses.
//! 2. Attaches to a runtime already listening there (a headless `misaka-studiod`, or a second
//!    window) instead of starting a rival that would load the same model into the same GPU twice.
//! 3. Otherwise spawns one, and waits for it to answer `/api/v1/health`.
//!
//! # What it does when the window closes
//!
//! Kills the runtime **only if it spawned it**. A runtime someone started themselves outlives the
//! window; one this shell started must not outlive it holding 20 GB of VRAM.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use misaka_studio_desktop::{RuntimeSource, api_base_script, configured_port, health_check, locate_runtime_binary, resolve_runtime};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};

/// How long to wait for a freshly spawned runtime to answer. Generous: the runtime scans the
/// model directory at startup, and that directory can be on a slow external disk.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);

/// The child runtime, when this process started one.
struct Supervised(Mutex<Option<Child>>);

impl Supervised {
    fn stop(&self) {
        if let Ok(mut guard) = self.0.lock()
            && let Some(mut child) = guard.take()
        {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn main() {
    let settings_path = default_settings_path();
    let port = configured_port(&settings_path);
    let source = resolve_runtime(port);
    let base_url = source.base_url();

    let child = match &source {
        RuntimeSource::Attached(port) => {
            eprintln!("misaka-studio: attaching to the runtime already on port {port}");
            None
        }
        RuntimeSource::Spawn(port) => match spawn_runtime(*port) {
            Ok(child) => Some(child),
            Err(message) => {
                // No window yet, so there is nowhere to show a dialog: say it on stderr and exit
                // non-zero rather than opening a window that can never work.
                eprintln!("misaka-studio: {message}");
                std::process::exit(1);
            }
        },
    };

    if child.is_some() && !wait_for_health(source.port()) {
        eprintln!(
            "misaka-studio: the runtime did not answer on {} within {}s. \
             Run `misaka-studiod --check` to see how it is configured.",
            source.base_url(),
            STARTUP_TIMEOUT.as_secs()
        );
        std::process::exit(1);
    }

    let supervised = Supervised(Mutex::new(child));

    tauri::Builder::default()
        .manage(supervised)
        .setup(move |app| {
            WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
                .title("MISAKA Studio")
                .inner_size(1280.0, 860.0)
                .min_inner_size(880.0, 600.0)
                // Injected before the app's own bundle runs, so `window.__MISAKA_STUDIO_API__` is
                // set by the time the first request is made.
                .initialization_script(api_base_script(&base_url))
                .build()?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("the Tauri application builds")
        .run(|app, event| {
            // Both events: Exit covers the normal path, ExitRequested covers a quit that a
            // window's close handler started. Killing twice is harmless; killing never is a
            // leaked process holding the GPU.
            if matches!(event, RunEvent::Exit | RunEvent::ExitRequested { .. })
                && let Some(supervised) = app.try_state::<Supervised>()
            {
                supervised.stop();
            }
        });
}

fn spawn_runtime(port: u16) -> Result<Child, String> {
    let exe_dir = std::env::current_exe().ok().and_then(|exe| exe.parent().map(PathBuf::from));
    let binary = locate_runtime_binary(exe_dir.as_deref()).ok_or_else(|| {
        "could not find `misaka-studiod`. A packaged build ships it beside this executable; \
         in development, build it with `cargo build -p misaka-studio-runtime`, or point \
         MISAKA_STUDIOD at it."
            .to_string()
    })?;

    Command::new(&binary)
        .arg("--port")
        .arg(port.to_string())
        // Loopback, always. The shell has no business exposing the user's models to their network.
        .arg("--host")
        .arg("127.0.0.1")
        // The belt to the exit handler's braces. A force-quit or a crash never runs that handler,
        // and an orphaned runtime keeps a model resident — so the runtime is given a pipe on stdin
        // and exits when it closes. The kernel closes it when this process dies, however it dies,
        // which is a guarantee no amount of polling the parent's PID can match.
        .arg("--exit-on-stdin-close")
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("could not start {}: {e}", binary.display()))
}

fn wait_for_health(port: u16) -> bool {
    let deadline = Instant::now() + STARTUP_TIMEOUT;
    while Instant::now() < deadline {
        if health_check(port, Duration::from_millis(500)) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    false
}

/// The settings file the runtime uses. Kept in step with
/// `misaka_studio_core::settings::default_settings_path`.
fn default_settings_path() -> PathBuf {
    let data_dir = if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(".")).join("MISAKA Studio")
    } else if cfg!(target_os = "macos") {
        home().join("Library/Application Support/MISAKA Studio")
    } else {
        std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".local/share")).join("misaka-studio")
    };
    data_dir.join("settings.json")
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}
