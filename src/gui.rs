//! sw1tch: a small native Win32 window for the Valorant account switcher.
//!
//! Plain user32/gdi32 controls declared by hand, so there is no GUI framework
//! and no crates. Slow actions (closing Riot, copying files) run on a worker
//! thread so the window never freezes.

#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(not(windows))]
fn main() {
    eprintln!("The sw1tch window is Windows-only. Use sw1tch-cli instead.");
    std::process::exit(1);
}

#[cfg(windows)]
fn main() {
    win::run();
}

#[cfg(windows)]
mod win {
    use std::cell::RefCell;
    use std::ffi::c_void;
    use std::ptr::{null, null_mut};
    use sw1tch::ops::{self, Opts};
    use sw1tch::store::Store;

    type Hwnd = *mut c_void;
    type Handle = *mut c_void;
    type WndProc = unsafe extern "system" fn(Hwnd, u32, usize, isize) -> isize;

    #[repr(C)]
    struct WndClassExW {
        size: u32,
        style: u32,
        wnd_proc: Option<WndProc>,
        cls_extra: i32,
        wnd_extra: i32,
        instance: Handle,
        icon: Handle,
        cursor: Handle,
        background: Handle,
        menu_name: *const u16,
        class_name: *const u16,
        icon_sm: Handle,
    }

    #[repr(C)]
    struct Msg {
        hwnd: Hwnd,
        message: u32,
        wparam: usize,
        lparam: isize,
        time: u32,
        pt: [i32; 2],
        private: u32,
    }

    #[repr(C)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    #[link(name = "user32")]
    extern "system" {
        fn RegisterClassExW(wc: *const WndClassExW) -> u16;
        fn CreateWindowExW(
            ex_style: u32, class: *const u16, title: *const u16, style: u32,
            x: i32, y: i32, w: i32, h: i32,
            parent: Hwnd, menu: Handle, instance: Handle, param: *mut c_void,
        ) -> Hwnd;
        fn DefWindowProcW(h: Hwnd, msg: u32, w: usize, l: isize) -> isize;
        fn GetMessageW(msg: *mut Msg, h: Hwnd, min: u32, max: u32) -> i32;
        fn TranslateMessage(msg: *const Msg) -> i32;
        fn DispatchMessageW(msg: *const Msg) -> isize;
        fn IsDialogMessageW(h: Hwnd, msg: *mut Msg) -> i32;
        fn PostQuitMessage(code: i32);
        fn PostMessageW(h: Hwnd, msg: u32, w: usize, l: isize) -> i32;
        fn SendMessageW(h: Hwnd, msg: u32, w: usize, l: isize) -> isize;
        fn MessageBoxW(h: Hwnd, text: *const u16, caption: *const u16, kind: u32) -> i32;
        fn LoadCursorW(inst: Handle, name: *const u16) -> Handle;
        fn LoadIconW(inst: Handle, name: *const u16) -> Handle;
        fn SetWindowTextW(h: Hwnd, text: *const u16) -> i32;
        fn GetWindowTextW(h: Hwnd, buf: *mut u16, max: i32) -> i32;
        fn GetWindowTextLengthW(h: Hwnd) -> i32;
        fn EnableWindow(h: Hwnd, enable: i32) -> i32;
        fn SetProcessDPIAware() -> i32;
        fn GetDC(h: Hwnd) -> Handle;
        fn ReleaseDC(h: Hwnd, dc: Handle) -> i32;
        fn AdjustWindowRectEx(r: *mut Rect, style: u32, menu: i32, ex_style: u32) -> i32;
        fn SetFocus(h: Hwnd) -> Hwnd;
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetModuleHandleW(name: *const u16) -> Handle;
    }

    #[link(name = "gdi32")]
    extern "system" {
        fn CreateFontW(
            h: i32, w: i32, esc: i32, orient: i32, weight: i32, italic: u32, underline: u32,
            strike: u32, charset: u32, out_prec: u32, clip_prec: u32, quality: u32,
            pitch: u32, face: *const u16,
        ) -> Handle;
        fn GetDeviceCaps(dc: Handle, index: i32) -> i32;
    }

