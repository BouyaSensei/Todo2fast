//! Todo2fast — entry point.
//!
//! Starts the HTTP API (axum + SQLite) and, on Windows, a native app window
//! (WebView2 — the same engine as Asana/Figma desktop) showing the UI at `/`.

pub mod ai;
pub mod api;
pub mod db;
pub mod models;
pub mod pdf_extract;
pub mod repo;

use std::sync::Arc;

use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

/// Build the application router. `state` is shared, immutable app state.
/// If `web_dir` points to a valid directory, the frontend SPA is served at `/`.
pub fn build_router(state: Arc<api::AppState>, web_dir: Option<std::path::PathBuf>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let mut app = Router::new()
        .merge(api::routes())
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    if let Some(dir) = web_dir {
        if dir.join("index.html").exists() {
            tracing::info!("Serving frontend from {}", dir.display());
            // SPA: serve static files, fall back to index.html for client routes.
            let spa = ServeDir::new(&dir).not_found_service(ServeFile::new(dir.join("index.html")));
            app = app.fallback_service(spa);
        } else {
            tracing::warn!("web_dir {} has no index.html — API only", dir.display());
        }
    }

    app
}

/// Per-user data directory: `%LOCALAPPDATA%\Todo2fast` on Windows,
/// `~/.local/share/todo2fast` elsewhere. Always writable by the current user,
/// even when the executable lives in read-only `Program Files`.
fn data_dir() -> std::path::PathBuf {
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        return std::path::PathBuf::from(local).join("Todo2fast");
    }
    if let Ok(home) = std::env::var("HOME") {
        return std::path::PathBuf::from(home).join(".local/share/todo2fast");
    }
    std::path::PathBuf::from(".")
}

/// Default port, unique to Todo2fast (avoids clashing with the generic 8080
/// that other local tools grab). Overridable via `T2F_ADDR`. If it is already
/// taken the app falls back to the next ports, then to an OS-assigned free one.
const DEFAULT_PORT: u16 = 42817;

/// Resolve the database path: `T2F_DB_PATH` wins, otherwise the per-user data dir.
fn db_path() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("T2F_DB_PATH") {
        return std::path::PathBuf::from(p);
    }
    data_dir().join("todo2fast.sqlite")
}

/// Candidate bind addresses, in order: the preferred port first, then the next
/// ten ports if it is taken, and finally an OS-assigned free port (`:0`). This
/// guarantees the app always finds a usable port.
fn port_candidates(preferred: &str) -> Vec<String> {
    let mut candidates = vec![preferred.to_string()];
    if let Some(p) = preferred
        .rsplit(':')
        .next()
        .and_then(|s| s.parse::<u16>().ok())
    {
        for i in 1..=10 {
            candidates.push(format!("0.0.0.0:{}", p.wrapping_add(i)));
        }
    }
    candidates.push("0.0.0.0:0".into()); // OS-assigned free port
    candidates
}

/// Bind to the preferred port; fall back to the next ports, then any free port.
#[cfg(not(windows))]
async fn bind_with_fallback(preferred: &str) -> (tokio::net::TcpListener, std::net::SocketAddr) {
    for c in &port_candidates(preferred) {
        if let Ok(l) =
            tokio::net::TcpListener::bind(c.parse::<std::net::SocketAddr>().expect("addr")).await
        {
            let addr = l.local_addr().expect("local_addr");
            return (l, addr);
        }
    }
    panic!("no free port available");
}

