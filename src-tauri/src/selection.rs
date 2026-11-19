//! 全局钩子：记录划词手势，按下触发键（默认双击 Ctrl）后才取词并弹窗
use rdev::{listen, simulate, Button, EventType, Key};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition};

pub static ENABLED: AtomicBool = AtomicBool::new(true);
/// 触发方式：ctrl2 / alt2 / shift2（双击修饰键）或 auto（划词即弹）
pub static TRIGGER: Mutex<String> = Mutex::new(String::new());
/// 划词小图标对应的 (位置, 剪贴板序号)
static BUBBLE: Mutex<Option<((f64, f64), u32)>> = Mutex::new(None);
static BUBBLE_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

fn trigger_keys(t: &str) -> Option<[Key; 2]> {
    match t {
        "alt2" => Some([Key::Alt, Key::AltGr]),
        "shift2" => Some([Key::ShiftLeft, Key::ShiftRight]),
        "auto" | "icon" => None,
        _ => Some([Key::ControlLeft, Key::ControlRight]),
    }
}

pub fn start(app: AppHandle) {
    thread::spawn(move || {
        let mut pos = (0.0, 0.0);
        let mut down_pos = (0.0, 0.0);
        let mut last_up = Instant::now() - Duration::from_secs(10);
        let mut last_up_pos = (0.0, 0.0);
        let mut seq_at_press = 0u32;
        // 最近一次划词手势：(时间, 按下时剪贴板序号)
        let mut last_sel: Option<(Instant, u32)> = None;
        // 双击修饰键检测
        let mut mod_down = false; // 修饰键当前按下且期间无其它键
        let mut last_tap: Option<Instant> = None;
        let res = listen(move |ev| match ev.event_type {
            EventType::MouseMove { x, y } => pos = (x, y),
            EventType::ButtonPress(Button::Left) => {
                down_pos = pos;
                seq_at_press = clip_seq();
                last_tap = None;
                if !in_window(&app, "bubble", pos) {
                    hide_bubble(&app);
                }
            }
            EventType::ButtonRelease(Button::Left) => {
                let dragged = dist(pos, down_pos) > 8.0;
                let dbl = last_up.elapsed() < Duration::from_millis(400) && dist(pos, last_up_pos) < 6.0;
                last_up = Instant::now();
                last_up_pos = pos;
                if !ENABLED.load(Ordering::Relaxed) || !(dragged || dbl) || in_window(&app, "popup", pos) || in_window(&app, "bubble", pos) {
                    return;
                }
                last_sel = Some((Instant::now(), seq_at_press));
                let trig = TRIGGER.lock().unwrap().clone();
                if trig == "icon" {
                    show_bubble(&app, pos, seq_at_press);
                } else if trig == "auto" {
                    let app = app.clone();
                    let p = pos;
                    let seq = seq_at_press;
                    thread::spawn(move || on_select(app, p, Some(seq)));
                }
            }
            EventType::Wheel { .. } => hide_bubble(&app),
            EventType::KeyPress(k) => {
                hide_bubble(&app);
                let trig = TRIGGER.lock().unwrap().clone();
                match trigger_keys(&trig) {
                    Some(ks) if ks.contains(&k) => {
                        if !mod_down {
                            mod_down = true;
                        }
                    }
                    _ => {
                        // 中间按了别的键（如 Ctrl+C），作废
                        mod_down = false;
                        last_tap = None;
                    }
                }
            }
            EventType::KeyRelease(k) => {
                let trig = TRIGGER.lock().unwrap().clone();
                let Some(ks) = trigger_keys(&trig) else { return };
                if !ks.contains(&k) || !mod_down {
                    return;
                }
                mod_down = false;
                let now = Instant::now();
                match last_tap {
                    Some(t) if now.duration_since(t) < Duration::from_millis(400) => {
                        last_tap = None;
                        if !ENABLED.load(Ordering::Relaxed) {
                            return;
                        }
                        // 终端：仅当最近 15 秒内有划词手势时，才用“选中即复制”的剪贴板
                        let seq = last_sel.filter(|(t, _)| t.elapsed() < Duration::from_secs(15)).map(|(_, s)| s);
                        let app = app.clone();
                        let p = pos;
                        thread::spawn(move || on_select(app, p, seq));
                    }
                    _ => last_tap = Some(now),
                }
            }
            _ => {}
        });
        if let Err(e) = res {
            eprintln!("input hook error: {:?}", e);
        }
    });
}
fn in_window(app: &AppHandle, label: &str, p: (f64, f64)) -> bool {
    let Some(w) = app.get_webview_window(label) else { return false };
    let visible = match w.hwnd() {
        Ok(h) => unsafe { windows::Win32::UI::WindowsAndMessaging::IsWindowVisible(windows::Win32::Foundation::HWND(h.0 as _)).as_bool() },
        Err(_) => false,
    };
    if !visible {
        return false;
    }
    match (w.outer_position(), w.outer_size()) {
        (Ok(o), Ok(s)) => {
            p.0 >= o.x as f64 && p.0 <= (o.x + s.width as i32) as f64 && p.1 >= o.y as f64 && p.1 <= (o.y + s.height as i32) as f64
        }
        _ => false,
    }
}

