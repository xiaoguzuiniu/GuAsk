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
    let mut last = prev.clone();
    let mut psig = row_sigs(&prev, w as usize, h as usize);
    // pos = 当前帧顶部在拼接图中的位置（可上下滚动，只有超出已拼部分才追加）
    let mut pos: i64 = 0;
    let mut lost = false;
    let mut lost_since: Option<std::time::Instant> = None;
    let t0 = std::time::Instant::now();
    while !STOP.load(Ordering::Relaxed) && out.len() / row < 30000 && t0.elapsed().as_secs() < 300 {
        std::thread::sleep(std::time::Duration::from_millis(30));
        let cur = capture(x, y, w, h)?;
        if cur == last {
            continue; // 画面没变
        }
        last = cur.clone();
        let csig = row_sigs(&cur, w as usize, h as usize);
        match match_shift(&psig, &csig) {
            Some(d) => {
                pos += d;
                let total = (out.len() / row) as i64;
                if pos + h as i64 > total && pos >= 0 {
                    let from = (total - pos).max(0) as usize;
                    out.extend_from_slice(&cur[from * row..]);
                    let _ = app.emit_to("shot", "shot-progress", out.len() / row);
                }
                prev = cur;
                psig = csig;
                lost_since = None;
                if lost {
                    lost = false;
                    let _ = app.emit_to("shot", "shot-warn", false);
                }
            }
            None => {
                // 对不上可能只是滚动动画中途；持续 0.6 秒都对不上才提示
                let since = *lost_since.get_or_insert_with(std::time::Instant::now);
                if !lost && since.elapsed().as_millis() > 600 {
                    lost = true;
                    let _ = app.emit_to("shot", "shot-warn", true);
                }
            }
        }
        let _ = &prev;
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

/// 每行的特征值：忽略左右边缘（滚动条、边框），颜色量化以容忍轻微抖动；
/// 纯色行（空白背景）记为 None，不参与匹配
fn row_sigs(img: &[u8], w: usize, h: usize) -> Vec<Option<u64>> {
    let row = w * 4;
    let (l, r) = ((w / 20).max(2), w.saturating_sub((w / 20).max(24)));
    let r = r.max(l + 1);
    (0..h)
        .map(|y| {
            let px = &img[y * row..(y + 1) * row];
            let mut hsh: u64 = 1469598103934665603;
            let first = (px[l * 4] >> 3, px[l * 4 + 1] >> 3, px[l * 4 + 2] >> 3);
            let mut uniform = true;
            for x in l..r {
                let i = x * 4;
                let c = (px[i] >> 3, px[i + 1] >> 3, px[i + 2] >> 3);
                if c != first {
                    uniform = false;
                }
                for v in [c.0, c.1, c.2] {
                    hsh ^= v as u64;
                    hsh = hsh.wrapping_mul(1099511628211);
                }
            }
            if uniform { None } else { Some(hsh) }
        })
        .collect()
}

/// 找 cur 相对 prev 的滚动量 d（向下滚为正）：cur[i] == prev[i + d]
/// 只统计有内容的行，匹配率足够高才认可；允许局部变化（悬停高亮、滚动条等）
fn match_shift(p: &[Option<u64>], c: &[Option<u64>]) -> Option<i64> {
    let h = p.len() as i64;
    let min_overlap = (h / 8).max(16);
    let mut best: Option<(i64, usize, usize)> = None; // (d, 命中, 有效行)
    for d in -(h - min_overlap)..=(h - min_overlap) {
        let (mut hit, mut n) = (0usize, 0usize);
        let (a, b) = (0.max(-d), h.min(h - d));
        for i in a..b {
            let (ci, pi) = (c[i as usize], p[(i + d) as usize]);
            if ci.is_none() && pi.is_none() {
                continue;
            }
            n += 1;
            if ci == pi {
                hit += 1;
            }
        }
        if n < 6 {
            continue;
        }
        let better = match best {
            None => true,
            Some((bd, bh, bn)) => hit * bn > bh * n || (hit * bn == bh * n && (hit > bh || (hit == bh && d.abs() < bd.abs()))),
        };
        if better {
            best = Some((d, hit, n));
        }
    }
    let (d, hit, n) = best?;
    // 至少 60% 的有内容行对得上，并且命中行数不能太少
    if hit * 10 >= n * 6 && hit >= 6 {
        Some(d)
    } else {
        None
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shift_detect() {
        // 模拟一张长页面：每 7 行一段文字，其余空白
        let page: Vec<Option<u64>> = (0..2000u64).map(|i| if i % 7 < 3 { Some(i * 31 + 5) } else { None }).collect();
        let win = |top: usize| page[top..top + 400].to_vec();
        assert_eq!(match_shift(&win(100), &win(100)), Some(0));
        assert_eq!(match_shift(&win(100), &win(103)), Some(3));
        assert_eq!(match_shift(&win(100), &win(400)), Some(300));
        assert_eq!(match_shift(&win(400), &win(250)), Some(-150));
        assert_eq!(match_shift(&win(100), &win(900)), None);
    }
}
