use mtg_engine::http::serve;
use std::thread;

fn main() -> std::io::Result<()> {
    // The server is frequently launched as a compiled executable rather than
    // through Cargo or Vite. Load the repository configuration before any
    // process-wide service (history, analytics, authentication bridge) starts.
    dotenvy::dotenv().ok();
    let addr = std::env::var("MTG_ENGINE_ADDR").unwrap_or_else(|_| "127.0.0.1:8787".to_string());
    let ui_addr = std::env::var("MTG_UI_ADDR").unwrap_or_else(|_| "127.0.0.1:5173".to_string());
    if !ui_addr.is_empty() && ui_addr != addr {
        thread::spawn(move || {
            if let Err(error) = serve(&ui_addr) {
                eprintln!("UI server failed on {ui_addr}: {error}");
            }
        });
    }
    serve(&addr)
}
