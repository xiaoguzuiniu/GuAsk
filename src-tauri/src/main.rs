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
    let old_hotkey = state.cfg.lock().unwrap().hotkey.clone();
    if old_hotkey != cfg.hotkey {
        register_hotkey(&app, &cfg.hotkey)?;
    }
    config::save(app.path().app_config_dir().map_err(|e| e.to_string())?, &cfg)?;
    *selection::TRIGGER.lock().unwrap() = cfg.trigger.clone();
    *state.cfg.lock().unwrap() = cfg;
    Ok(())
}

fn register_hotkey(app: &AppHandle, hk: &str) -> Result<(), String> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let hk = hk.trim();
    if hk.is_empty() {
        return Ok(());
    }
    gs.register(hk).map_err(|e| format!("快捷键 {hk} 注册失败（可能格式不对或已被占用）：{e}"))
}

#[tauri::command]
fn get_autostart(app: AppHandle) -> bool {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
fn set_autostart(app: AppHandle, on: bool) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    let al = app.autolaunch();
    if on { al.enable() } else { al.disable() }.map_err(|e| e.to_string())
}

#[tauri::command]
fn stop_chat() {
    llm::stop();
}

#[tauri::command]
fn bubble_click(app: AppHandle) {
    selection::bubble_click(app);
}

/// 弹窗大小变化后记住（逻辑像素）
#[tauri::command]
fn save_popup_size(app: AppHandle, state: State<AppState>, w: f64, h: f64) -> Result<(), String> {
    let mut cfg = state.cfg.lock().unwrap();
    if (cfg.popup_w - w).abs() < 1.0 && (cfg.popup_h - h).abs() < 1.0 {
        return Ok(());
    }
    cfg.popup_w = w;
    cfg.popup_h = h;
    config::save(app.path().app_config_dir().map_err(|e| e.to_string())?, &cfg)
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
        .plugin(tauri_plugin_single_instance::init(|app, _, _| open_settings(app)))
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _sc, ev| {
                    if ev.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        selection::open_blank(app);
                    }
                })
                .build(),
        )
        // 关闭窗口只隐藏，程序常驻托盘
        .on_window_event(|w, e| {
            if let WindowEvent::CloseRequested { api, .. } = e {
                api.prevent_close();
                let _ = w.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![get_config, save_config, chat, list_models, test_connection, set_ask_model, get_autostart, set_autostart, bubble_click, save_popup_size, stop_chat])
        .setup(|app| {
            let dir = app.path().app_config_dir()?;
            let cfg = config::load(dir);
            let first_run = cfg.api_key.is_empty();
            *selection::TRIGGER.lock().unwrap() = cfg.trigger.clone();
            let hotkey = cfg.hotkey.clone();
            if let Some(w) = app.get_webview_window("popup") {
                let _ = w.set_size(tauri::LogicalSize::new(cfg.popup_w.max(320.0), cfg.popup_h.max(220.0)));
            }
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


            if let Err(e) = register_hotkey(app.handle(), &hotkey) {
                eprintln!("{e}");
            }
            selection::start(app.handle().clone());
            if first_run {
                open_settings(app.handle());
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building GuAsk")
        .run(|_app, e| {
            // 只有托盘“退出”(带 code) 才真正退出
            if let tauri::RunEvent::ExitRequested { code: None, api, .. } = e {
                api.prevent_exit();
            }
        });
}
