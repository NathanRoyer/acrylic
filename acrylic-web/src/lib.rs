use acrylic::app::Application;
use acrylic::app::Style;
use acrylic::bitmap::RGBA;
use acrylic::node::Event;
use acrylic::BlitKey;
use acrylic::Point;
use acrylic::Size;
use acrylic::Spot;

use std::collections::HashMap;

extern "C" {
    fn raw_log(s: *const u8, l: usize);
    fn raw_set_request_url(s: *const u8, l: usize);
    fn raw_set_request_url_prefix(s: *const u8, l: usize);
    fn raw_update_blit(
        x: isize,
        y: isize,
        w: isize,
        h: isize,
        px: *const u8,
        d: isize,
        s: *const u8,
        l: usize,
    );
    fn raw_set_blit_dirty(s: *const u8, l: usize);
    fn raw_is_request_pending() -> usize;
}

pub fn log(s: &str) {
    unsafe { raw_log(s.as_ptr(), s.len()) };
}

pub fn set_blit_dirty(h: u64) {
    let s = format!("{:#02X}", h);
    unsafe { raw_set_blit_dirty(s.as_ptr(), s.len()) };
}

pub fn update_blit(p: Point, s: Size, px: &[u8], d: usize, hash: u64) {
    let w = s.w as isize;
    let h = s.h as isize;
    let d = d as isize;
    let s = format!("{:#02X}", hash);
    unsafe { raw_update_blit(p.x, p.y, w, h, px.as_ptr(), d, s.as_ptr(), s.len()) };
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
pub static mut BLITS_PIXELS: Option<HashMap<BlitKey, (Spot, Vec<u8>)>> = None;

pub fn blit(spot: Spot, key: BlitKey) -> Option<(&'static mut [u8], usize, bool)> {
    let (position, size) = spot;
    let (depth, hash) = match key {
        BlitKey::Node(depth, hash) => (depth, hash),
        BlitKey::Overlay => (0, 0),
        BlitKey::Background => (999999, u64::MAX),
    };
    let (saved_spot, slice) = unsafe {
        let total_pixels = size.w * size.h * RGBA;
        let blits = BLITS_PIXELS.as_mut().unwrap();
        if let None = blits.get(&key) {
            let pixels = vec![0; total_pixels];
            let spot = (Point::zero(), Size::zero());
            blits.insert(key, (spot, pixels));
        }
        let (spot, vec) = blits.get_mut(&key).unwrap();
        vec.resize(total_pixels, 0);
        (spot, vec.as_mut_slice())
    };
    if *saved_spot != spot {
        *saved_spot = spot;
        update_blit(position, size, slice, depth, hash);
    } else {
        set_blit_dirty(hash);
    }
    Some((slice, 0, true))
}

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
    let request = app.data_requests.pop().unwrap();
    let node = app.get_node(&request.node).unwrap();
    let mut node = node.lock().unwrap();
    let data = unsafe { RESPONSE_BYTES.as_ref().unwrap() };
    let _ = node.loaded(app, &request.node, &request.name, 0, data);
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

#[export_name = "set_output_size"]
pub extern "C" fn set_output_size(app: &mut Application, w: usize, h: usize) {
    let spot = (Point::zero(), Size::new(w, h));
    app.set_spot(spot);
}

#[export_name = "pointing_at"]
pub extern "C" fn pointing_at(app: &mut Application, x: isize, y: isize) {
    app.pointing_at(Point::new(x, y));
}

#[export_name = "quick_action"]
pub extern "C" fn quick_action(app: &mut Application, action: usize) {
    let event = match action {
        1 => Event::QuickAction1,
        2 => Event::QuickAction2,
        3 => Event::QuickAction3,
        4 => Event::QuickAction4,
        5 => Event::QuickAction5,
        6 => Event::QuickAction6,
        _ => unreachable!(),
    };
    let _ = app.fire_event(&event);
}

#[export_name = "frame"]
pub extern "C" fn frame(app: &mut Application, age_ms: usize) {
    app.set_age(age_ms);
    app.render();
    ensure_pending_request(app);
}

pub fn wasm_init(assets: &str, mut app: Application) -> &'static Application {
    app.set_styles(vec![
        Style {
            background: [47, 49, 54, 255],
            foreground: [220, 221, 222, 255],
            border: [0; RGBA],
        },
        Style {
            background: [32, 34, 37, 255],
            foreground: [255; RGBA],
            border: [0; RGBA],
        },
        Style {
            background: [54, 57, 63, 255],
            foreground: [220, 221, 222, 255],
            border: [0; RGBA],
        },
        Style {
            background: [59, 165, 93, 255],
            foreground: [255; RGBA],
            border: [0; RGBA],
        },
        Style {
            background: [220, 220, 220, 255],
            foreground: [40, 40, 40, 255],
            border: [0; RGBA],
        },
    ]);
    unsafe {
        set_request_url_prefix(&String::from(assets));
        BLITS_PIXELS = Some(HashMap::new());
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
