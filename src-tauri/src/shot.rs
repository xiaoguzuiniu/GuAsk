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

/// 全局滚轮计数：长截图只在用户真的滚动后才去对齐/提示
pub static WHEEL: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
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
    let mut synced = WHEEL.load(Ordering::Relaxed);
    let mut last_wheel_at = std::time::Instant::now() - std::time::Duration::from_secs(10);
    let mut lost = false;
    let mut lost_since: Option<std::time::Instant> = None;
    let t0 = std::time::Instant::now();
    while !STOP.load(Ordering::Relaxed) && out.len() / row < 30000 && t0.elapsed().as_secs() < 300 {
        std::thread::sleep(std::time::Duration::from_millis(30));
        let wn = WHEEL.load(Ordering::Relaxed);
        if wn != synced {
            synced = wn;
            last_wheel_at = std::time::Instant::now();
        }
        // 最近 1.5 秒内没有滚轮：画面变化来自动图/悬停等，不是滚动，忽略
        if last_wheel_at.elapsed().as_millis() > 1500 {
            if lost {
                // 停下来了还对不上，继续尝试（用户可能往回滚了），但不重复提示
            } else {
                continue;
            }
        }
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
                if !lost && since.elapsed().as_millis() > 800 {
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

const SEG: usize = 24;
type Sig = [u8; SEG];

/// 每行压成 24 个色块的灰度均值。用均值而不是精确像素：
/// 高 DPI / 平滑滚动下文字会按亚像素位置重新渲染，像素不会完全相同，均值很稳定
fn row_sigs(img: &[u8], w: usize, h: usize) -> Vec<Sig> {
    let row = w * 4;
    let l = (w / 20).max(2);
    let r = w.saturating_sub((w / 20).max(24)).max(l + SEG);
    let seg = (r - l) / SEG;
    (0..h)
        .map(|y| {
            let px = &img[y * row..(y + 1) * row];
            let mut s = [0u8; SEG];
            for (k, sv) in s.iter_mut().enumerate() {
                let mut sum = 0u32;
                for x in (l + k * seg)..(l + (k + 1) * seg) {
                    let i = x * 4;
                    sum += (px[i] as u32 * 3 + px[i + 1] as u32 * 6 + px[i + 2] as u32) / 10;
                }
                *sv = (sum / seg as u32) as u8;
            }
            s
        })
        .collect()
}

/// 一行是否有内容（不是纯背景）
fn busy_row(s: &Sig) -> bool {
    let (mn, mx) = s.iter().fold((255u8, 0u8), |(a, b), &v| (a.min(v), b.max(v)));
    mx - mn > 6
}

/// 两行差异：丢掉差得最多的 1/4 块（动图、悬停高亮只影响局部），其余求和
fn row_cost(a: &Sig, b: &Sig) -> u32 {
    let mut d = [0u32; SEG];
    for k in 0..SEG {
        d[k] = (a[k] as i32 - b[k] as i32).unsigned_abs().min(40);
    }
    d.sort_unstable();
    d[..SEG * 3 / 4].iter().sum::<u32>() * 4 / 3
}

/// 找 cur 相对 prev 的滚动量 d（向下滚为正）：cur[i] ≈ prev[i + d]
fn match_shift(p: &[Sig], c: &[Sig]) -> Option<i64> {
    let h = p.len() as i64;
    let min_overlap = (h / 6).max(20);
    let pb: Vec<bool> = p.iter().map(busy_row).collect();
    let cb: Vec<bool> = c.iter().map(busy_row).collect();
    let mut best: Option<(i64, u64)> = None; // (d, 平均代价*100)
    let mut costs = Vec::new();
    for d in -(h - min_overlap)..=(h - min_overlap) {
        let (a, b) = (0.max(-d), h.min(h - d));
        let (mut sum, mut n) = (0u64, 0u64);
        let mut i = a;
        while i < b {
            let (ci, pi) = (i as usize, (i + d) as usize);
            if cb[ci] || pb[pi] {
                sum += row_cost(&c[ci], &p[pi]) as u64;
                n += 1;
            }
            i += 2;
        }
        if n < 8 {
            continue;
        }
        let avg = sum * 100 / n / SEG as u64; // 每块平均灰度差 ×100
        costs.push(avg);
        if best.map_or(true, |(bd, bv)| avg < bv || (avg == bv && d.abs() < bd.abs())) {
            best = Some((d, avg));
        }
    }
    let (d, v) = best?;
    // 绝对阈值：平均每块差 < 4 灰度；相对阈值：明显好于其它位置（避免空白/重复内容误判）
    costs.sort_unstable();
    let med = costs[costs.len() / 2];
    if v <= 50 || (v <= 400 && v * 2 <= med) {
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
        let page: Vec<Sig> = (0..2000u32).map(|i| {
            let mut s = [245u8; SEG];
            if i % 9 < 4 { for k in 0..SEG { s[k] = ((i * 37 + k as u32 * 101) % 200) as u8; } }
            s
        }).collect();
        let win = |top: usize| page[top..top + 400].to_vec();
        assert_eq!(match_shift(&win(100), &win(100)), Some(0));
        assert_eq!(match_shift(&win(100), &win(103)), Some(3));
        assert_eq!(match_shift(&win(100), &win(400)), Some(300));
        assert_eq!(match_shift(&win(400), &win(250)), Some(-150));
        assert_eq!(match_shift(&win(100), &win(900)), None);
        // 亚像素渲染：每个值都抖动 ±2，仍要对上
        let noisy = |top: usize| win(top).into_iter().enumerate().map(|(r, mut s)| { for (k, v) in s.iter_mut().enumerate() { *v = v.saturating_add(((r + k) % 3) as u8).saturating_sub(1); } s }).collect::<Vec<_>>();
        assert_eq!(match_shift(&win(100), &noisy(160)), Some(60));
        assert_eq!(match_shift(&win(500), &noisy(420)), Some(-80));
        // 局部动图：每行有 3 块乱变
        let gif = |top: usize| win(top).into_iter().enumerate().map(|(r, mut s)| { for k in 5..8 { s[k] = ((r * 13 + k * 7) % 255) as u8; } s }).collect::<Vec<_>>();
        assert_eq!(match_shift(&win(100), &gif(140)), Some(40));
    }
}
