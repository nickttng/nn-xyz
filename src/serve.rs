//! Dev server: build, serve `dist/` on localhost, rebuild and reload on save.
//!
//! Live reload is a tiny external script served only by this server (never written
//! to `dist/`), so it works under the site's strict Content-Security-Policy.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use axum::Router;
use axum::extract::{Query, State};
use axum::http::header;
use axum::response::IntoResponse;
use axum::routing::get;
use notify::{RecursiveMode, Watcher};
use serde::Deserialize;
use tokio::sync::watch;
use tower_http::services::ServeDir;

/// Loopback only: the dev server is never reachable from the network.
const ADDR: ([u8; 4], u16) = ([127, 0, 0, 1], 4000);

const RELOAD_JS: &str = r#"(async () => {
  let since = "";
  for (;;) {
    try {
      const res = await fetch("/__kwaj/version?since=" + since);
      const version = await res.text();
      if (since && version !== since) location.reload();
      since = version;
    } catch {
      await new Promise((r) => setTimeout(r, 1000));
    }
  }
})();
"#;

/// Dev-only theme switcher (auto / light / dark). The choice persists in this browser only.
const THEME_JS: &str = r#"(() => {
  const themes = ["auto", "light", "dark"];
  const root = document.documentElement;
  const get = (k) => { try { return localStorage.getItem(k); } catch { return null; } };
  const set = (k, v) => { try { localStorage.setItem(k, v); } catch {} };

  const apply = () => {
    const theme = get("kwaj-theme") || "auto";
    if (theme === "auto") delete root.dataset.theme; else root.dataset.theme = theme;
    for (const b of document.querySelectorAll(".kwaj-dev button")) {
      b.setAttribute("aria-pressed", String(root.dataset[b.dataset.key] === b.dataset.value
        || (b.dataset.key === "theme" && b.dataset.value === "auto" && !root.dataset.theme)));
    }
  };

  const group = (label, key, values) => {
    const g = document.createElement("div");
    g.setAttribute("role", "group");
    g.setAttribute("aria-label", label);
    for (const value of values) {
      const b = document.createElement("button");
      b.type = "button";
      b.textContent = value;
      b.dataset.key = key;
      b.dataset.value = value;
      b.addEventListener("click", () => { set("kwaj-" + key, value); apply(); });
      g.append(b);
    }
    return g;
  };

  const panel = document.createElement("div");
  panel.className = "kwaj-dev";
  panel.append(group("Theme", "theme", themes));
  document.body.append(panel);
  apply();
})();
"#;

const DEV_CSS: &str = r#".kwaj-dev {
  position: fixed; inset-block-end: 1rem; inset-inline-end: 1rem; z-index: 100;
  display: flex; gap: 0.5rem; padding: 0.375rem;
  font: 500 12px/1 var(--font-mono, monospace);
  background: var(--surface); color: var(--ink);
  border: 1px solid var(--rule); border-radius: 999px;
  box-shadow: 0 4px 16px rgb(0 0 0 / 0.12);
}
.kwaj-dev [role="group"] { display: flex; gap: 2px; }
.kwaj-dev [role="group"] + [role="group"] { border-inline-start: 1px solid var(--rule); padding-inline-start: 0.5rem; }
.kwaj-dev button {
  font: inherit; color: inherit; background: none; border: 0; cursor: pointer;
  padding: 0.4rem 0.6rem; border-radius: 999px;
}
.kwaj-dev button:hover { background: var(--rule); }
.kwaj-dev button[aria-pressed="true"] { background: var(--ink); color: var(--paper); }
"#;

pub struct Paths {
    pub out: PathBuf,
    /// Folders that trigger a rebuild when anything inside changes.
    pub watch: Vec<PathBuf>,
}

#[derive(Deserialize)]
struct Since {
    since: Option<String>,
}

/// Long-poll: answer as soon as the build version differs from `since`,
/// or after 25 seconds so the request never hangs forever.
async fn version(State(rx): State<watch::Receiver<u64>>, Query(q): Query<Since>) -> String {
    let mut rx = rx.clone();
    let current = rx.borrow().to_string();
    if q.since.as_deref() == Some(current.as_str()) {
        let _ = tokio::time::timeout(Duration::from_secs(25), rx.changed()).await;
    }
    rx.borrow().to_string()
}

async fn reload_js() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/javascript")], RELOAD_JS)
}

async fn theme_js() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/javascript")], THEME_JS)
}

async fn dev_css() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/css")], DEV_CSS)
}

/// Run until Ctrl-C. `rebuild` is called once at start and after every change.
pub fn run(paths: Paths, rebuild: impl Fn() + Send + 'static) -> anyhow::Result<()> {
    rebuild();

    let (version_tx, version_rx) = watch::channel(0u64);

    // File watcher on a plain thread; bursts of events (editors save in several steps)
    // are collapsed into one rebuild.
    let (event_tx, event_rx) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(event_tx)?;
    for dir in &paths.watch {
        if dir.exists() {
            watcher.watch(dir, RecursiveMode::Recursive)?;
        }
    }
    std::thread::spawn(move || {
        let _watcher = watcher;
        while event_rx.recv().is_ok() {
            while event_rx.recv_timeout(Duration::from_millis(100)).is_ok() {}
            rebuild();
            version_tx.send_modify(|v| *v += 1);
        }
    });

    let app = Router::new()
        .route("/__kwaj/version", get(version))
        .route("/__kwaj/reload.js", get(reload_js))
        .route("/__kwaj/theme.js", get(theme_js))
        .route("/__kwaj/dev.css", get(dev_css))
        .with_state(version_rx)
        .fallback_service(ServeDir::new(&paths.out));

    let addr = SocketAddr::from(ADDR);
    println!("serving http://{addr} (Ctrl-C to stop)");

    tokio::runtime::Runtime::new()?.block_on(async {
        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, app).await?;
        Ok(())
    })
}
