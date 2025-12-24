#![no_std]
#![no_main]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use linked_list_allocator::LockedHeap;
use log::info;
use uefi::prelude::*;
use uefi::proto::console::gop::GraphicsOutput;
use uefi::proto::console::text::Key;
use uefi::proto::media::file::{Directory, FileAttribute, FileMode, FileType, RegularFile};
use uefi::proto::media::fs::SimpleFileSystem;
use uefi::proto::loaded_image::LoadedImage;
use uefi::table::boot::{AllocateType, MemoryType};
use uefi::table::runtime::ResetType;

mod font8x8;

const HEAP_PAGES: usize = 128;
const BG_COLOR: (u8, u8, u8) = (12, 16, 26);
const WINDOW_COLOR: (u8, u8, u8) = (26, 34, 52);
const TEXT_COLOR: (u8, u8, u8) = (214, 214, 214);
const TITLE_COLOR: (u8, u8, u8) = (118, 189, 230);
const PROMPT_COLOR: (u8, u8, u8) = (136, 224, 144);

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

#[entry]
fn efi_main(image_handle: Handle, mut st: SystemTable<Boot>) -> Status {
    uefi_services::init(&mut st).expect("failed to init services");
    init_heap(&mut st);

    info!("RAMOS booting...");

    let bs = st.boot_services();
    let mut gop = unsafe { &mut *bs.locate_protocol::<GraphicsOutput>().unwrap().get() };
    let (width, height, stride, px_format, bgr) = framebuffer_info(gop);
    let fb = gop.frame_buffer();
    let mut shell = Shell::new(width, height, stride, px_format, bgr, fb.as_mut_ptr(), fb.size());

    let mut fs = open_fs(bs, image_handle);
    let mut state = load_state(&mut fs, &mut shell);

    shell.println("Welcome to RAMOS meme OS (UEFI)");
    shell.println("Type 'help' for commands.");
    if !state.hints_shown {
        shell.println("Hint: the vault remembers the curious.");
        state.hints_shown = true;
    }

    shell.redraw(&state);

    loop {
        if let Some(key) = read_key(bs) {
            match key {
                Key::Printable(ch) => {
                    shell.push_char(ch);
                }
                Key::Special(uefi::proto::console::text::ScanCode::BACKSPACE) => shell.backspace(),
                Key::Special(uefi::proto::console::text::ScanCode::ENTER) => {
                    let input = shell.take_input();
                    if !input.is_empty() {
                        state.history.push(input.clone());
                    }
                    shell.execute(input, &mut state, &mut fs, &mut st);
                }
                Key::Special(uefi::proto::console::text::ScanCode::ESCAPE) => {
                    shell.clear_input();
                }
                _ => {}
            }
            shell.redraw(&state);
        } else {
            bs.stall(1_000);
        }
    }
}

fn init_heap(st: &mut SystemTable<Boot>) {
    let pages = HEAP_PAGES;
    let bytes = pages * 4096;
    let ptr = st
        .boot_services()
        .allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, pages)
        .expect("alloc heap") as usize;
    unsafe {
        ALLOCATOR.lock().init(ptr as *mut u8, bytes);
    }
}

fn framebuffer_info(gop: &GraphicsOutput) -> (usize, usize, usize, usize, bool) {
    let mode = gop.current_mode_info();
    let res = mode.resolution();
    let stride = mode.stride();
    let bgr = matches!(mode.pixel_format(), uefi::proto::console::gop::PixelFormat::Bgr);
    (res.0, res.1, stride, 4, bgr)
}

fn open_fs(bs: &BootServices, image: Handle) -> Directory {
    let loaded_image = unsafe { &mut *bs.handle_protocol::<LoadedImage>(image).unwrap().get() };
    let fs_handle = loaded_image.device();
    let fs = unsafe { &mut *bs.handle_protocol::<SimpleFileSystem>(fs_handle).unwrap().get() };
    fs.open_volume().expect("open fs")
}

