use gtk::prelude::*;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::Duration;
use webkit2gtk::traits::WebViewExt;

const SERVER_URL: &str = "http://127.0.0.1:2891";
const SERVER_ADDR: &str = "127.0.0.1:2891";
const APP_NAME: &str = "CC Switch";
const WM_CLASS: &str = "cc-switch";

// ── Single-instance lock ──

fn lock_path() -> PathBuf {
    let dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(dir).join("cc-switch.lock")
}

/// Try to acquire the single-instance lock. Returns `true` if we got the lock.
/// If another instance is running, tries to focus its window and returns `false`.
fn acquire_lock() -> bool {
    let path = lock_path();

    // If lock file exists, check if owning process is alive
    if let Ok(pid_str) = std::fs::read_to_string(&path) {
        if let Ok(pid) = pid_str.trim().parse::<u32>() {
            if std::path::Path::new(&format!("/proc/{pid}")).exists() {
                // Another instance is running — try to focus it
                let _ = Command::new("xdotool")
                    .args(["search", "--class", "cc-switch", "windowactivate"])
                    .output();
                let _ = Command::new("wmctrl")
                    .args(["-a", APP_NAME])
                    .output();
                return false;
            }
        }
        // Stale lock (process died), remove it
        let _ = std::fs::remove_file(&path);
    }

    // Write our PID to the lock file
    std::fs::write(&path, std::process::id().to_string()).ok();
    true
}

// ── Server management ──

fn is_server_running() -> bool {
    TcpStream::connect_timeout(
        &SERVER_ADDR.parse().unwrap(),
        Duration::from_millis(300),
    )
    .is_ok()
}

fn wait_for_server(timeout: Duration) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if is_server_running() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

fn resolve_dist_dir() -> String {
    if let Ok(dir) = std::env::var("CC_SWITCH_DIST_DIR") {
        return dir;
    }
    let installed = "/usr/share/cc-switch-web/dist";
    if std::path::Path::new(installed).is_dir() {
        return installed.to_string();
    }
    "dist".to_string()
}

fn start_server() -> Option<Child> {
    let dist_dir = resolve_dist_dir();
    let bin_paths = [
        std::env::var("CC_SWITCH_BIN").ok(),
        Some("/usr/bin/cc-switch-web-server".to_string()),
        std::env::current_dir()
            .ok()
            .map(|d| d.join("../target/release/web-server").to_string_lossy().to_string()),
        std::env::current_dir()
            .ok()
            .map(|d| d.join("../target/debug/web-server").to_string_lossy().to_string()),
    ];

    for path in bin_paths.iter().flatten() {
        if let Ok(child) = Command::new(path)
            .env("CC_SWITCH_DIST_DIR", &dist_dir)
            .spawn()
        {
            eprintln!("Started web-server from: {path}");
            if wait_for_server(Duration::from_secs(10)) {
                return Some(child);
            }
            eprintln!("Server at {path} failed to start in time");
        }
    }

    eprintln!("Warning: Could not start web-server. The window may show a blank page.");
    None
}

