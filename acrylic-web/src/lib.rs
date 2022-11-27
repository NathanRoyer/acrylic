use acrylic::app::Application;
use acrylic::node::Event;
use acrylic::node::EventType;
use acrylic::node::Direction;
use acrylic::bitmap::RGBA;
use acrylic::Spot;
use acrylic::Point;
use acrylic::Size;

extern "C" {
    fn raw_log(s: *const u8, l: usize);
    fn raw_set_request_url(s: *const u8, l: usize);
    fn raw_set_request_url_prefix(s: *const u8, l: usize);
    fn raw_set_buffer_address(
        framebuffer: *const u8,
    );
    fn raw_is_request_pending() -> usize;
}

pub fn log(s: &str) {
    unsafe { raw_log(s.as_ptr(), s.len()) };
}

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
        if let Some(data_request) = app.data_requests.last() {
            set_request_url(&data_request.name);
        }
    }
}

#[allow(dead_code)]
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
    let request = app.data_requests.len() - 1;
    let data = unsafe { RESPONSE_BYTES.as_ref().unwrap() };
    app.data_response(request, data).unwrap();
}

#[export_name = "drop_response_bytes"]
pub extern "C" fn drop_response_bytes() {
    unsafe {
        RESPONSE_BYTES = None;
    }
}

#[export_name = "discard_request"]
pub extern "C" fn discard_request(app: &mut Application) {
    app.data_requests.pop().unwrap();
}

pub static mut MAIN_FB: Option<Vec<u8>> = None;
pub static mut SCRATCH: Option<Vec<u8>> = None;
pub static mut FB_SIZE: Size = Size::zero();

#[export_name = "set_output_size"]
pub extern "C" fn set_output_size(app: &mut Application, w: usize, h: usize) {
    let fb_size = Size::new(w, h);
    app.set_fb_size(fb_size);
    let pixels = w * h;
    let subpx = pixels * RGBA;
    unsafe {
        FB_SIZE = fb_size;
        if MAIN_FB.is_some() {
            MAIN_FB.as_mut().unwrap().resize(subpx, 0);
        } else {
            MAIN_FB = Some(vec![0; subpx]);
            SCRATCH = Some(Vec::new());
        }
        raw_set_buffer_address(
            MAIN_FB.as_ref().unwrap().as_ptr(),
        );
    };
}

#[export_name = "frame"]
pub extern "C" fn frame(app: &mut Application, age_ms: usize) {
    app.set_age(age_ms);
    let size = unsafe { FB_SIZE };
    let mut spot = Spot {
        window: (Point::zero(), size, None),
        framebuffer: unsafe { &mut MAIN_FB.as_mut().unwrap() },
        fb_size: size,
    };
    app.render(&mut spot, &mut Vec::new());
    ensure_pending_request(app);
}

pub static mut TEXT_INPUT: [u8; 16] = [0; 16];
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
pub extern "C" fn send_dir_input(app: &mut Application, dir: usize) {
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
    ($path: literal, $init: block) => {
        #[export_name = "init"]
        pub extern "C" fn init() -> &'static Application {
            std::panic::set_hook(Box::new(|panic_info| {
                let dbg = format!("{}", panic_info);
                log(&dbg);
            }));
            platform::wasm_init($path, $init)
        }
    };
}