    const WS_CHILD: u32 = 0x4000_0000;
    const WS_VISIBLE: u32 = 0x1000_0000;
    const WS_CAPTION: u32 = 0x00C0_0000;
    const WS_SYSMENU: u32 = 0x0008_0000;
    const WS_MINIMIZEBOX: u32 = 0x0002_0000;
    const WS_TABSTOP: u32 = 0x0001_0000;
    const WS_VSCROLL: u32 = 0x0020_0000;
    const WS_EX_CLIENTEDGE: u32 = 0x200;
    const CW_USEDEFAULT: i32 = i32::MIN;
    const LBS_NOTIFY: u32 = 0x1;
    const LBS_NOINTEGRALHEIGHT: u32 = 0x100;
    const ES_AUTOHSCROLL: u32 = 0x80;
    const BS_DEFPUSHBUTTON: u32 = 0x1;
    const BS_AUTOCHECKBOX: u32 = 0x3;
    const WM_DESTROY: u32 = 0x2;
    const WM_SETFONT: u32 = 0x30;
    const WM_COMMAND: u32 = 0x111;
    const WM_APP_DONE: u32 = 0x8000 + 1;
    const LB_ADDSTRING: u32 = 0x180;
    const LB_RESETCONTENT: u32 = 0x184;
    const LB_SETCURSEL: u32 = 0x186;
    const LB_GETCURSEL: u32 = 0x188;
    const LBN_SELCHANGE: u16 = 1;
    const LBN_DBLCLK: u16 = 2;
    const BM_GETCHECK: u32 = 0xF0;
    const BM_SETCHECK: u32 = 0xF1;
    const EM_SETCUEBANNER: u32 = 0x1501;
    const MB_YESNO: u32 = 0x4;
    const MB_ICONERROR: u32 = 0x10;
    const MB_ICONWARNING: u32 = 0x30;
    const IDYES: i32 = 6;
    const COLOR_BTNFACE: usize = 15;
    const LOGPIXELSY: i32 = 90;

    // Control IDs. Switch is IDOK so Enter triggers it.
    const ID_SWITCH: u16 = 1;
    const ID_SAVE: u16 = 101;
    const ID_ADD: u16 = 102;
    const ID_RENAME: u16 = 103;
    const ID_REMOVE: u16 = 104;
    const ID_LIST: u16 = 200;
    const ID_NAME: u16 = 201;
    const ID_VALORANT: u16 = 202;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    struct Gui {
        hwnd: Hwnd,
        list: Hwnd,
        name: Hwnd,
        valorant: Hwnd,
        status: Hwnd,
        buttons: Vec<Hwnd>,
        names: Vec<String>,
        busy: bool,
        store: Store,
    }

    thread_local! {
        static GUI: RefCell<Option<Gui>> = const { RefCell::new(None) };
    }

    fn set_text(h: Hwnd, s: &str) {
        unsafe { SetWindowTextW(h, wide(s).as_ptr()) };
    }

    fn get_text(h: Hwnd) -> String {
        unsafe {
            let len = GetWindowTextLengthW(h);
            let mut buf = vec![0u16; len as usize + 1];
            let n = GetWindowTextW(h, buf.as_mut_ptr(), buf.len() as i32);
            String::from_utf16_lossy(&buf[..n.max(0) as usize])
        }
    }

    fn message_box(owner: Hwnd, text: &str, kind: u32) -> i32 {
        unsafe { MessageBoxW(owner, wide(text).as_ptr(), wide("SW1TCH").as_ptr(), kind) }
    }

    impl Gui {
        fn refresh(&mut self) {
            self.names = self.store.accounts();
            let cur = self.store.current();
            unsafe {
                SendMessageW(self.list, LB_RESETCONTENT, 0, 0);
                for n in &self.names {
                    let label = if cur.as_ref() == Some(n) { format!("{n}    (active)") } else { n.clone() };
                    SendMessageW(self.list, LB_ADDSTRING, 0, wide(&label).as_ptr() as isize);
                }
                if let Some(i) = cur.and_then(|c| self.names.iter().position(|n| *n == c)) {
                    SendMessageW(self.list, LB_SETCURSEL, i, 0);
                }
            }
        }

        fn selected(&self) -> Option<String> {
            let i = unsafe { SendMessageW(self.list, LB_GETCURSEL, 0, 0) };
            usize::try_from(i).ok().and_then(|i| self.names.get(i).cloned())
        }

        fn opts(&self) -> Opts {
            let checked = unsafe { SendMessageW(self.valorant, BM_GETCHECK, 0, 0) } == 1;
            Opts { launch: true, valorant: checked }
        }

        fn set_busy(&mut self, busy: bool) {
            self.busy = busy;
            for &b in self.buttons.iter().chain([&self.list]) {
                unsafe { EnableWindow(b, (!busy) as i32) };
            }
        }

