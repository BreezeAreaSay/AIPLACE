//! Triad Canvas desktop: a Tauri window around the embedded room server. The server binds
//! 127.0.0.1 on the configured port (or a free one), the window navigates to it, and the app
//! lives in the tray so the room keeps breathing when the window is closed.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    net::SocketAddr,
    sync::mpsc,
    time::{Duration, Instant},
};

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, WindowEvent,
};
use tauri_plugin_notification::NotificationExt;
use triad_canvas_backend::{config, Config, LlmSettings};

struct Backend {
    url: String,
}

fn start_server() -> mpsc::Receiver<anyhow::Result<SocketAddr>> {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("triad-server".into())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = tx.send(Err(e.into()));
                    return;
                }
            };
            rt.block_on(async move {
                // Desktop never exposes the server: loopback only, keep the user's PORT if free.
                std::env::set_var("HOST", "127.0.0.1");
                std::env::set_var("TRIAD_DESKTOP", "1");
                let cfg = match Config::from_env() {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx.send(Err(e));
                        return;
                    }
                };
                let settings = LlmSettings::from_env();
                let listener = match triad_canvas_backend::bind("127.0.0.1", cfg.port).await {
                    Ok(l) => l,
                    Err(e) => {
                        let _ = tx.send(Err(e));
                        return;
                    }
                };
                let addr = listener.local_addr().expect("local addr");
                let _ = tx.send(Ok(addr));
                if let Err(e) = triad_canvas_backend::serve(listener, cfg, settings).await {
                    tracing::error!(error = %e, "embedded server stopped");
                }
            });
        })
        .expect("spawn server thread");
    rx
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Follow the embedded server's event stream; when the window is hidden and an inhabitant
/// speaks (heartbeat or a round the player left running), show a native notification.
fn watch_events(app: AppHandle, url: String) {
    tauri::async_runtime::spawn(async move {
        use futures::StreamExt;
        loop {
            let client = reqwest::Client::new();
            let Ok(resp) = client.get(format!("{url}api/events")).send().await else {
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            };
            let mut stream = resp.bytes_stream();
            let mut buf = String::new();
            while let Some(Ok(chunk)) = stream.next().await {
                buf.push_str(&String::from_utf8_lossy(&chunk));
                while let Some(pos) = buf.find('\n') {
                    let line = buf[..pos].trim().to_string();
                    buf.drain(..=pos);
                    let Some(data) = line.strip_prefix("data:") else { continue };
                    let Ok(v) = serde_json::from_str::<serde_json::Value>(data.trim()) else { continue };
                    if v.get("type").and_then(|t| t.as_str()) != Some("message") {
                        continue;
                    }
                    let sender = v.pointer("/message/sender_id").and_then(|s| s.as_str()).unwrap_or("");
                    if sender == "user" || sender == "system" {
                        continue;
                    }
                    let visible = app.get_webview_window("main").and_then(|w| w.is_visible().ok()).unwrap_or(true);
                    let focused = app.get_webview_window("main").and_then(|w| w.is_focused().ok()).unwrap_or(false);
                    if visible && focused {
                        continue;
                    }
                    let content = v.pointer("/message/content").and_then(|c| c.as_str()).unwrap_or("");
                    let who = match sender {
                        "physicist" => "Аркадий Борисович",
                        "lit_teacher" => "Софья Генриховна",
                        "graffiti" => "Кэп",
                        other => other,
                    };
                    let body: String = content.chars().take(180).collect();
                    let _ = app.notification().builder().title(who).body(body).show();
                }
            }
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    });
}

fn post_tick(app: &AppHandle) {
    let url = app.state::<Backend>().url.clone();
    tauri::async_runtime::spawn(async move {
        let client = reqwest::Client::new();
        if let Err(e) = client.post(format!("{url}api/tick")).send().await {
            tracing::warn!(error = %e, "tray tick failed");
        }
    });
}

fn main() {
    config::load_env_files();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,sqlx=warn,tower_http=warn".into()),
        )
        .init();

    let rx = start_server();

    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .setup(move |app| {
            let started = Instant::now();
            let addr = loop {
                match rx.recv_timeout(Duration::from_secs(20)) {
                    Ok(Ok(a)) => break a,
                    Ok(Err(e)) => return Err(format!("embedded server failed to start: {e:#}").into()),
                    Err(_) if started.elapsed() > Duration::from_secs(20) => {
                        return Err("embedded server did not start in 20s".into())
                    }
                    Err(_) => continue,
                }
            };
            let url = format!("http://{addr}/");
            tracing::info!(%url, "embedded server ready");
            app.manage(Backend { url: url.clone() });

            if let Some(w) = app.get_webview_window("main") {
                w.navigate(url::Url::parse(&url)?)?;
            }
            watch_events(app.handle().clone(), url.clone());

            // Tray: the room keeps living when the window is closed.
            let open = MenuItem::with_id(app, "open", "Открыть комнату", true, None::<&str>)?;
            let tick = MenuItem::with_id(app, "tick", "🎲 Автономный тик", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Выход", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &tick, &PredefinedMenuItem::separator(app)?, &quit])?;
            let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?;
            TrayIconBuilder::new()
                .icon(icon)
                .tooltip("Triad Canvas — комната живёт")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, ev| match ev.id().as_ref() {
                    "open" => show_main(app),
                    "tick" => post_tick(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, ev| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                        show_main(tray.app_handle());
                    }
                })
                .build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                // Hide instead of quitting; "Выход" lives in the tray menu.
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Triad Canvas");
}
