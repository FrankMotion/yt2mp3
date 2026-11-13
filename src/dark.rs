//! Black look for plain Win32 controls: dark title bar, dark fields,
//! dark buttons/menus (Windows' own dark themes), white progress bar.

use std::ffi::c_void;

type Hwnd = *mut c_void;

#[repr(C)]
struct Rect {
    l: i32,
    t: i32,
    r: i32,
    b: i32,
}

#[link(name = "dwmapi")]
extern "system" {
    fn DwmSetWindowAttribute(h: Hwnd, attr: u32, value: *const c_void, size: u32) -> i32;
}
#[link(name = "uxtheme")]
extern "system" {
    fn SetWindowTheme(h: Hwnd, app: *const u16, idlist: *const u16) -> i32;
}
#[link(name = "gdi32")]
extern "system" {
    fn CreateSolidBrush(color: u32) -> *mut c_void;
    fn SetTextColor(hdc: *mut c_void, color: u32) -> u32;
    fn SetBkColor(hdc: *mut c_void, color: u32) -> u32;
}
#[link(name = "user32")]
extern "system" {
    fn FillRect(hdc: *mut c_void, r: *const Rect, brush: *mut c_void) -> i32;
    fn GetClientRect(h: Hwnd, r: *mut Rect) -> i32;
    fn GetWindowRect(h: Hwnd, r: *mut Rect) -> i32;
    fn GetDpiForWindow(h: Hwnd) -> u32;
    fn RedrawWindow(h: Hwnd, r: *const Rect, region: *mut c_void, flags: u32) -> i32;
    fn GetWindowDC(h: Hwnd) -> *mut c_void;
    fn ReleaseDC(h: Hwnd, dc: *mut c_void) -> i32;
    fn ClientToScreen(h: Hwnd, p: *mut Point) -> i32;
}

#[repr(C)]
struct Point {
    x: i32,
    y: i32,
}

#[link(name = "gdi32")]
extern "system" {
    fn ExcludeClipRect(dc: *mut c_void, l: i32, t: i32, r: i32, b: i32) -> i32;
}

/// A solid brush, as an address so it can live in a 'static closure.
pub fn brush(color: u32) -> usize {
    unsafe { CreateSolidBrush(color) as usize }
}

/// Paints a control's frame (the part outside its client area) in one
/// colour. Windows / nwg would paint it system-white after a resize.
pub fn frame(h: Hwnd, msg: u32, brush: usize) -> Option<isize> {
    const WM_NCPAINT: u32 = 0x0085;
    if msg != WM_NCPAINT {
        return None;
    }
    unsafe {
        let dc = GetWindowDC(h);
        if dc.is_null() {
            return None;
        }
        let mut wr = Rect { l: 0, t: 0, r: 0, b: 0 };
        let mut cr = Rect { l: 0, t: 0, r: 0, b: 0 };
        GetWindowRect(h, &mut wr);
        GetClientRect(h, &mut cr);
        let mut origin = Point { x: 0, y: 0 };
        ClientToScreen(h, &mut origin);
        let (ox, oy) = (origin.x - wr.l, origin.y - wr.t);
        ExcludeClipRect(dc, ox, oy, ox + cr.r, oy + cr.b);
        let all = Rect { l: 0, t: 0, r: wr.r - wr.l, b: wr.b - wr.t };
        FillRect(dc, &all, brush as *mut c_void);
        ReleaseDC(h, dc);
    }
    Some(0)
}

#[link(name = "user32")]
extern "system" {
    fn SendMessageW(h: Hwnd, msg: u32, w: usize, l: isize) -> isize;
}

/// Pixels per logical unit (1.0 at 100 % scaling).
fn scale(h: Hwnd) -> f64 {
    let dpi = unsafe { GetDpiForWindow(h) };
    if dpi == 0 { 1.0 } else { dpi as f64 / 96.0 }
}

/// Client area in the same logical units nwg positions controls with.
pub fn client_logical(h: Hwnd) -> (i32, i32) {
    let mut r = Rect { l: 0, t: 0, r: 0, b: 0 };
    unsafe { GetClientRect(h, &mut r) };
    let s = scale(h);
    (((r.r - r.l) as f64 / s) as i32, ((r.b - r.t) as f64 / s) as i32)
}

/// Outer window size in real pixels.
pub fn window_pixels(h: Hwnd) -> (i32, i32) {
    let mut r = Rect { l: 0, t: 0, r: 0, b: 0 };
    unsafe { GetWindowRect(h, &mut r) };
    (r.r - r.l, r.b - r.t)
}

/// Repaint the whole window, background and every control, right now
/// (after a relayout, so no edges of old positions are left behind).
pub fn repaint(h: Hwnd) {
    const RDW_INVALIDATE: u32 = 0x0001;
    const RDW_ERASE: u32 = 0x0004;
    const RDW_ALLCHILDREN: u32 = 0x0080;
    const RDW_UPDATENOW: u32 = 0x0100;
    const RDW_FRAME: u32 = 0x0400;
    unsafe {
        RedrawWindow(
            h,
            std::ptr::null(),
            std::ptr::null_mut(),
            RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN | RDW_UPDATENOW | RDW_FRAME,
        )
    };
}
#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryW(name: *const u16) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
}