        /// Run a slow action on a worker thread; the result comes back as WM_APP_DONE.
        fn run_bg(&mut self, label: &str, f: impl FnOnce(&Store) -> Result<String, String> + Send + 'static) {
            if self.busy {
                return;
            }
            self.set_busy(true);
            set_text(self.status, label);
            let store = self.store.clone();
            let hwnd = self.hwnd as isize;
            std::thread::spawn(move || {
                let result = Box::new(f(&store));
                unsafe { PostMessageW(hwnd as Hwnd, WM_APP_DONE, 0, Box::into_raw(result) as isize) };
            });
        }

        /// Handle a button / control notification. Returns an error to show in a dialog.
        fn command(&mut self, id: u16, code: u16) -> Option<String> {
            if self.busy {
                return None;
            }
            let opts = self.opts();
            match (id, code) {
                (ID_LIST, LBN_SELCHANGE) => {
                    if let Some(n) = self.selected() {
                        set_text(self.name, &n);
                    }
                }
                (ID_SWITCH, _) | (ID_LIST, LBN_DBLCLK) => match self.selected() {
                    Some(n) => self.run_bg(&format!("Switching to '{n}'..."), move |s| ops::switch(s, opts, &n)),
                    None => set_text(self.status, "Pick an account to switch to."),
                },
                (ID_SAVE, _) => {
                    let n = get_text(self.name).trim().to_string();
                    if n.is_empty() {
                        set_text(self.status, "Type a name for the signed-in account, then click Save.");
                        unsafe { SetFocus(self.name) };
                    } else {
                        self.run_bg(&format!("Saving '{n}'..."), move |s| ops::save(s, opts, &n));
                    }
                }
                (ID_ADD, _) => self.run_bg("Signing out...", move |s| ops::new(s, opts)),
                (ID_RENAME, _) => {
                    let to = get_text(self.name).trim().to_string();
                    match self.selected() {
                        None => set_text(self.status, "Pick the account to rename."),
                        Some(_) if to.is_empty() => set_text(self.status, "Type the new name first."),
                        Some(from) => match ops::rename(&self.store, &from, &to) {
                            Ok(m) => {
                                set_text(self.status, &m);
                                self.refresh();
                            }
                            Err(e) => return Some(e),
                        },
                    }
                }
                _ => {}
            }
            None
        }
    }

    /// Borrow the GUI state if it is free. Child controls can notify the main
    /// window while it is already handling a message; those calls are skipped.
    fn with_gui<R>(f: impl FnOnce(&mut Gui) -> R) -> Option<R> {
        GUI.with(|g| g.try_borrow_mut().ok().and_then(|mut g| g.as_mut().map(f)))
    }