fn clip_seq() -> u32 {
    unsafe { windows::Win32::System::DataExchange::GetClipboardSequenceNumber() }
}

/// 前台窗口所属进程的 exe 名（小写）
fn fg_exe(pid: u32) -> String {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
    unsafe {
        let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return String::new() };
        let mut buf = [0u16; 520];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, windows::core::PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(h);
        if !ok {
            return String::new();
        }
        let path = String::from_utf16_lossy(&buf[..len as usize]).to_lowercase();
        path.rsplit('\\').next().unwrap_or("").to_string()
    }
}

/// “选中即复制”类终端：等待剪贴板序号变化后读取（不发 Ctrl+C）
fn wait_auto_copy(seq_at_press: u32) -> Option<String> {
    for _ in 0..12 {
        if clip_seq() != seq_at_press {
            thread::sleep(Duration::from_millis(30));
            return arboard::Clipboard::new().ok()?.get_text().ok().filter(|t| !t.trim().is_empty());
        }
        thread::sleep(Duration::from_millis(25));
    }
    None
}

#[derive(PartialEq)]
enum Fg {
    Own,
    Terminal,
    Normal,
}

const TERM_EXES: &[&str] = &[
    "mobaxterm.exe", "mobaxterm_personal.exe", "putty.exe", "kitty.exe", "mintty.exe", "xshell.exe",
    "securecrt.exe", "termius.exe", "tabby.exe", "wezterm-gui.exe", "alacritty.exe", "conemu64.exe",
    "conemu.exe", "windowsterminal.exe", "cmd.exe", "powershell.exe", "pwsh.exe", "finalshell.exe",
];

fn foreground_kind() -> Fg {
    use windows::Win32::UI::WindowsAndMessaging::{GetClassNameW, GetForegroundWindow, GetWindowThreadProcessId};
    unsafe {
        let hwnd = GetForegroundWindow();
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == std::process::id() {
            return Fg::Own;
        }
        let mut buf = [0u16; 256];
        let n = GetClassNameW(hwnd, &mut buf) as usize;
        let class = String::from_utf16_lossy(&buf[..n]);
        let exe = fg_exe(pid);
        if matches!(class.as_str(), "ConsoleWindowClass" | "CASCADIA_HOSTING_WINDOW_CLASS" | "mintty" | "PuTTY" | "VirtualConsoleClass")
            || TERM_EXES.contains(&exe.as_str())
            || exe.starts_with("mobaxterm")
        {
            Fg::Terminal
        } else {
            Fg::Normal
        }
    }
}

/// 通过 UI Automation 的 TextPattern 直接读取选中文本：不发按键、不碰剪贴板。
/// Windows Terminal、conhost、记事本、Office、VS Code 等都支持。
fn uia_selection() -> Option<String> {
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
    use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation, IUIAutomationTextPattern, UIA_TextPatternId};
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let uia: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
        let el = uia.GetFocusedElement().ok()?;
        let pat: IUIAutomationTextPattern = el.GetCurrentPatternAs(UIA_TextPatternId).ok()?;
        let ranges = pat.GetSelection().ok()?;
        let mut s = String::new();
        for i in 0..ranges.Length().ok()? {
            if let Ok(r) = ranges.GetElement(i) {
                if let Ok(t) = r.GetText(-1) {
                    s.push_str(&t.to_string());
                }
            }
        }
        let s = s.trim().to_string();
        (!s.is_empty()).then_some(s)
    }
}

fn key(ev: EventType) {
    let _ = simulate(&ev);
    thread::sleep(Duration::from_millis(15));
}

