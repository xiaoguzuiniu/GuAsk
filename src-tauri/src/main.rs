#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod llm;
mod selection;

use config::Config;
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use tauri::ipc::Channel;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, State, WindowEvent};

struct AppState {
    cfg: Mutex<Config>,
}

#[tauri::command]
fn get_config(state: State<AppState>) -> Config {
    state.cfg.lock().unwrap().clone()
}

#[tauri::command]
fn save_config(app: AppHandle, state: State<AppState>, cfg: Config) -> Result<(), String> {
    config::save(app.path().app_config_dir().map_err(|e| e.to_string())?, &cfg)?;
    *selection::TRIGGER.lock().unwrap() = cfg.trigger.clone();
    *state.cfg.lock().unwrap() = cfg;
    Ok(())
}

#[tauri::command]
async fn chat(state: State<'_, AppState>, messages: Vec<llm::Msg>, model: Option<String>, on_event: Channel<String>) -> Result<(), String> {
    let mut cfg = state.cfg.lock().unwrap().clone();
    if let Some(m) = model.filter(|m| !m.trim().is_empty()) {
        cfg.model = m;
    }
    llm::chat(cfg, messages, on_event).await
}

#[tauri::command]
fn set_ask_model(app: AppHandle, state: State<AppState>, model: String) -> Result<(), String> {
    let mut cfg = state.cfg.lock().unwrap();
    cfg.ask_model = model;
    config::save(app.path().app_config_dir().map_err(|e| e.to_string())?, &cfg)
}

#[tauri::command]
async fn list_models(cfg: Config) -> Result<Vec<String>, String> {
    llm::list_models(cfg).await
}

#[tauri::command]
async fn test_connection(cfg: Config) -> Result<(u128, String), String> {
    llm::test(cfg).await
}

fn open_settings(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![get_config, save_config, chat, list_models, test_connection, set_ask_model])
        .setup(|app| {
            let dir = app.path().app_config_dir()?;
            let cfg = config::load(dir);
            let first_run = cfg.api_key.is_empty();
            *selection::TRIGGER.lock().unwrap() = cfg.trigger.clone();
            app.manage(AppState { cfg: Mutex::new(cfg) });

            // 托盘
            let enable = CheckMenuItem::with_id(app, "enable", "启用划词", true, true, None::<&str>)?;
            let settings = MenuItem::with_id(app, "settings", "设置", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let sep = PredefinedMenuItem::separator(app)?;
            let menu = Menu::with_items(app, &[&enable, &settings, &sep, &quit])?;
            let enable_c = enable.clone();
            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("GuAsk 划词翻译/提问")
                .menu(&menu)
                .on_menu_event(move |app, e| match e.id().as_ref() {
                    "enable" => {
                        let on = enable_c.is_checked().unwrap_or(true);
                        selection::ENABLED.store(on, Ordering::Relaxed);
                    }
                    "settings" => open_settings(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            // 关闭窗口只隐藏，程序常驻托盘
            for label in ["popup", "settings"] {
                if let Some(w) = app.get_webview_window(label) {
                    let w2 = w.clone();
                    w.on_window_event(move |e| {
                        if let WindowEvent::CloseRequested { api, .. } = e {
                            api.prevent_close();
                            let _ = w2.hide();
                        }
                    });
                }
            }

            selection::start(app.handle().clone());
            if first_run {
                open_settings(app.handle());
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running GuAsk");
}
