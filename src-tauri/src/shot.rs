//! 截图提问：全屏透明遮罩框选区域 → GDI 截取该区域 → PNG 发给弹窗
use base64::Engine;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

/// 打开框选遮罩，覆盖所有显示器
pub fn start(app: &AppHandle) {
    if let Some(p) = app.get_webview_window("popup") {
        let _ = p.hide();
    }
    let Some(w) = app.get_webview_window("shot") else { return };
    let (x, y, cw, ch) = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    };
    let _ = w.set_position(PhysicalPosition::new(x, y));
    let _ = w.set_size(PhysicalSize::new(cw as u32, ch as u32));
    let _ = w.emit_to("shot", "shot-start", ());
    let _ = w.show();
    let _ = w.set_focus();
}

pub static STOP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 框选完成：参数为遮罩窗口内的 CSS 像素；long=长截图（自动滚动拼接）
pub fn done(app: &AppHandle, x: f64, y: f64, w: f64, h: f64, long: bool) -> Result<(), String> {
    let win = app.get_webview_window("shot").ok_or("no shot window")?;
    let sf = win.scale_factor().map_err(|e| e.to_string())?;
    let pos = win.outer_position().map_err(|e| e.to_string())?;
    let _ = win.hide();
    let (sx, sy) = (pos.x + (x * sf).round() as i32, pos.y + (y * sf).round() as i32);
    let (sw, sh) = (((w * sf).round() as i32).max(1), ((h * sf).round() as i32).max(1));
    // 等遮罩真正消失再截
    std::thread::sleep(std::time::Duration::from_millis(150));
    let (img, ih) = if long { long_capture(sx, sy, sw, sh)? } else { (capture(sx, sy, sw, sh)?, sh) };
    let png = encode(&img, sw, ih)?;
    let url = format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png));
    // 弹窗放在选区右侧/下方
    crate::selection::show_popup_with(app, ((sx + sw) as f64, sy as f64), "image", url);
    Ok(())
}

pub fn cancel(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("shot") {
        let _ = w.hide();
    }
}

/// 长截图：鼠标移到选区中央，模拟滚轮向下滚动，逐帧找重叠并拼接；滚不动或按 Esc 结束
fn long_capture(x: i32, y: i32, w: i32, h: i32) -> Result<(Vec<u8>, i32), String> {
    use std::sync::atomic::Ordering;
    STOP.store(false, Ordering::Relaxed);
    let row = (w * 4) as usize;
    let mut out = capture(x, y, w, h)?;
    let mut prev = out.clone();
    let notches = (h / 260).clamp(1, 4) as i64;
    let mut still = 0;
    unsafe {
        let _ = SetCursorPos(x + w / 2, y + h / 2);
    }
    for _ in 0..60 {
        if STOP.load(Ordering::Relaxed) || out.len() / row > 20000 {
            break;
        }
        for _ in 0..notches {
            let _ = rdev::simulate(&rdev::EventType::Wheel { delta_x: 0, delta_y: -1 });
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        std::thread::sleep(std::time::Duration::from_millis(380));
        let cur = capture(x, y, w, h)?;
        match find_shift(&prev, &cur, w as usize, h as usize) {
            Some(0) => {
                still += 1;
                if still >= 2 {
                    break; // 到底了
                }
            }
            Some(d) => {
                still = 0;
                out.extend_from_slice(&cur[(h as usize - d) * row..]);
                prev = cur;
            }
            None => break, // 内容变化无法对齐（非滚动区域），停止
        }
    }
    let total = (out.len() / row) as i32;
    Ok((out, total))
}

/// 找出 cur 相对 prev 向上滚动了多少行：cur[0..h-d] ≈ prev[d..h]
fn find_shift(prev: &[u8], cur: &[u8], w: usize, h: usize) -> Option<usize> {
    let row = w * 4;
    let xs: Vec<usize> = (0..w).step_by((w / 64).max(1)).collect();
    let diff = |d: usize, limit: u64| -> Option<u64> {
        let n = h - d;
        let step = (n / 80).max(1);
        let mut s = 0u64;
        let mut cnt = 0u64;
        for r in (0..n).step_by(step) {
            let a = &cur[r * row..];
            let b = &prev[(r + d) * row..];
            for &x in &xs {
                let i = x * 4;
                s += (a[i] as i32 - b[i] as i32).unsigned_abs() as u64
                    + (a[i + 1] as i32 - b[i + 1] as i32).unsigned_abs() as u64
                    + (a[i + 2] as i32 - b[i + 2] as i32).unsigned_abs() as u64;
                cnt += 1;
            }
            if s > limit * cnt.max(1) * 4 {
                return None;
            }
        }
        Some(s / cnt.max(1))
    };
    if diff(0, 2).map_or(false, |v| v <= 1) {
        return Some(0);
    }
    let mut best: Option<(usize, u64)> = None;
    for d in 1..h.saturating_sub(h / 10).max(2) {
        if let Some(v) = diff(d, 12) {
            if best.map_or(true, |(_, b)| v < b) {
                best = Some((d, v));
                if v == 0 {
                    break;
                }
            }
        }
    }
    best.filter(|&(_, v)| v <= 6).map(|(d, _)| d)
}

fn capture(x: i32, y: i32, w: i32, h: i32) -> Result<Vec<u8>, String> {
    let mut buf = vec![0u8; (w * h * 4) as usize];
    unsafe {
        let screen = GetDC(HWND::default());
        let mem = CreateCompatibleDC(screen);
        let bmp = CreateCompatibleBitmap(screen, w, h);
        let old = SelectObject(mem, bmp);
        let ok = BitBlt(mem, 0, 0, w, h, screen, x, y, SRCCOPY);
        let mut bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let lines = GetDIBits(mem, bmp, 0, h as u32, Some(buf.as_mut_ptr() as _), &mut bi, DIB_RGB_COLORS);
        SelectObject(mem, old);
        let _ = DeleteObject(bmp);
        let _ = DeleteDC(mem);
        ReleaseDC(HWND::default(), screen);
        ok.map_err(|e| format!("截图失败：{e}"))?;
        if lines == 0 {
            return Err("截图失败：GetDIBits".into());
        }
    }
    // BGRA → RGBA
    for p in buf.chunks_exact_mut(4) {
        p.swap(0, 2);
        p[3] = 255;
    }
    Ok(buf)
}

fn encode(buf: &[u8], w: i32, h: i32) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w as u32, h as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Fast);
        let mut wr = enc.write_header().map_err(|e| e.to_string())?;
        wr.write_image_data(buf).map_err(|e| e.to_string())?;
    }
    Ok(out)
}