fn copy_selection() -> Option<String> {
    let mut cb = arboard::Clipboard::new().ok()?;
    let old = cb.get_text().ok();
    let _ = cb.clear();
    key(EventType::KeyPress(Key::ControlLeft));
    key(EventType::KeyPress(Key::KeyC));
    key(EventType::KeyRelease(Key::KeyC));
    key(EventType::KeyRelease(Key::ControlLeft));
    let mut text = None;
    for _ in 0..12 {
        thread::sleep(Duration::from_millis(25));
        if let Ok(t) = cb.get_text() {
            if !t.trim().is_empty() {
                text = Some(t);
                break;
            }
        }
    }
    // 恢复用户原来的剪贴板（目前只恢复文本）
    if let Some(o) = old {
        let _ = cb.set_text(o);
    }
    text
}

pub fn on_select(app: AppHandle, p: (f64, f64), seq_at_press: Option<u32>) {
    thread::sleep(Duration::from_millis(60));
    let text = match foreground_kind() {
        Fg::Own => return,
        // 终端里绝不发 Ctrl+C（没选中时会中断程序）：先 UIA，再看是否“选中即复制”
        Fg::Terminal => uia_selection().or_else(|| seq_at_press.and_then(wait_auto_copy)),
        // 其它程序优先 UIA，读不到再退回 Ctrl+C
        Fg::Normal => uia_selection().or_else(copy_selection),
    };
    let Some(text) = text else { return };
    let text = text.trim().to_string();
    if text.is_empty() || text.chars().count() > 8000 {
        return;
    }
    show_popup(&app, p, text);
}

pub fn show_popup(app: &AppHandle, p: (f64, f64), text: String) {
    let Some(w) = app.get_webview_window("popup") else { return };
    let (mut x, mut y) = (p.0 as i32 + 12, p.1 as i32 + 18);
    if let (Ok(Some(m)), Ok(s)) = (w.monitor_from_point(p.0, p.1), w.outer_size()) {
        let mp = m.position();
        let ms = m.size();
        x = x.min(mp.x + ms.width as i32 - s.width as i32 - 8).max(mp.x);
        if y + s.height as i32 > mp.y + ms.height as i32 - 40 {
            y = p.1 as i32 - s.height as i32 - 12;
        }
        y = y.max(mp.y);
    }
    let _ = w.set_position(PhysicalPosition::new(x, y));
    let _ = w.show();
    let _ = w.set_focus();
    let _ = w.emit_to("popup", "selection", text);
}

/// 在鼠标旁显示小图标（不抢焦点，保持原程序的选区）
fn show_bubble(app: &AppHandle, p: (f64, f64), seq: u32) {
    let Some(w) = app.get_webview_window("bubble") else { return };
    *BUBBLE.lock().unwrap() = Some((p, seq));
    let gen = BUBBLE_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    let _ = w.set_position(PhysicalPosition::new(p.0 as i32 + 10, p.1 as i32 + 14));
    if let Ok(h) = w.hwnd() {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::*;
        unsafe {
            let hwnd = HWND(h.0 as _);
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | (WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0 | WS_EX_TOPMOST.0) as isize);
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            let _ = SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }
    let app = app.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(3));
        if BUBBLE_GEN.load(Ordering::SeqCst) == gen {
            hide_bubble(&app);
        }
    });
}

fn hide_bubble(app: &AppHandle) {
    BUBBLE_GEN.fetch_add(1, Ordering::SeqCst);
    // 直接用 Win32 隐藏：窗口是用 ShowWindow 显示的，Tauri 的状态不可靠
    if let Some(w) = app.get_webview_window("bubble") {
        if let Ok(h) = w.hwnd() {
            use windows::Win32::Foundation::HWND;
            use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};
            unsafe {
                let _ = ShowWindow(HWND(h.0 as _), SW_HIDE);
            }
        }
    }
}

/// 小图标被点击：取词并弹窗
pub fn bubble_click(app: AppHandle) {
    hide_bubble(&app);
    if let Some((p, seq)) = BUBBLE.lock().unwrap().take() {
        thread::spawn(move || on_select(app, p, Some(seq)));
    }
}

pub fn cursor_pos() -> (f64, f64) {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let mut pt = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut pt);
    }
    (pt.x as f64, pt.y as f64)
}

/// 不划词，直接打开提问
pub fn open_blank(app: &AppHandle) {
    let p = cursor_pos();
    show_popup(app, p, String::new());
}