    unsafe extern "system" fn wnd_proc(h: Hwnd, msg: u32, w: usize, l: isize) -> isize {
        match msg {
            WM_COMMAND => {
                let id = (w & 0xFFFF) as u16;
                let code = (w >> 16) as u16;
                if id == ID_REMOVE {
                    remove_selected(h);
                } else if let Some(Some(err)) = with_gui(|g| g.command(id, code)) {
                    message_box(h, &err, MB_ICONERROR);
                }
                0
            }
            WM_APP_DONE => {
                let result = *Box::from_raw(l as *mut Result<String, String>);
                let err = with_gui(|g| {
                    g.set_busy(false);
                    g.refresh();
                    match &result {
                        Ok(m) => set_text(g.status, m),
                        Err(_) => set_text(g.status, "Something went wrong."),
                    }
                });
                if let (Some(()), Err(e)) = (err, result) {
                    message_box(h, &e, MB_ICONERROR);
                }
                0
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(h, msg, w, l),
        }
    }

    /// Removing asks for confirmation, so it runs without holding the GUI borrow
    /// while the message box's own message loop is active.
    fn remove_selected(h: Hwnd) {
        let Some(Some(name)) = with_gui(|g| if g.busy { None } else { g.selected() }) else {
            with_gui(|g| set_text(g.status, "Pick the account to remove."));
            return;
        };
        let ask = format!("Remove the saved account '{name}'?\n\nYour Riot account itself is not affected.");
        if message_box(h, &ask, MB_YESNO | MB_ICONWARNING) != IDYES {
            return;
        }
        let err = with_gui(|g| match ops::remove(&g.store, &name) {
            Ok(m) => {
                set_text(g.status, &m);
                g.refresh();
                None
            }
            Err(e) => Some(e),
        });
        if let Some(Some(e)) = err {
            message_box(h, &e, MB_ICONERROR);
        }
    }

    pub fn run() {
        let store = match ops::open_store() {
            Ok(s) => s,
            Err(e) => {
                message_box(null_mut(), &e, MB_ICONERROR);
                return;
            }
        };
        unsafe {
            SetProcessDPIAware();
            let screen = GetDC(null_mut());
            let dpi = GetDeviceCaps(screen, LOGPIXELSY).max(96);
            ReleaseDC(null_mut(), screen);
            let s = |v: i32| v * dpi / 96;

            let instance = GetModuleHandleW(null());
            let class = wide("Sw1tchWindow");
            let wc = WndClassExW {
                size: std::mem::size_of::<WndClassExW>() as u32,
                style: 0,
                wnd_proc: Some(wnd_proc),
                cls_extra: 0,
                wnd_extra: 0,
                instance,
                icon: LoadIconW(null_mut(), 32512 as *const u16),
                cursor: LoadCursorW(null_mut(), 32512 as *const u16),
                background: (COLOR_BTNFACE + 1) as Handle,
                menu_name: null(),
                class_name: class.as_ptr(),
                icon_sm: null_mut(),
            };
            RegisterClassExW(&wc);

            let style = WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
            let mut r = Rect { left: 0, top: 0, right: s(372), bottom: s(300) };
            AdjustWindowRectEx(&mut r, style, 0, 0);
            let hwnd = CreateWindowExW(
                0, class.as_ptr(), wide("SW1TCH - Valorant accounts").as_ptr(), style,
                CW_USEDEFAULT, CW_USEDEFAULT, r.right - r.left, r.bottom - r.top,
                null_mut(), null_mut(), instance, null_mut(),
            );
            let font = CreateFontW(-s(12), 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 5, 0, wide("Segoe UI").as_ptr());

            let child = |ex: u32, class: &str, text: &str, extra: u32, id: u16, x: i32, y: i32, w: i32, h: i32| {
                let c = CreateWindowExW(
                    ex, wide(class).as_ptr(), wide(text).as_ptr(), WS_CHILD | WS_VISIBLE | extra,
                    s(x), s(y), s(w), s(h), hwnd, id as usize as Handle, instance, null_mut(),
                );
                SendMessageW(c, WM_SETFONT, font as usize, 1);
                c
            };

            let list = child(WS_EX_CLIENTEDGE, "LISTBOX", "",
                WS_TABSTOP | WS_VSCROLL | LBS_NOTIFY | LBS_NOINTEGRALHEIGHT, ID_LIST, 12, 12, 228, 186);
            let name = child(WS_EX_CLIENTEDGE, "EDIT", "", WS_TABSTOP | ES_AUTOHSCROLL, ID_NAME, 12, 206, 228, 24);
            SendMessageW(name, EM_SETCUEBANNER, 1, wide("Account name").as_ptr() as isize);
            let valorant = child(0, "BUTTON", "Start Valorant (untick for Riot Client only)",
                WS_TABSTOP | BS_AUTOCHECKBOX, ID_VALORANT, 12, 236, 348, 22);
            SendMessageW(valorant, BM_SETCHECK, 1, 0);
            let status = child(0, "STATIC", "", 0, 0, 12, 262, 348, 34);

            let mut buttons = Vec::new();
            let specs = [
                (ID_SWITCH, "Switch", BS_DEFPUSHBUTTON),
                (ID_SAVE, "Save current as", 0),
                (ID_ADD, "Add account", 0),
                (ID_RENAME, "Rename", 0),
                (ID_REMOVE, "Remove", 0),
            ];
            for (i, (id, text, extra)) in specs.into_iter().enumerate() {
                let y = 12 + i as i32 * 36 + if i > 0 { 8 } else { 0 };
                buttons.push(child(0, "BUTTON", text, WS_TABSTOP | extra, id, 252, y, 108, 28));
            }

            let mut gui = Gui { hwnd, list, name, valorant, status, buttons, names: Vec::new(), busy: false, store };
            gui.refresh();
            set_text(gui.status, if gui.names.is_empty() {
                "Sign in to the Riot Client with 'Stay signed in' ticked, type a name, then click Save current as."
            } else {
                "Double-click an account to switch."
            });
            GUI.with(|g| *g.borrow_mut() = Some(gui));

            const SW_SHOW: i32 = 5;
            #[link(name = "user32")]
            extern "system" {
                fn ShowWindow(h: Hwnd, cmd: i32) -> i32;
            }
            ShowWindow(hwnd, SW_SHOW);
            SetFocus(list);

            let mut msg: Msg = std::mem::zeroed();
            while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
                if IsDialogMessageW(hwnd, &mut msg) == 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }
    }
}