fn read_key(bs: &BootServices) -> Option<Key> {
    let stdin = bs.stdin();
    let events = [stdin.wait_for_key_event().unsafe_clone()];
    let _ = bs.wait_for_event(&events).ok()?;
    stdin.read_key().transpose().ok().flatten()
}

#[derive(Clone)]
struct PersistedState {
    vars: BTreeMap<String, String>,
    history: Vec<String>,
    hints_shown: bool,
}

impl PersistedState {
    fn new() -> Self {
        let mut vars = BTreeMap::new();
        vars.insert("user".into(), "ramos".into());
        vars.insert("host".into(), "ramos".into());
        Self {
            vars,
            history: Vec::new(),
            hints_shown: false,
        }
    }
}

struct Shell<'a> {
    width: usize,
    height: usize,
    stride: usize,
    px_format: usize,
    bgr: bool,
    buffer: &'a mut [u8],
    lines: Vec<String>,
    input: String,
}

impl<'a> Shell<'a> {
    fn new(
        width: usize,
        height: usize,
        stride: usize,
        px_format: usize,
        bgr: bool,
        fb_ptr: *mut u8,
        fb_size: usize,
    ) -> Self {
        let buffer = unsafe { core::slice::from_raw_parts_mut(fb_ptr, fb_size) };
        Self {
            width,
            height,
            stride,
            px_format,
            bgr,
            buffer,
            lines: Vec::new(),
            input: String::new(),
        }
    }

    fn redraw(&mut self, state: &PersistedState) {
        self.fill_rect(0, 0, self.width, self.height, BG_COLOR);
        let margin = 20;
        let win_w = self.width - margin * 2;
        let win_h = self.height - margin * 2;
        self.fill_rect(margin, margin, win_w, win_h, WINDOW_COLOR);
        self.fill_rect(margin, margin, win_w, 24, TITLE_COLOR);
        self.draw_text(margin + 8, margin + 6, "RAMOS :: meme UEFI shell", TEXT_COLOR, TITLE_COLOR);

        let start_y = margin + 32;
        let max_lines = (win_h - 64) / 10;
        let start_line = self.lines.len().saturating_sub(max_lines);
        for (idx, line) in self.lines.iter().skip(start_line).enumerate() {
            let y = start_y + idx * 10;
            self.draw_text(margin + 8, y, line, TEXT_COLOR, WINDOW_COLOR);
        }

        let prompt = format!(
            "{}@{}:> {}",
            state.vars.get("user").map(|s| s.as_str()).unwrap_or("ramos"),
            state.vars.get("host").map(|s| s.as_str()).unwrap_or("ramos"),
            self.input
        );
        let prompt_y = start_y + max_lines * 10;
        self.draw_text(margin + 8, prompt_y, &prompt, PROMPT_COLOR, WINDOW_COLOR);
    }

    fn fill_rect(&mut self, x: usize, y: usize, w: usize, h: usize, color: (u8, u8, u8)) {
        for row in y..(y + h) {
            let base = row * self.stride * self.px_format;
            for col in x..(x + w) {
                let idx = base + col * self.px_format;
                if idx + 3 > self.buffer.len() {
                    continue;
                }
                self.buffer[idx] = color.2;
                self.buffer[idx + 1] = color.1;
                self.buffer[idx + 2] = color.0;
            }
        }
    }

    fn draw_text(&mut self, x: usize, y: usize, text: &str, fg: (u8, u8, u8), bg: (u8, u8, u8)) {
        let mut cursor_x = x;
        for ch in text.chars() {
            if cursor_x + 8 >= self.width {
                break;
            }
            font8x8::render_char(
                self.buffer,
                self.stride,
                self.px_format,
                self.bgr,
                cursor_x,
                y,
                fg,
                bg,
                ch,
            );
            cursor_x += 8;
        }
    }

    fn println(&mut self, text: &str) {
        self.lines.push(text.into());
    }

    fn push_char(&mut self, ch: char) {
        if ch.is_ascii_graphic() || ch == ' ' {
            self.input.push(ch);
        }
    }

