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
    // 提示条放在鼠标所在屏幕的顶部中央（CSS 像素，相对遮罩窗口）
    let (cx, cy) = crate::selection::cursor_pos();
    let sf = w.scale_factor().unwrap_or(1.0);
    let mut tip = ((cw as f64 / 2.0) / sf, 24.0);
    if let Ok(ms) = w.available_monitors() {
        for m in ms {
            let (p, s) = (m.position(), m.size());
            if cx >= p.x as f64 && cx < (p.x + s.width as i32) as f64 && cy >= p.y as f64 && cy < (p.y + s.height as i32) as f64 {
                tip = (((p.x - x) as f64 + s.width as f64 / 2.0) / sf, ((p.y - y) as f64) / sf + 24.0);
            }
        }
    }
    let _ = w.emit_to("shot", "shot-start", tip);
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
    let (img, ih) = if long { long_capture(app, sx, sy, sw, sh)? } else { (capture(sx, sy, sw, sh)?, sh) };
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

/// 长截图（手动滚动）：在选区旁显示小工具条，用户自己滚动，持续截取并按重叠拼接；
/// 点「完成」或按 Enter / Esc 结束
fn long_capture(app: &AppHandle, x: i32, y: i32, w: i32, h: i32) -> Result<(Vec<u8>, i32), String> {
    use std::sync::atomic::Ordering;
    STOP.store(false, Ordering::Relaxed);
    let row = (w * 4) as usize;
    let mut out = capture(x, y, w, h)?;
    let mut prev = out.clone();
    // 工具条放在选区上方（放不下就放下方）
    if let Some(win) = app.get_webview_window("shot") {
        let sf = win.scale_factor().unwrap_or(1.0);
        let (bw, bh) = ((330.0 * sf) as i32, (44.0 * sf) as i32);
        let top = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
        let by = if y - bh - 8 >= top { y - bh - 8 } else { y + h + 8 };
        let _ = win.set_size(PhysicalSize::new(bw as u32, bh as u32));
        let _ = win.set_position(PhysicalPosition::new(x + (w - bw) / 2, by));
        let _ = win.emit_to("shot", "shot-bar", h);
        let _ = win.show();
    }
    // 选区边框：画在选区外侧 3px，鼠标穿透，不会被截进去
    let frame = app.get_webview_window("frame");
    if let Some(f) = &frame {
        let _ = f.set_ignore_cursor_events(true);
        let _ = f.set_size(PhysicalSize::new((w + 6) as u32, (h + 6) as u32));
        let _ = f.set_position(PhysicalPosition::new(x - 3, y - 3));
        let _ = f.show();
    }
    let t0 = std::time::Instant::now();
    while !STOP.load(Ordering::Relaxed) && out.len() / row < 30000 && t0.elapsed().as_secs() < 300 {
        std::thread::sleep(std::time::Duration::from_millis(120));
        let cur = capture(x, y, w, h)?;
        if let Some(d) = find_shift(&prev, &cur, w as usize, h as usize) {
            if d > 0 {
                out.extend_from_slice(&cur[(h as usize - d) * row..]);
                prev = cur;
                let _ = app.emit_to("shot", "shot-progress", out.len() / row);
            }
        }
        // 对不上（滚动动画中途等）就跳过这一帧，等画面稳定
    }
    if let Some(win) = app.get_webview_window("shot") {
        let _ = win.hide();
    }
    if let Some(f) = &frame {
        let _ = f.hide();
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
