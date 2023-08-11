pub use acrylic::core::app::Application;
use acrylic::core::{rgb::RGBA8, event::UserInputEvent, visual::{Position, SignedPixels}};

use log::{error, set_logger, set_max_level, Record, LevelFilter, Level, Metadata};
use std::fmt::Write;
use core::str::from_utf8;

extern "C" {
    fn raw_error(s: *const u8, l: usize);
    fn raw_warn(s: *const u8, l: usize);
    fn raw_info(s: *const u8, l: usize);
    fn raw_debug(s: *const u8, l: usize);
    fn raw_trace(s: *const u8, l: usize);
    fn raw_set_request_url(s: *const u8, l: usize);
    fn raw_set_request_url_prefix(s: *const u8, l: usize);
    fn raw_set_buffer_address(
        framebuffer: *const u8,
    );
    fn raw_is_request_pending() -> usize;
}

struct ConsoleLog;

impl log::Log for ConsoleLog {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let mut s = String::new();
            let _ = write!(&mut s, "{}", record.args());
            unsafe {
                use Level::*;
                match record.level() {
                    Error => raw_error(s.as_ptr(), s.len()),
                    Warn => raw_warn(s.as_ptr(), s.len()),
                    Info => raw_info(s.as_ptr(), s.len()),
                    Debug => raw_debug(s.as_ptr(), s.len()),
                    Trace => raw_trace(s.as_ptr(), s.len()),
                }
            }
        }
    }

    fn flush(&self) {}
}

static LOGGER: ConsoleLog = ConsoleLog;

pub fn set_request_url(s: &str) {
    unsafe { raw_set_request_url(s.as_ptr(), s.len()) };
}

pub fn set_request_url_prefix(s: &str) {
    unsafe { raw_set_request_url_prefix(s.as_ptr(), s.len()) };
}

pub fn is_request_pending() -> bool {
    unsafe { raw_is_request_pending() != 0 }
}

pub fn ensure_pending_request(app: &Application) {
    if !is_request_pending() {
        if let Some(asset) = app.requested() {
            set_request_url(&asset);
        }
    }
}

pub static mut APPLICATION: Option<Application> = None;
pub static mut RESPONSE_BYTES: Option<Vec<u8>> = None;

#[export_name = "alloc_response_bytes"]
pub extern "C" fn alloc_response_bytes(len: usize) -> *const u8 {
    let mut vec = Vec::with_capacity(len);
    unsafe { vec.set_len(len) };
    let ptr = vec.as_ptr();
    unsafe { RESPONSE_BYTES = Some(vec) };
    ptr
}

#[export_name = "process_response"]
pub extern "C" fn process_response(app: &mut Application) {
    let data = unsafe { RESPONSE_BYTES.take().unwrap() };
    app.data_response(app.requested().unwrap(), data.into()).unwrap();
}

pub static mut MAIN_FB: Option<Vec<RGBA8>> = None;
pub static mut SCRATCH: Option<Vec<u8>> = None;
pub static mut FB_SIZE: (usize, usize) = (0, 0);

#[no_mangle]
pub extern "C" fn set_output_size(w: usize, h: usize) {
    let pixels = w * h;
    let black = RGBA8::new(0, 0, 0, 0);
    unsafe {
        FB_SIZE = (w, h);
        if MAIN_FB.is_some() {
            MAIN_FB.as_mut().unwrap().resize(pixels, black);
        } else {
            MAIN_FB = Some(vec![black; pixels]);
        }
        raw_set_buffer_address(MAIN_FB.as_ref().unwrap().as_ptr() as *mut _);
    };
}

#[export_name = "frame"]
pub extern "C" fn frame(app: &mut Application, _age_ms: usize) {
    let (fb_size, fb, _scratch) = unsafe { (FB_SIZE, &mut MAIN_FB, &mut SCRATCH) };
    app.render(fb_size, fb.as_mut().unwrap()).unwrap();
    ensure_pending_request(app);
}

pub static mut TEXT_INPUT: [u8; 16] = [0; 16];
pub static mut _FOCUS_GRABBED: bool = false;

#[export_name = "get_text_input_buffer"]
pub extern "C" fn get_text_input_buffer() -> *const u8 {
    unsafe { TEXT_INPUT.as_ptr() }
}

#[export_name = "send_text_input"]
pub extern "C" fn send_text_input(app: &mut Application, len: usize, replace: bool) {
    let slice = unsafe { &TEXT_INPUT[..len] };
    if let Ok(string) = from_utf8(slice) {
        let event = match replace {
            true => UserInputEvent::TextReplace(string),
            false => UserInputEvent::TextInsert(string),
        };

        if let Some(node_key) = app.get_focused_node() {
            app.handle_user_input(node_key, &event).unwrap();
        }
    }
}

#[export_name = "send_text_delete"]
pub extern "C" fn send_text_delete(app: &mut Application, delete: isize) {
    let event = UserInputEvent::TextDelete(delete);
    if let Some(node_key) = app.get_focused_node() {
        app.handle_user_input(node_key, &event).unwrap();
    }
}
/*
#[export_name = "send_dir_input"]
pub extern "C" fn send_dir_input(_app: &mut Application, _dir: usize) {
    let direction = [
        Direction::Up,
        Direction::Left,
        Direction::Down,
        Direction::Right,
    ][dir];
    let _ = app.fire_event(&Event::DirInput(direction));
}*/

#[export_name = "quick_action"]
pub extern "C" fn quick_action(app: &mut Application, action: usize, x: usize, y: usize) {
    let input_event = match action {
        1 => UserInputEvent::QuickAction1,
        2 => UserInputEvent::QuickAction2,
        3 => UserInputEvent::QuickAction3,
        4 => UserInputEvent::QuickAction4,
        5 => UserInputEvent::QuickAction5,
        6 => UserInputEvent::QuickAction6,
        _ => unreachable!(),
    };

    /*let grabbed = unsafe { FOCUS_GRABBED };
    if action == 1 {
        if app.can_grab_focus(Some(EventType::QUICK_ACTION_1)) {
            unsafe { FOCUS_GRABBED = !grabbed };
            input_event = Event::FocusGrab(!grabbed);
        }
    }*/

    app.clear_focused_node().unwrap();

    let (x, y) = (SignedPixels::from_num(x), SignedPixels::from_num(y));
    let node_key = app.get_focused_node_or_at(Position::new(x, y));
    app.handle_user_input(node_key, &input_event).unwrap();

    /*if grabbed {
        app.pointing_at(unsafe { FOCUS_POINT });
        quick_action(app, action)
    }*/
}

pub fn pre_init() {
    set_max_level(LevelFilter::Trace);
    set_logger(&LOGGER).unwrap();
    std::panic::set_hook(Box::new(|panic_info| error!("PANIC! {}", panic_info)));
}

pub fn wasm_init(assets: &str, app: Application) -> &'static Application {
    unsafe {
        set_request_url_prefix(&String::from(assets));
        APPLICATION = Some(app);
        &APPLICATION.as_ref().unwrap()
    }
}

#[macro_export]
macro_rules! app {
    ($path:literal, $layout:expr, $callbacks:expr, $initial_state:expr) => {
        #[export_name = "init"]
        pub extern "C" fn init() -> &'static $crate::Application {
            platform::pre_init();
            let app = $crate::Application::new($layout().into(), $callbacks);
            platform::wasm_init($path, app)
        }
    };
}