    fn backspace(&mut self) {
        self.input.pop();
    }

    fn clear_input(&mut self) {
        self.input.clear();
    }

    fn take_input(&mut self) -> String {
        let mut out = String::new();
        core::mem::swap(&mut out, &mut self.input);
        out
    }

    fn execute(&mut self, line: String, state: &mut PersistedState, fs: &mut Directory, st: &mut SystemTable<Boot>) {
        let prompt = format!("{}@{}:> {}", state.vars.get("user").map(|s| s.as_str()).unwrap_or("ramos"), state.vars.get("host").map(|s| s.as_str()).unwrap_or("ramos"), line);
        self.println(&prompt);
        if line.is_empty() {
            return;
        }
        let mut parts = line.split_whitespace();
        if let Some(cmd) = parts.next() {
            match cmd {
                "help" => {
                    self.println("Commands: help, about, clear, echo, set, get, vars, save, load, reboot, shutdown");
                }
                "about" => {
                    self.println("RAMOS is a Rust UEFI meme OS with a hand-drawn UI.");
                }
                "clear" => {
                    self.lines.clear();
                }
                "echo" => {
                    let rest: String = parts.collect::<Vec<_>>().join(" ");
                    self.println(&rest);
                }
                "set" => {
                    let key = parts.next();
                    let val = parts.next();
                    match (key, val) {
                        (Some(k), Some(v)) => {
                            state.vars.insert(k.into(), v.into());
                            self.println("ok");
                        }
                        _ => self.println("usage: set <key> <value>"),
                    }
                }
                "get" => {
                    if let Some(k) = parts.next() {
                        if let Some(v) = state.vars.get(k) {
                            self.println(v);
                        } else {
                            self.println("(unset)");
                        }
                    } else {
                        self.println("usage: get <key>");
                    }
                }
                "vars" => {
                    for (k, v) in state.vars.iter() {
                        if k.starts_with('_') {
                            continue;
                        }
                        self.println(&format!("{}={}", k, v));
                    }
                }
                "history" => {
                    for (idx, h) in state.history.iter().enumerate() {
                        self.println(&format!("{}: {}", idx, h));
                    }
                }
                "save" => {
                    if let Err(e) = save_state(state, fs) {
                        self.println(&format!("save failed: {:?}", e));
                    } else {
                        self.println("state saved");
                    }
                }
                "load" => {
                    *state = load_state(fs, self);
                    self.println("state loaded");
                }
                "reboot" => {
                    st.runtime_services().reset(ResetType::COLD, Status::SUCCESS, None);
                }
                "shutdown" => {
                    st.runtime_services().reset(ResetType::SHUTDOWN, Status::SUCCESS, None);
                }
                "flag" => {
                    if let Some(secret) = state.vars.get("_vault") {
                        if let Some(decoded) = decode_base64(secret) {
                            self.println(&decoded);
                        } else {
                            self.println("vault sealed");
                        }
                    } else {
                        self.println("nothing here");
                    }
                }
                _ => {
                    self.println("unknown command");
                }
            }
        }
    }
}

fn decode_base64(data: &str) -> Option<String> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let mut chunk = [0u8; 4];
    let mut idx = 0;
    for b in data.bytes() {
        if b == b'=' {
            break;
        }
        if let Some(pos) = TABLE.iter().position(|c| *c == b) {
            chunk[idx] = pos as u8;
            idx += 1;
            if idx == 4 {
                out.push((chunk[0] << 2) | (chunk[1] >> 4));
                out.push((chunk[1] << 4) | (chunk[2] >> 2));
                out.push((chunk[2] << 6) | chunk[3]);
                idx = 0;
            }
        }
    }
    if idx == 3 {
        out.push((chunk[0] << 2) | (chunk[1] >> 4));
        out.push((chunk[1] << 4) | (chunk[2] >> 2));
    } else if idx == 2 {
        out.push((chunk[0] << 2) | (chunk[1] >> 4));
    }
    String::from_utf8(out).ok()
}

