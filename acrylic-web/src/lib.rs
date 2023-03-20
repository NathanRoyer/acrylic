pub use acrylic::core::{app::Application, state::parse_state};
use acrylic::core::rgb::RGBA8;

use log::{error, set_logger, set_max_level, Record, LevelFilter, Level, Metadata};
use std::fmt::Write;

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
    app.data_response(app.requested().unwrap(), data).unwrap();
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
pub extern "C" fn frame(app: &mut Application, _age_ms: usize, mx: usize, my: usize, wheel_delta: isize, click: usize) {
    let (fb_size, fb, _scratch) = unsafe { (FB_SIZE, &mut MAIN_FB, &mut SCRATCH) };
    app.render(fb_size, fb.as_mut().unwrap(), mx, my, wheel_delta, click != 0);
    ensure_pending_request(app);
}

/*pub static mut TEXT_INPUT: [u8; 16] = [0; 16];
pub static mut FOCUS_GRABBED: bool = false;
pub static mut FOCUS_POINT: Point = Point {
    x: 0,
    y: 0,
};

#[export_name = "get_text_input_buffer"]
pub extern "C" fn get_text_input_buffer() -> *const u8 {
    unsafe { TEXT_INPUT.as_ptr() }
}

#[export_name = "send_text_input"]
pub extern "C" fn send_text_input(app: &mut Application, len: usize, replace: bool) {
    let bytes = unsafe { &TEXT_INPUT[..len] }.to_vec();
    if let Ok(string) = String::from_utf8(bytes) {
        let event = match replace {
            true => Event::TextReplace(string),
            false => Event::TextInsert(string),
        };
        let _ = app.fire_event(&event);
    }
}

#[export_name = "send_text_delete"]
pub extern "C" fn send_text_delete(app: &mut Application, delete: isize) {
    let _ = app.fire_event(&Event::TextDelete(delete));
}

#[export_name = "send_dir_input"]
pub extern "C" fn send_dir_input(_app: &mut Application, _dir: usize) {
    let direction = [
        Direction::Up,
        Direction::Left,
        Direction::Down,
        Direction::Right,
    ][dir];
    let _ = app.fire_event(&Event::DirInput(direction));
}

#[export_name = "pointing_at"]
pub extern "C" fn pointing_at(app: &mut Application, x: isize, y: isize) {
    let p = Point::new(x, y);
    if unsafe { !FOCUS_GRABBED } {
        app.pointing_at(p);
    }
    unsafe { FOCUS_POINT = p };
}

#[export_name = "quick_action"]
pub extern "C" fn quick_action(app: &mut Application, action: usize) {
    let mut event = match action {
        1 => Event::QuickAction1,
        2 => Event::QuickAction2,
        3 => Event::QuickAction3,
        4 => Event::QuickAction4,
        5 => Event::QuickAction5,
        6 => Event::QuickAction6,
        _ => unreachable!(),
    };
    let grabbed = unsafe { FOCUS_GRABBED };
    if action == 1 {
        if app.can_grab_focus(Some(EventType::QUICK_ACTION_1)) {
            unsafe { FOCUS_GRABBED = !grabbed };
            event = Event::FocusGrab(!grabbed);
        }
    }
    let _ = app.fire_event(&event);
    if grabbed {
        app.pointing_at(unsafe { FOCUS_POINT });
        quick_action(app, action)
    }
}*/

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
    ($path:literal, $layout:expr, $initial_state:expr) => {
        #[export_name = "init"]
        pub extern "C" fn init() -> &'static $crate::Application {
            platform::pre_init();
            let app = $crate::Application::new($layout().into(), Vec::new());
            platform::wasm_init($path, app)
        }
    };
}
