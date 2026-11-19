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

/// 框选完成：参数为遮罩窗口内的 CSS 像素
pub fn done(app: &AppHandle, x: f64, y: f64, w: f64, h: f64) -> Result<(), String> {
    let win = app.get_webview_window("shot").ok_or("no shot window")?;
    let sf = win.scale_factor().map_err(|e| e.to_string())?;
    let pos = win.outer_position().map_err(|e| e.to_string())?;
    let _ = win.hide();
    let (sx, sy) = (pos.x + (x * sf).round() as i32, pos.y + (y * sf).round() as i32);
    let (sw, sh) = (((w * sf).round() as i32).max(1), ((h * sf).round() as i32).max(1));
    // 等遮罩真正消失再截
    std::thread::sleep(std::time::Duration::from_millis(120));
    let png = capture(sx, sy, sw, sh)?;
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
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w as u32, h as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Fast);
        let mut wr = enc.write_header().map_err(|e| e.to_string())?;
        wr.write_image_data(&buf).map_err(|e| e.to_string())?;
    }
    Ok(out)
}
