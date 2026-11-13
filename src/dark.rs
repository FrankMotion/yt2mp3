//! The look: a warm near-black window in moonlight colours (cream text,
//! moon-beige accent) built from plain Win32 controls – dark title bar,
//! self-drawn rounded buttons, framed fields, dark list and menus.

use std::ffi::c_void;

type Hwnd = *mut c_void;

#[repr(C)]
#[derive(Clone, Copy)]
struct Rect {
    l: i32,
    t: i32,
    r: i32,
    b: i32,
}

#[repr(C)]
struct Point {
    x: i32,
    y: i32,
}

/// DRAWITEMSTRUCT, sent with WM_DRAWITEM for owner-drawn buttons.
#[repr(C)]
struct DrawItem {
    ctl_type: u32,
    ctl_id: u32,
    item_id: u32,
    item_action: u32,
    item_state: u32,
    hwnd: Hwnd,
    hdc: *mut c_void,
    rc: Rect,
    item_data: usize,
}

/// MINMAXINFO: five POINTs; ptMinTrackSize is the fourth.
#[repr(C)]
struct MinMaxInfo {
    reserved: [i32; 2],
    max_size: [i32; 2],
    max_position: [i32; 2],
    min_track: [i32; 2],
    max_track: [i32; 2],
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
    fn CreatePen(style: i32, width: i32, color: u32) -> *mut c_void;
    fn SelectObject(hdc: *mut c_void, obj: *mut c_void) -> *mut c_void;
    fn DeleteObject(obj: *mut c_void) -> i32;
    fn RoundRect(hdc: *mut c_void, l: i32, t: i32, r: i32, b: i32, w: i32, h: i32) -> i32;
    fn SetTextColor(hdc: *mut c_void, color: u32) -> u32;
    fn SetBkColor(hdc: *mut c_void, color: u32) -> u32;
    fn SetBkMode(hdc: *mut c_void, mode: i32) -> i32;
    fn ExcludeClipRect(dc: *mut c_void, l: i32, t: i32, r: i32, b: i32) -> i32;
}
#[link(name = "user32")]
extern "system" {
    fn FillRect(hdc: *mut c_void, r: *const Rect, brush: *mut c_void) -> i32;
    fn FrameRect(hdc: *mut c_void, r: *const Rect, brush: *mut c_void) -> i32;
    fn GetClientRect(h: Hwnd, r: *mut Rect) -> i32;
    fn GetWindowRect(h: Hwnd, r: *mut Rect) -> i32;
    fn GetDpiForWindow(h: Hwnd) -> u32;
    fn RedrawWindow(h: Hwnd, r: *const Rect, region: *mut c_void, flags: u32) -> i32;
    fn GetWindowDC(h: Hwnd) -> *mut c_void;
    fn ReleaseDC(h: Hwnd, dc: *mut c_void) -> i32;
    fn ClientToScreen(h: Hwnd, p: *mut Point) -> i32;
    fn SendMessageW(h: Hwnd, msg: u32, w: usize, l: isize) -> isize;
    fn GetWindowTextW(h: Hwnd, buf: *mut u16, max: i32) -> i32;
    fn DrawTextW(hdc: *mut c_void, text: *const u16, len: i32, rc: *mut Rect, format: u32) -> i32;
    fn GetWindowLongPtrW(h: Hwnd, index: i32) -> isize;
    fn SetWindowLongPtrW(h: Hwnd, index: i32, value: isize) -> isize;
    fn SetWindowPos(h: Hwnd, after: Hwnd, x: i32, y: i32, cx: i32, cy: i32, flags: u32) -> i32;
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
/// warm near-black background
pub const BG: u32 = rgb(20, 19, 23);
/// fields, list, secondary buttons
pub const FIELD: u32 = rgb(30, 29, 35);
/// thin frames around fields and buttons
pub const BORDER: u32 = rgb(52, 50, 58);
/// cream text
const TEXT: u32 = rgb(236, 230, 216);
/// warm grey for quiet text (status line)
const MUTED: u32 = rgb(156, 150, 138);
/// moonlight beige: the Download button and the progress bar
const ACCENT: u32 = rgb(230, 220, 195);
const ACCENT_DOWN: u32 = rgb(200, 190, 166);
const TRACK: u32 = rgb(38, 37, 44);

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
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

/// A solid brush, as an address so it can live in a 'static closure.
pub fn brush(color: u32) -> usize {
    unsafe { CreateSolidBrush(color) as usize }
}

/// Paints a control's frame (the part outside its client area): filled
/// with `fill`, with a 1-px outline in `border` (0 = none). Windows / nwg
/// would paint it system-white after a resize.
pub fn frame(h: Hwnd, msg: u32, fill: usize, border: usize) -> Option<isize> {
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
        FillRect(dc, &all, fill as *mut c_void);
        if border != 0 {
            FrameRect(dc, &all, border as *mut c_void);
        }
        ReleaseDC(h, dc);
    }
    Some(0)
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

/// Dark title bar in the window colour (Windows 10 2004+; colours on Windows 11).
pub fn title_bar(window: Hwnd) {
    unsafe {
        let on: i32 = 1;
        DwmSetWindowAttribute(window, 20, &on as *const i32 as _, 4);
        DwmSetWindowAttribute(window, 35, &BG as *const u32 as _, 4); // caption
        DwmSetWindowAttribute(window, 34, &BORDER as *const u32 as _, 4); // border
        DwmSetWindowAttribute(window, 36, &TEXT as *const u32 as _, 4); // caption text
    }
}

pub fn theme(control: Hwnd, name: &str) {
    unsafe {
        SetWindowTheme(control, wide(name).as_ptr(), std::ptr::null());
    }
}

/// Classic (unthemed) progress bar so its colours can be set: beige on dark.
pub fn progress(bar: Hwnd) {
    unsafe {
        let empty = wide("");
        SetWindowTheme(bar, empty.as_ptr(), empty.as_ptr());
        SendMessageW(bar, 0x0409, 0, ACCENT as isize); // PBM_SETBARCOLOR
        SendMessageW(bar, 0x2001, 0, TRACK as isize); // PBM_SETBKCOLOR
    }
}

/// Lets the window draw this button itself (rounded, in our colours).
pub fn owner_draw(button: Hwnd) {
    const GWL_STYLE: i32 = -16;
    const BS_TYPEMASK: isize = 0xF;
    const BS_OWNERDRAW: isize = 0xB;
    unsafe {
        let style = GetWindowLongPtrW(button, GWL_STYLE);
        SetWindowLongPtrW(button, GWL_STYLE, (style & !BS_TYPEMASK) | BS_OWNERDRAW);
    }
}

/// Dark history list: Explorer's dark theme (soft grey selection, dark
/// scrollbar), our colours, no sunken 3D border.
pub fn list(view: Hwnd) {
    const GWL_EXSTYLE: i32 = -20;
    const WS_EX_CLIENTEDGE: isize = 0x200;
    const SWP_FRAME: u32 = 0x0020 | 0x0002 | 0x0001 | 0x0004; // FRAMECHANGED|NOMOVE|NOSIZE|NOZORDER
    theme(view, "DarkMode_Explorer");
    unsafe {
        SendMessageW(view, 0x1001, 0, FIELD as isize); // LVM_SETBKCOLOR
        SendMessageW(view, 0x1026, 0, FIELD as isize); // LVM_SETTEXTBKCOLOR
        SendMessageW(view, 0x1024, 0, TEXT as isize); // LVM_SETTEXTCOLOR
        let ex = GetWindowLongPtrW(view, GWL_EXSTYLE);
        SetWindowLongPtrW(view, GWL_EXSTYLE, ex & !WS_EX_CLIENTEDGE);
        SetWindowPos(view, std::ptr::null_mut(), 0, 0, 0, 0, SWP_FRAME);
    }
}

#[repr(C)]
struct PaintStruct {
    hdc: *mut c_void,
    erase: i32,
    rc: Rect,
    restore: i32,
    inc_update: i32,
    reserved: [u8; 32],
}

#[link(name = "user32")]
extern "system" {
    fn BeginPaint(h: Hwnd, ps: *mut PaintStruct) -> *mut c_void;
    fn EndPaint(h: Hwnd, ps: *const PaintStruct) -> i32;
}
#[link(name = "gdi32")]
extern "system" {
    fn CreateCompatibleDC(hdc: *mut c_void) -> *mut c_void;
    fn CreateCompatibleBitmap(hdc: *mut c_void, w: i32, h: i32) -> *mut c_void;
    fn DeleteDC(hdc: *mut c_void) -> i32;
    fn BitBlt(dst: *mut c_void, x: i32, y: i32, w: i32, h: i32, src: *mut c_void, sx: i32, sy: i32, rop: u32) -> i32;
}

/// Paints the history list flicker-free on our dark background. (nwg's own
/// double buffer clears it with a hard-coded white brush.)
pub fn list_paint(h: Hwnd, msg: u32, fill: usize) -> Option<isize> {
    const WM_ERASEBKGND: u32 = 0x0014;
    const WM_PAINT: u32 = 0x000F;
    const WM_PRINTCLIENT: u32 = 0x0318;
    const SRCCOPY: u32 = 0x00CC_0020;
    match msg {
        WM_ERASEBKGND => Some(1),
        WM_PAINT => unsafe {
            let mut r = Rect { l: 0, t: 0, r: 0, b: 0 };
            GetClientRect(h, &mut r);
            let mut ps: PaintStruct = std::mem::zeroed();
            let hdc = BeginPaint(h, &mut ps);
            let buf_dc = CreateCompatibleDC(hdc);
            let bmp = CreateCompatibleBitmap(hdc, r.r.max(1), r.b.max(1));
            let old = SelectObject(buf_dc, bmp);
            FillRect(buf_dc, &r, fill as *mut c_void);
            SendMessageW(h, WM_PRINTCLIENT, buf_dc as usize, 0);
            BitBlt(hdc, 0, 0, r.r, r.b, buf_dc, 0, 0, SRCCOPY);
            SelectObject(buf_dc, old);
            DeleteObject(bmp);
            DeleteDC(buf_dc);
            EndPaint(h, &ps);
            Some(0)
        },
        _ => None,
    }
}

/// Brushes and settings the window's message hook works with.
pub struct Paint {
    bg: *mut c_void,
    field: *mut c_void,
    min_size: (i32, i32),
    /// the accent (Download) button
    primary: usize,
}

impl Paint {
    /// `min_size`: smallest outer window size in pixels.
    /// `primary`: the button drawn in the accent colour.
    pub fn new(min_size: (i32, i32), primary: Hwnd) -> Self {
        unsafe {
            Paint {
                bg: CreateSolidBrush(BG),
                field: CreateSolidBrush(FIELD),
                min_size,
                primary: primary as usize,
            }
        }
    }

    /// One owner-drawn button: rounded box, centred label.
    unsafe fn draw_button(&self, d: &DrawItem) {
        const ODS_SELECTED: u32 = 0x1;
        const ODS_DISABLED: u32 = 0x4;
        let primary = d.hwnd as usize == self.primary;
        let pressed = d.item_state & ODS_SELECTED != 0;
        let disabled = d.item_state & ODS_DISABLED != 0;
        let (fill, edge, text) = match (primary, pressed, disabled) {
            (_, _, true) => (FIELD, BORDER, MUTED),
            (true, false, _) => (ACCENT, ACCENT, BG),
            (true, true, _) => (ACCENT_DOWN, ACCENT_DOWN, BG),
            (false, false, _) => (FIELD, BORDER, TEXT),
            (false, true, _) => (BORDER, BORDER, TEXT),
        };
        let hdc = d.hdc;
        FillRect(hdc, &d.rc, self.bg);
        let radius = (10.0 * scale(d.hwnd)) as i32;
        let brush = CreateSolidBrush(fill);
        let pen = CreatePen(0, 1, edge);
        let old_brush = SelectObject(hdc, brush);
        let old_pen = SelectObject(hdc, pen);
        RoundRect(hdc, d.rc.l, d.rc.t, d.rc.r, d.rc.b, radius, radius);
        SelectObject(hdc, old_brush);
        SelectObject(hdc, old_pen);
        DeleteObject(brush);
        DeleteObject(pen);

        let mut label = [0u16; 128];
        let len = GetWindowTextW(d.hwnd, label.as_mut_ptr(), label.len() as i32);
        let font = SendMessageW(d.hwnd, 0x0031, 0, 0) as *mut c_void; // WM_GETFONT
        let old_font = SelectObject(hdc, font);
        SetBkMode(hdc, 1); // transparent
        SetTextColor(hdc, text);
        let mut rc = d.rc;
        DrawTextW(hdc, label.as_ptr(), len, &mut rc, 0x1 | 0x4 | 0x20); // centre, vcentre, single line
        SelectObject(hdc, old_font);
    }

    /// Answers the colour/draw messages the window gets from its children.
    /// `edit` is the link field (it asks with CTLCOLORSTATIC while read-only).
    pub fn handle(&self, hwnd: Hwnd, msg: u32, w: usize, l: isize, edit: Hwnd) -> Option<isize> {
        const WM_ERASEBKGND: u32 = 0x0014;
        const WM_GETMINMAXINFO: u32 = 0x0024;
        const WM_DRAWITEM: u32 = 0x002B;
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
                WM_DRAWITEM => {
                    let d = &*(l as *const DrawItem);
                    if d.ctl_type != 4 {
                        return None; // ODT_BUTTON only
                    }
                    self.draw_button(d);
                    Some(1)
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