fn ensure_dir(fs: &mut Directory, name: &str) {
    if fs.open(name, FileMode::Read, FileAttribute::DIRECTORY).is_err() {
        let _ = fs.create(name, FileMode::CreateReadWrite, FileAttribute::DIRECTORY);
    }
}

fn save_state(state: &PersistedState, fs: &mut Directory) -> Result<(), uefi::Error> {
    ensure_dir(fs, "EFI");
    let mut efi_dir = match fs.open("EFI", FileMode::Read, FileAttribute::DIRECTORY)? {
        FileType::Dir(d) => d,
        _ => return Err(uefi::Status::ACCESS_DENIED.into()),
    };
    ensure_dir(&mut efi_dir, "RAMOS");
    let mut target = match efi_dir.open("RAMOS", FileMode::Read, FileAttribute::DIRECTORY)? {
        FileType::Dir(d) => d,
        _ => return Err(uefi::Status::ACCESS_DENIED.into()),
    };
    let mut file = match target.open("state.txt", FileMode::CreateReadWrite, FileAttribute::empty())? {
        FileType::Regular(f) => f,
        _ => return Err(uefi::Status::ACCESS_DENIED.into()),
    };
    file.set_position(0);
    let mut data = String::new();
    for (k, v) in state.vars.iter() {
        data.push_str("kv:");
        data.push_str(k);
        data.push('=');
        data.push_str(v);
        data.push('\n');
    }
    for h in state.history.iter() {
        data.push_str("h:");
        data.push_str(h);
        data.push('\n');
    }
    if state.hints_shown {
        data.push_str("hint:1\n");
    }
    let _ = file.write(data.as_bytes());
    let _ = file.flush();
    Ok(())
}

fn load_state(fs: &mut Directory, shell: &mut Shell) -> PersistedState {
    ensure_dir(fs, "EFI");
    let mut efi_dir = match fs.open("EFI", FileMode::Read, FileAttribute::DIRECTORY) {
        Ok(FileType::Dir(d)) => d,
        _ => return default_state(shell),
    };
    ensure_dir(&mut efi_dir, "RAMOS");
    let mut target = match efi_dir.open("RAMOS", FileMode::Read, FileAttribute::DIRECTORY) {
        Ok(FileType::Dir(d)) => d,
        _ => return default_state(shell),
    };
    match target.open("state.txt", FileMode::Read, FileAttribute::empty()) {
        Ok(FileType::Regular(mut f)) => parse_state(&mut f, shell),
        _ => {
            let mut state = default_state(shell);
            embed_flag(&mut state);
            state
        }
    }
}

fn default_state(shell: &mut Shell) -> PersistedState {
    let mut state = PersistedState::new();
    shell.println("No prior state found. Fresh session.");
    embed_flag(&mut state);
    state
}

fn embed_flag(state: &mut PersistedState) {
    let encoded = "UkFNT1N7RjB1bmRfM3ZlbjNfenJfc3Qwbmx5X2luX3RoZV9mdXR1cmV9";
    state.vars.insert("_vault".into(), encoded.into());
    state.history.push("echo the vault lives under hidden keys".into());
    state.history.push("echo base64 unlocks forgotten things".into());
}

fn parse_state(file: &mut RegularFile, shell: &mut Shell) -> PersistedState {
    let mut buf = [0u8; 4096];
    let size = file.read(&mut buf).unwrap_or(0);
    let content = core::str::from_utf8(&buf[..size]).unwrap_or("");
    let mut state = PersistedState::new();
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("kv:") {
            if let Some((k, v)) = rest.split_once('=') {
                state.vars.insert(k.into(), v.into());
            }
        } else if let Some(hist) = line.strip_prefix("h:") {
            state.history.push(hist.into());
        } else if line.starts_with("hint:") {
            state.hints_shown = true;
        }
    }
    if !state.vars.contains_key("_vault") {
        embed_flag(&mut state);
    }
    state
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    let _ = info;
    loop {}
}