/// COLORREF is 0x00BBGGRR.
const fn rgb(r: u32, g: u32, b: u32) -> u32 {
    r | (g << 8) | (b << 16)
}
pub const BG: u32 = rgb(10, 10, 10);
pub const FIELD: u32 = rgb(26, 26, 26);
const TEXT: u32 = rgb(236, 236, 236);
const MUTED: u32 = rgb(150, 150, 150);
const BAR: u32 = rgb(236, 236, 236);
const TRACK: u32 = rgb(34, 34, 34);

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Undocumented but stable uxtheme exports (used by Notepad++, Explorer &c.):
/// #135 SetPreferredAppMode(ForceDark) makes popup menus dark, #136 flushes.
/// Call once before any window exists. Silently does nothing on old Windows.
pub fn dark_menus() {
    unsafe {
        let ux = LoadLibraryW(wide("uxtheme.dll").as_ptr());
        if ux.is_null() {
            return;
        }
        let set_mode = GetProcAddress(ux, 135usize as *const u8);
        let flush = GetProcAddress(ux, 136usize as *const u8);
        if !set_mode.is_null() {
            let f: extern "system" fn(i32) -> i32 = std::mem::transmute(set_mode);
            f(2);
        }
        if !flush.is_null() {
            let f: extern "system" fn() = std::mem::transmute(flush);
            f();
        }
    }
}

/// Black title bar (Windows 10 2004+; caption colours on Windows 11).
pub fn title_bar(window: Hwnd) {
    unsafe {
        let on: i32 = 1;
        DwmSetWindowAttribute(window, 20, &on as *const i32 as _, 4);
        DwmSetWindowAttribute(window, 35, &BG as *const u32 as _, 4); // caption
        let border = rgb(38, 38, 38);
        DwmSetWindowAttribute(window, 34, &border as *const u32 as _, 4);
        DwmSetWindowAttribute(window, 36, &TEXT as *const u32 as _, 4); // caption text
    }
}

pub fn theme(control: Hwnd, name: &str) {
    unsafe {
        SetWindowTheme(control, wide(name).as_ptr(), std::ptr::null());
    }
}

/// Classic (unthemed) progress bar so its colours can be set: white on dark.
pub fn progress(bar: Hwnd) {
    unsafe {
        let empty = wide("");
        SetWindowTheme(bar, empty.as_ptr(), empty.as_ptr());
        SendMessageW(bar, 0x0409, 0, BAR as isize); // PBM_SETBARCOLOR
        SendMessageW(bar, 0x2001, 0, TRACK as isize); // PBM_SETBKCOLOR
    }
}

/// Brushes the window's message hook hands back to Windows,
/// plus the smallest size the window may be dragged to.
pub struct Paint {
    bg: *mut c_void,
    field: *mut c_void,
    min_size: (i32, i32),
}

/// Layout of MINMAXINFO: five POINTs; ptMinTrackSize is the fourth.
#[repr(C)]
struct MinMaxInfo {
    reserved: [i32; 2],
    max_size: [i32; 2],
    max_position: [i32; 2],
    min_track: [i32; 2],
    max_track: [i32; 2],
}

impl Paint {
    /// `min_size`: smallest outer window size in pixels.
    pub fn new(min_size: (i32, i32)) -> Self {
        unsafe {
            Paint {
                bg: CreateSolidBrush(BG),
                field: CreateSolidBrush(FIELD),
                min_size,
            }
        }
    }

    /// Answers the colour messages a dialog-style window gets from its children.
    /// `edit` is the link field (it asks with CTLCOLORSTATIC while read-only).
    pub fn handle(&self, hwnd: Hwnd, msg: u32, w: usize, l: isize, edit: Hwnd) -> Option<isize> {
        const WM_ERASEBKGND: u32 = 0x0014;
        const WM_GETMINMAXINFO: u32 = 0x0024;
        const WM_CTLCOLOREDIT: u32 = 0x0133;
        const WM_CTLCOLORLISTBOX: u32 = 0x0134;
        const WM_CTLCOLORBTN: u32 = 0x0135;
        const WM_CTLCOLORSTATIC: u32 = 0x0138;
        let hdc = w as *mut c_void;
        unsafe {
            match msg {
                WM_ERASEBKGND => {
                    let mut r = Rect { l: 0, t: 0, r: 0, b: 0 };
                    GetClientRect(hwnd, &mut r);
                    FillRect(hdc, &r, self.bg);
                    Some(1)
                }
                WM_GETMINMAXINFO if self.min_size.0 > 0 => {
                    let info = &mut *(l as *mut MinMaxInfo);
                    info.min_track = [self.min_size.0, self.min_size.1];
                    Some(0)
                }
                WM_CTLCOLOREDIT => {
                    SetTextColor(hdc, TEXT);
                    SetBkColor(hdc, FIELD);
                    Some(self.field as isize)
                }
                WM_CTLCOLORSTATIC if l as Hwnd == edit => {
                    SetTextColor(hdc, MUTED);
                    SetBkColor(hdc, FIELD);
                    Some(self.field as isize)
                }
                WM_CTLCOLORSTATIC => {
                    SetTextColor(hdc, MUTED);
                    SetBkColor(hdc, BG);
                    Some(self.bg as isize)
                }
                WM_CTLCOLORBTN => Some(self.bg as isize),
                // the drop-down list of the format / quality boxes
                WM_CTLCOLORLISTBOX => {
                    SetTextColor(hdc, TEXT);
                    SetBkColor(hdc, FIELD);
                    Some(self.field as isize)
                }
                _ => None,
            }
        }
    }
}
