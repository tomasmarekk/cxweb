//! Off-screen render placement for the exclusively owned browser process.
//! No window titles, browser content or windows owned by another PID are read.
use std::io;
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, RECT},
    UI::WindowsAndMessaging::{
        EnumWindows, GWL_EXSTYLE, GetClassNameW, GetSystemMetrics, GetWindowLongPtrW,
        GetWindowRect, GetWindowThreadProcessId, HWND_BOTTOM, IsWindowVisible, SM_CXSCREEN,
        SM_CXVIRTUALSCREEN, SM_CYSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
        SW_HIDE, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_SHOWWINDOW, SetWindowLongPtrW, SetWindowPos,
        ShowWindow, WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    },
};

pub fn login_bounds() -> (i32, i32, i32, i32) {
    // A background render window can become Chrome's saved window placement.
    // Explicit login must instead start within the primary desktop.
    // SAFETY: these metrics have no pointer parameters or ownership effects.
    unsafe {
        let screen_width = GetSystemMetrics(SM_CXSCREEN).max(320);
        let screen_height = GetSystemMetrics(SM_CYSCREEN).max(240);
        let width = (screen_width - 48).min(1280);
        let height = (screen_height - 80).min(900);
        ((screen_width - width) / 2, 24, width, height)
    }
}

pub fn bounds() -> (i32, i32, i32, i32) {
    // SAFETY: these metrics have no pointer parameters or ownership effects.
    unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN) - 1400,
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            1280,
            900,
        )
    }
}

pub fn park(pid: u32) -> io::Result<(usize, bool, bool)> {
    struct Search {
        pid: u32,
        windows: Vec<HWND>,
    }
    unsafe extern "system" fn visit(window: HWND, parameter: LPARAM) -> i32 {
        // SAFETY: EnumWindows calls synchronously with the live Search below.
        let search = unsafe { &mut *(parameter as *mut Search) };
        let mut owner = 0;
        // SAFETY: window belongs to this enumeration; output is initialized.
        unsafe {
            GetWindowThreadProcessId(window, &mut owner);
        }
        if owner != search.pid {
            return 1;
        }
        let mut class = [0u16; 64];
        // SAFETY: buffer is valid for its declared capacity.
        let length = unsafe { GetClassNameW(window, class.as_mut_ptr(), class.len() as i32) };
        if length > 0 && String::from_utf16_lossy(&class[..length as usize]) == "Chrome_WidgetWin_1"
        {
            search.windows.push(window);
        }
        1
    }
    let mut search = Search {
        pid,
        windows: Vec::new(),
    };
    // SAFETY: Search lives through the synchronous callback and is uniquely borrowed.
    if unsafe { EnumWindows(Some(visit), (&mut search as *mut Search) as LPARAM) } == 0 {
        return Err(io::Error::last_os_error());
    }
    if search.windows.is_empty() {
        return Err(io::Error::other("E_NO_BROWSER_WINDOW"));
    }
    let (x, y, width, height) = bounds();
    let mut exposed = false;
    let mut taskbar = false;
    for &window in &search.windows {
        // SAFETY: recheck process ownership immediately before any mutation.
        unsafe {
            let mut owner = 0;
            GetWindowThreadProcessId(window, &mut owner);
            if owner != pid {
                return Err(io::Error::other("E_BROWSER_WINDOW_OWNER"));
            }
            let mut initial: RECT = std::mem::zeroed();
            let initial_style = GetWindowLongPtrW(window, GWL_EXSTYLE);
            let visible = IsWindowVisible(window) != 0 && GetWindowRect(window, &mut initial) != 0;
            if visible {
                let left = GetSystemMetrics(SM_XVIRTUALSCREEN);
                let top = GetSystemMetrics(SM_YVIRTUALSCREEN);
                exposed |= initial.right > left
                    && initial.bottom > top
                    && initial.left < left + GetSystemMetrics(SM_CXVIRTUALSCREEN)
                    && initial.top < top + GetSystemMetrics(SM_CYVIRTUALSCREEN);
                taskbar |= initial_style & WS_EX_TOOLWINDOW as isize == 0;
            }
            let desired = (initial_style | WS_EX_TOOLWINDOW as isize | WS_EX_NOACTIVATE as isize)
                & !(WS_EX_APPWINDOW as isize);
            if visible
                && initial.right <= GetSystemMetrics(SM_XVIRTUALSCREEN)
                && initial.right - initial.left == width
                && initial.bottom - initial.top == height
                && initial_style == desired
            {
                continue;
            }
            ShowWindow(window, SW_HIDE);
            SetWindowLongPtrW(window, GWL_EXSTYLE, desired);
            if GetWindowLongPtrW(window, GWL_EXSTYLE) != desired {
                return Err(io::Error::other("E_BROWSER_WINDOW_STYLE"));
            }
            if SetWindowPos(
                window,
                HWND_BOTTOM,
                x,
                y,
                width,
                height,
                SWP_NOACTIVATE | SWP_FRAMECHANGED | SWP_SHOWWINDOW,
            ) == 0
            {
                return Err(io::Error::last_os_error());
            }
            let mut rect: RECT = std::mem::zeroed();
            if GetWindowRect(window, &mut rect) == 0
                || rect.right > GetSystemMetrics(SM_XVIRTUALSCREEN)
            {
                ShowWindow(window, SW_HIDE);
                return Err(io::Error::other("E_BROWSER_WINDOW_BOUNDS"));
            }
        }
    }
    Ok((search.windows.len(), exposed, taskbar))
}
