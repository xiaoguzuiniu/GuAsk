#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod history;
mod shot;
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
    let (oh, os) = { let c = state.cfg.lock().unwrap(); (c.hotkey.clone(), c.shot_hotkey.clone()) };
    if oh != cfg.hotkey || os != cfg.shot_hotkey {
        register_hotkey(&app, &cfg.hotkey, &cfg.shot_hotkey)?;
    }
    config::save(app.path().app_config_dir().map_err(|e| e.to_string())?, &cfg)?;
    *selection::TRIGGER.lock().unwrap() = cfg.trigger.clone();
    *state.cfg.lock().unwrap() = cfg;
    Ok(())
}

static SHOT_KEY: Mutex<Option<tauri_plugin_global_shortcut::Shortcut>> = Mutex::new(None);

fn register_hotkey(app: &AppHandle, hk: &str, shk: &str) -> Result<(), String> {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    *SHOT_KEY.lock().unwrap() = None;
    let mut errs = vec![];
    for (k, is_shot) in [(hk.trim(), false), (shk.trim(), true)] {
        if k.is_empty() {
            continue;
        }
        match k.parse::<Shortcut>() {
            Ok(sc) => match gs.register(sc) {
                Ok(_) => {
                    if is_shot {
                        *SHOT_KEY.lock().unwrap() = Some(sc);
                    }
                }
                Err(e) => errs.push(format!("快捷键 {k} 注册失败（可能已被占用）：{e}")),
            },
            Err(e) => errs.push(format!("快捷键 {k} 格式不对：{e}")),
        }
    }
    if errs.is_empty() { Ok(()) } else { Err(errs.join("；")) }
}

#[tauri::command]
fn shot_done(app: AppHandle, x: f64, y: f64, w: f64, h: f64) -> Result<(), String> {
    shot::done(&app, x, y, w, h)
}

#[tauri::command]
fn shot_cancel(app: AppHandle) {
    shot::cancel(&app);
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
fn history_save(state: State<AppState>, db: State<history::Db>, item: history::Item) -> Result<i64, String> {
    if !state.cfg.lock().unwrap().history_enabled {
        return Ok(0);
    }
    history::save(&db, item)
}

#[derive(serde::Serialize)]
struct Page {
    items: Vec<history::Item>,
    total: i64,
}

#[tauri::command]
fn history_list(db: State<history::Db>, q: String, mode: String, offset: i64, limit: i64) -> Result<Page, String> {
    let (items, total) = history::list(&db, q.trim(), &mode, offset, limit)?;
    Ok(Page { items, total })
}

#[tauri::command]
fn history_delete(db: State<history::Db>, id: i64) -> Result<(), String> {
    history::delete(&db, id)
}

#[tauri::command]
fn history_open(app: tauri::AppHandle, item: history::Item) {
    selection::open_history(&app, item);
}

#[tauri::command]
fn history_clear(db: State<history::Db>) -> Result<(), String> {
    history::clear(&db)
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
                .with_handler(|app, sc, ev| {
                    if ev.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        if SHOT_KEY.lock().unwrap().as_ref() == Some(sc) {
                            shot::start(app);
                        } else {
                            selection::open_blank(app);
                        }
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
        .invoke_handler(tauri::generate_handler![get_config, save_config, chat, list_models, test_connection, set_ask_model, get_autostart, set_autostart, bubble_click, save_popup_size, stop_chat, history_save, history_list, history_delete, history_clear, history_open, shot_done, shot_cancel])
        .setup(|app| {
            let dir = app.path().app_config_dir()?;
            app.manage(history::open(&dir)?);
            let cfg = config::load(dir);
            let first_run = cfg.api_key.is_empty();
            *selection::TRIGGER.lock().unwrap() = cfg.trigger.clone();
            let hotkey = cfg.hotkey.clone();
            let shot_hotkey = cfg.shot_hotkey.clone();
            if let Some(w) = app.get_webview_window("popup") {
                let _ = w.set_size(tauri::LogicalSize::new(cfg.popup_w.max(320.0), cfg.popup_h.max(220.0)));
            }
            app.manage(AppState { cfg: Mutex::new(cfg) });

            // 托盘
            let enable = CheckMenuItem::with_id(app, "enable", "启用划词", true, true, None::<&str>)?;
            let auto_on = {
                use tauri_plugin_autostart::ManagerExt;
                app.autolaunch().is_enabled().unwrap_or(false)
            };
            let autostart = CheckMenuItem::with_id(app, "autostart", "开机自启", true, auto_on, None::<&str>)?;
            let ask = MenuItem::with_id(app, "ask", "快速提问", true, None::<&str>)?;
            let shotm = MenuItem::with_id(app, "shot", "截图提问", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let sep = PredefinedMenuItem::separator(app)?;
            let sep2 = PredefinedMenuItem::separator(app)?;
            let menu = Menu::with_items(app, &[&enable, &autostart, &sep, &ask, &shotm, &sep2, &quit])?;
            let enable_c = enable.clone();
            let auto_c = autostart.clone();
            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("GuAsk 划词翻译/提问")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_tray_icon_event(|tray, e| {
                    if let tauri::tray::TrayIconEvent::Click { button: tauri::tray::MouseButton::Left, button_state: tauri::tray::MouseButtonState::Up, .. } = e {
                        open_settings(tray.app_handle());
                    }
                })
                .on_menu_event(move |app, e| match e.id().as_ref() {
                    "enable" => {
                        let on = enable_c.is_checked().unwrap_or(true);
                        selection::ENABLED.store(on, Ordering::Relaxed);
                    }
                    "autostart" => {
                        let on = auto_c.is_checked().unwrap_or(false);
                        if set_autostart(app.clone(), on).is_err() {
                            let _ = auto_c.set_checked(!on);
                        }
                    }
                    "ask" => selection::open_blank(app),
                    "shot" => shot::start(app),
                    "settings" => open_settings(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;


            if let Err(e) = register_hotkey(app.handle(), &hotkey, &shot_hotkey) {
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
