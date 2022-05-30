use acrylic::app::sub_spot;
use acrylic::app::Application;
use acrylic::app::Style;
use acrylic::bitmap::RGBA;
use acrylic::node::Event;
use acrylic::node::EventType;
use acrylic::node::NodePath;
use acrylic::Point;
use acrylic::Size;
use acrylic::Spot;

use std::collections::HashMap;
use std::mem::swap;

extern "C" {
    fn raw_log(s: *const u8, l: usize);
    fn raw_set_request_url(s: *const u8, l: usize);
    fn raw_set_request_url_prefix(s: *const u8, l: usize);
    fn raw_update_blit(
        x: isize,
        y: isize,
        w: usize,
        h: usize,
        px: *const u8,
        d: usize,
        p: *const u8,
        l: usize,
    );
    fn raw_set_blit_dirty(p: *const u8, l: usize);
    fn raw_is_request_pending() -> usize;
}

pub fn log(s: &str) {
    unsafe { raw_log(s.as_ptr(), s.len()) };
}

pub fn set_blit_dirty(m: &str) {
    unsafe { raw_set_blit_dirty(m.as_ptr(), m.len()) };
}

pub fn update_blit(p: Point, s: Size, px: &[u8], d: usize, m: &str) {
    unsafe { raw_update_blit(p.x, p.y, s.w, s.h, px.as_ptr(), d, m.as_ptr(), m.len()) };
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
pub static mut BLITS_PIXELS: Option<HashMap<NodePath, (Spot, Vec<u8>)>> = None;
pub static mut BG_PIXELS: Option<Vec<u8>> = None;
pub static mut FRAME_SPOT: Option<Spot> = None;
pub static mut BG_DIRTY: bool = true;

pub fn blit<'a>(spot: &'a Spot, path: Option<&'a NodePath>) -> Option<(&'a mut [u8], usize, bool)> {
    if let Some(path) = path {
        let depth = path.len();
        let (position, size) = *spot;
        let total_pixels = size.w * size.h * RGBA;
        let (saved_spot, slice) = unsafe {
            let blits = BLITS_PIXELS.as_mut().unwrap();
            if let None = blits.get_mut(path) {
                let pixels = vec![0; total_pixels];
                let spot = (Point::zero(), Size::zero());
                blits.insert(path.clone(), (spot, pixels));
            }
            let (spot, vec) = blits.get_mut(path).unwrap();
            vec.resize(total_pixels, 0);
            (spot, &mut *vec)
        };
        let mut name = String::new();
        for i in path {
            if name.len() > 0 {
                name.push('-');
            }
            name += &format!("{}", i);
        }
        if *saved_spot != *spot {
            *saved_spot = *spot;
            update_blit(position, size, slice, depth, &name);
        } else {
            set_blit_dirty(&name);
        }
        Some((slice, 0, true))
    } else {
        unsafe { BG_DIRTY = true };
        let f_spot = unsafe { FRAME_SPOT.unwrap() };
        let background = unsafe { BG_PIXELS.as_mut().unwrap() };
        let background = background.as_mut_slice();
        let (slice, pitch) = sub_spot(background, 0, [&f_spot, spot])?;
        Some((slice, pitch, true))
    }
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
pub extern "C" fn set_output_size(app: &mut Application, w: usize, h: usize) -> *const u8 {
    let spot = (Point::zero(), Size::new(w, h));
    let bg_pixels = unsafe {
        FRAME_SPOT = Some(spot);
        let bg_pixels = BG_PIXELS.as_mut().unwrap();
        bg_pixels.resize(w * h * RGBA, 0);
        bg_pixels.as_ptr()
    };
    app.set_spot(spot);
    bg_pixels
}

#[export_name = "quick_action"]
pub extern "C" fn quick_action(app: &mut Application, x: isize, y: isize, action: usize) {
    let (event, t) = match action {
        1 => (Event::QuickAction1, EventType::QUICK_ACTION_1),
        2 => (Event::QuickAction2, EventType::QUICK_ACTION_2),
        3 => (Event::QuickAction3, EventType::QUICK_ACTION_3),
        4 => (Event::QuickAction4, EventType::QUICK_ACTION_4),
        5 => (Event::QuickAction5, EventType::QUICK_ACTION_5),
        6 => (Event::QuickAction6, EventType::QUICK_ACTION_6),
        _ => unreachable!(),
    };
    let p = Point::new(x, y);
    if let Some(path) = app.hit_test(p, t) {
        let _ = app.call_handler(&path, event);
    }
}

#[export_name = "frame"]
pub extern "C" fn frame(app: &mut Application) -> usize {
    let mut bg_dirty = false;
    swap(unsafe { &mut BG_DIRTY }, &mut bg_dirty);
    app.render();
    ensure_pending_request(app);
    match bg_dirty {
        true => 1,
        false => 0,
    }
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
    ]);
    unsafe {
        BG_PIXELS = Some(Vec::new());
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