/// Windows: open a native application window (WebView2 — the same engine as
/// Asana/Figma desktop) showing the UI, and keep it alive until the user closes
/// it.
///
/// WebView2 requires the calling thread to be a COM STA apartment with a Win32
/// message pump, so this runs on the **main** thread (not a spawned one). The
/// tokio runtime that serves the HTTP API is moved onto a dedicated OS thread
/// before we enter the event loop. `event_loop.run` never returns — it calls
/// `std::process::exit` once the last window is destroyed, which is exactly the
/// desktop-app behavior we want (close the window → quit the app).
///
/// The server binds a tokio listener *inside* its own thread and reports the
/// address back over an mpsc channel, so the window can load the correct URL.
/// A dedicated OS thread running `block_on` is the proven pattern for tao/wry:
/// the runtime's workers handle connections while the main thread drives the
/// WebView2 event loop. (Binding via `std::net::TcpListener` + `from_std`, and
/// spawning axum directly on the runtime from the main thread, both left
/// hyper unable to read connections in testing.)
#[cfg(windows)]
fn run_native_window(rt: tokio::runtime::Runtime, app: Router, preferred: String) -> ! {
    // WebView2 stores its cache (EBWebView) next to the executable by default.
    // When installed under Program Files that folder is read-only, so creating
    // the webview fails silently and no window ever appears. Force the data
    // folder into the per-user data dir (writable) before the first WebView2 is
    // created — this must happen on the main thread, before any other thread.
    let wv_dir = data_dir().join("webview2");
    std::fs::create_dir_all(&wv_dir).ok();
    unsafe {
        std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", &wv_dir);
    }

    let (addr_tx, addr_rx) = std::sync::mpsc::channel();

    // Serve the API on a dedicated OS thread with its own runtime. `block_on`
    // from a non-worker thread is the correct way to run the server here: the
    // multi-thread runtime spawns worker threads that accept and handle
    // connections in the background while this thread just blocks.
    std::thread::Builder::new()
        .name("t2f-http".into())
        .spawn(move || {
            rt.block_on(async move {
                let mut listener = None;
                for c in &port_candidates(&preferred) {
                    if let Ok(l) = tokio::net::TcpListener::bind(c).await {
                        listener = Some(l);
                        break;
                    }
                }
                let listener = listener.expect("no free port available");
                let addr = listener.local_addr().expect("local_addr");
                tracing::info!("Todo2fast listening on http://{}", addr);
                let _ = addr_tx.send(addr); // unblock the main thread

                if let Err(e) = axum::serve(listener, app).await {
                    tracing::error!("server error: {e}");
                }
            });
        })
        .expect("spawn http thread");

    // Wait for the bound address so the window loads the right URL.
    let addr = addr_rx.recv().expect("bound address from server thread");
    let url = format!("http://{}/", addr);
    tracing::info!("native window loading: {url}");

    let event_loop = tao::event_loop::EventLoop::new();
    let window = tao::window::WindowBuilder::new()
        .with_title("Todo2fast")
        .with_inner_size(tao::dpi::LogicalSize::new(1280.0, 800.0))
        .build(&event_loop)
        .expect("failed to create native window");

    let _webview = wry::WebViewBuilder::new()
        .with_url(&url)
        .build(&window)
        .expect("failed to create WebView2");

    tracing::info!("native window ready: {url}");

    // The catch-all arm is required: the closure must handle every event so the
    // loop keeps running; only `Destroyed` triggers a quit.
    #[allow(clippy::single_match)]
    event_loop.run(move |event, _target, control_flow| match event {
        tao::event::Event::WindowEvent {
            event: tao::event::WindowEvent::Destroyed,
            ..
        } => {
            // Last window closed → quit the app (run() then process::exit).
            tracing::info!("window closed — shutting down");
            *control_flow = tao::event_loop::ControlFlow::Exit;
        }
        _ => {}
    });
}

/// Shared startup: per-user data dir, logging (stdout + file), database and the
/// app router. Returns everything `main` needs before binding a port.
fn bootstrap() -> (std::path::PathBuf, Arc<api::AppState>, Router) {
    let data = data_dir();
    std::fs::create_dir_all(&data).ok();

    // Logging: stdout (dev) + file in the per-user data dir (installed app —
    // this is where startup errors end up when no console is visible).
    let log_path = data.join("todo2fast.log");
    if let Ok(file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    {
        use tracing_subscriber::prelude::*;
        tracing_subscriber::registry()
            .with(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "todo2fast=info,tower_http=info".into()),
            )
            .with(tracing_subscriber::fmt::layer().with_writer(std::io::stdout))
            .with(
                tracing_subscriber::fmt::layer()
                    .with_ansi(false)
                    .with_writer(std::sync::Arc::new(file)),
            )
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "todo2fast=info,tower_http=info".into()),
            )
            .init();
    }

    let db = db_path();
    tracing::info!("database: {}", db.display());
    let state = Arc::new(api::AppState {
        db: crate::db::Db::open(&db).expect("failed to open database"),
        version: env!("CARGO_PKG_VERSION").to_string(),
    });

    // Frontend location: T2F_WEB_DIR (set by the installer), then dev layout,
    // then ./web next to the executable.
    let web_dir = std::env::var("T2F_WEB_DIR")
        .ok()
        .map(std::path::PathBuf::from)
        .or_else(|| std::path::Path::new("../frontend/dist").canonicalize().ok())
        .or_else(|| std::path::Path::new("web").canonicalize().ok());

    (data, state.clone(), build_router(state, web_dir))
}

#[cfg(windows)]
fn main() {
    let (_data, _state, app) = bootstrap();

    let preferred =
        std::env::var("T2F_ADDR").unwrap_or_else(|_| format!("0.0.0.0:{}", DEFAULT_PORT));

    // The tokio runtime is created here and handed to run_native_window, which
    // moves it onto a dedicated OS thread so the main thread can drive WebView2.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("build tokio runtime");

    tracing::info!("opening native window (preferred {preferred})");
    run_native_window(rt, app, preferred);
}

#[cfg(not(windows))]
#[tokio::main]
async fn main() {
    let (_data, _state, app) = bootstrap();

    let preferred =
        std::env::var("T2F_ADDR").unwrap_or_else(|_| format!("0.0.0.0:{}", DEFAULT_PORT));
    let (listener, addr) = bind_with_fallback(&preferred).await;
    tracing::info!("Todo2fast listening on http://{}", addr);

    println!("UI: http://{}/", addr);
    axum::serve(listener, app).await.expect("server error");
}
