

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cachex_control::{api, state::AppState};
use tokio::net::TcpListener;

fn find_exe(root: &Path, name: &str) -> PathBuf {
    let exe = if cfg!(target_os = "windows") {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    for profile in ["release", "debug"] {
        let candidate = root.join("target").join(profile).join(&exe);
        if candidate.exists() {
            return candidate;
        }
    }
    root.join("target").join("release").join(exe)
}

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned()
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "CacheX control service

Usage:
  cachex-control [--root DIR] [--port N] [--server PATH] [--bench PATH]

Options:
  --root DIR     workspace root (default: current directory)
  --port N       control API port (default: 9100)
  --server PATH  path to the cachex-server binary (default: <root>/target/<profile>/)
  --bench PATH   path to the cachex-bench binary (default: <root>/target/<profile>/)
  --help         show this help"
        );
        return Ok(());
    }

    let cwd = std::env::current_dir()?;
    let root = arg_value(&args, "--root").map(PathBuf::from).unwrap_or(cwd);
    let port: u16 = arg_value(&args, "--port").and_then(|s| s.parse().ok()).unwrap_or(9100);
    let server_exe = arg_value(&args, "--server").map(PathBuf::from).unwrap_or_else(|| find_exe(&root, "cachex-server"));
    let bench_exe = arg_value(&args, "--bench").map(PathBuf::from).unwrap_or_else(|| find_exe(&root, "cachex-bench"));

    let state = AppState::new(root.clone(), server_exe, bench_exe);
    std::fs::create_dir_all(&state.control_dir)?;

    let listener = TcpListener::bind(("127.0.0.1", port)).await?;
    println!(
        "control service listening on http://127.0.0.1:{port}  (root: {})",
        root.display()
    );
    println!(
        "  server binary: {} ({})",
        state.server_exe.display(),
        if state.server_exe.exists() { "present" } else { "MISSING" }
    );
    println!("  SSE stream:   http://127.0.0.1:{port}/control/events");

    let app = api::router(Arc::clone(&state));
    axum::serve(listener, app).await
}