fn send_shutdown() {
    invoke_api(r#"{"command":"shutdown","args":{}}"#);
}

fn invoke_api(body_json: &str) {
    let body = format!(
        "POST /api/invoke HTTP/1.1\r\n\
         Host: 127.0.0.1:2891\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\
         \r\n\
         {}",
        body_json.len(),
        body_json
    );
    if let Ok(mut stream) = TcpStream::connect(SERVER_ADDR) {
        let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
        let _ = stream.write_all(body.as_bytes());
        let _ = stream.flush();
        let mut buf = [0u8; 1024];
        let _ = stream.read(&mut buf);
    }
}

// ── Daemonize ──

/// Detach from terminal by forking. Parent exits immediately (terminal freed),
/// child continues in a new session with stdin/stdout/stderr redirected to /dev/null.
/// Pass `--foreground` or `-f` to skip daemonization (for debugging).
fn daemonize() {
    let foreground = std::env::args().any(|a| a == "--foreground" || a == "-f");
    if foreground {
        return;
    }

    match unsafe { libc::fork() } {
        0 => {
            // Child: create new session, detach from terminal
            unsafe { libc::setsid(); }

            // Redirect stdin/stdout/stderr to /dev/null
            let path = std::ffi::CString::new("/dev/null").unwrap();
            let null_fd = unsafe { libc::open(path.as_ptr(), libc::O_RDWR) };
            if null_fd >= 0 {
                unsafe {
                    libc::dup2(null_fd, 0); // stdin
                    libc::dup2(null_fd, 1); // stdout
                    libc::dup2(null_fd, 2); // stderr
                    if null_fd > 2 {
                        libc::close(null_fd);
                    }
                }
            }
        }
        child_pid if child_pid > 0 => {
            // Parent: exit, terminal is freed
            std::process::exit(0);
        }
        _ => {
            // Fork failed: run in foreground
            eprintln!("Warning: could not daemonize, running in foreground");
        }
    }
}

// ── Main ──

fn main() {
    // WebKitGTK 2.38 on Kylin ARM/Mali can render a blank surface when GPU
    // compositing is enabled. Keep the setting overridable for other systems.
    if std::env::var_os("WEBKIT_DISABLE_COMPOSITING_MODE").is_none() {
        std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
    }

    // Detach from terminal (like `code` command)
    daemonize();

    // Single instance check
    if !acquire_lock() {
        return;
    }

    // Build URL with native=1 flag
    let url = std::env::var("CC_SWITCH_URL").unwrap_or_else(|_| SERVER_URL.to_string());
    let separator = if url.contains('?') { '&' } else { '?' };
    let url = format!("{url}{separator}native=1");

    // Start the web-server as a child process
    let mut child_handle: Option<Child> = None;
    if is_server_running() {
        eprintln!("Web-server is already running (leftover from previous session?), will reuse.");
    } else {
        child_handle = start_server();
    }
    if !wait_for_server(Duration::from_secs(5)) {
        eprintln!("Warning: server did not become ready in time");
    }

    // Pass desktop session environment to web-server for terminal launcher
    let env_json = format!(
        r#"{{"command":"set_terminal_env","args":{{"DISPLAY":"{}","XAUTHORITY":"{}","DBUS_SESSION_BUS_ADDRESS":"{}","XDG_RUNTIME_DIR":"{}"}}}}"#,
        std::env::var("DISPLAY").unwrap_or_default().replace('"', "\\\""),
        std::env::var("XAUTHORITY").unwrap_or_default().replace('"', "\\\""),
        std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap_or_default().replace('"', "\\\""),
        std::env::var("XDG_RUNTIME_DIR").unwrap_or_default().replace('"', "\\\""),
    );
    invoke_api(&env_json);

    // ── GTK+WebKit window ──
    gtk::init().expect("Failed to initialize GTK");

    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title(APP_NAME);
    window.set_default_size(1200, 800);
    window.set_wmclass(WM_CLASS, APP_NAME);
    window.set_position(gtk::WindowPosition::Center);

    // Create the WebView and load our app
    let webview = webkit2gtk::WebView::new();
    // Kylin V10 ships WebKitGTK 2.38; keep diagnostics visible when the
    // frontend fails to execute, and allow opting out of GPU compositing.
    webview.connect_load_changed(|view, event| {
        eprintln!("WebView load event: {:?}, uri={}", event, view.uri().map(|u| u.to_string()).unwrap_or_default());
    });
    webview.connect_load_failed(|_, event, uri, error| {
        eprintln!("WebView load failed: {:?}, uri={}, error={}", event, uri, error);
        false
    });
    webview.load_uri(&url);

    window.add(&webview);
    window.show_all();

    // Quit GTK main loop when window is closed
    window.connect_delete_event(|_, _| {
        gtk::main_quit();
        Inhibit(false)
    });

    window.connect_destroy(|_| {
        gtk::main_quit();
    });

    gtk::main();

    // ── Cleanup ──
    eprintln!("Shutting down web-server...");
    send_shutdown();
    std::thread::sleep(Duration::from_millis(500));

    if let Some(mut child) = child_handle {
        let _ = child.kill();
        let _ = child.wait();
    }
    eprintln!("Web-server stopped.");
}
