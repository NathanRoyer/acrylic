use std::collections::HashMap;
pub use std::concat;
use std::fs::read;
use std::fs::File;
use std::time::Instant;
use std::os::unix::io::AsRawFd;

use wayland_client::protocol::wl_buffer::WlBuffer;
use wayland_client::protocol::wl_compositor::WlCompositor;
use wayland_client::protocol::wl_display::WlDisplay;
use wayland_client::protocol::wl_shm::Format;
use wayland_client::protocol::wl_shm::WlShm;
use wayland_client::protocol::wl_shm_pool::WlShmPool;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::Attached;
use wayland_client::Display;
use wayland_client::EventQueue;
use wayland_client::GlobalManager;
use wayland_client::Interface;
use wayland_client::Main;

use wayland_protocols::xdg_shell::client::xdg_surface::Event as XdgSurfaceEvent;
use wayland_protocols::xdg_shell::client::xdg_surface::XdgSurface;
use wayland_protocols::xdg_shell::client::xdg_toplevel::Event as XdgToplevelEvent;
use wayland_protocols::xdg_shell::client::xdg_toplevel::XdgToplevel;
use wayland_protocols::xdg_shell::client::xdg_wm_base::Event as XdgWmBaseEvent;
use wayland_protocols::xdg_shell::client::xdg_wm_base::XdgWmBase;

use acrylic::app::Application;
use acrylic::app::Style;
use acrylic::app::sub_spot;
use acrylic::app::for_each_line;
use acrylic::bitmap::RGBA;
use acrylic::BlitKey;
use acrylic::Point;
use acrylic::Size;
use acrylic::Spot;

use tempfile::tempfile;

use memmap::MmapMut;
use memmap::MmapOptions;

pub struct FrameBuffer {
    pool_size: usize,
    file: File,
    pool: Main<WlShmPool>,
    buffer: Main<WlBuffer>,
    data: MmapMut,
}

const DEFAULT_W: usize = 1000;
const DEFAULT_H: usize = 800;

impl FrameBuffer {
    pub fn new(shm: &WlShm, size: Size) -> Self {
        let len = RGBA * size.w * size.h;
        let file = tempfile().unwrap();
        file.set_len(len as u64).unwrap();
        let pool = shm.create_pool(file.as_raw_fd(), len as i32);
        let fmt = Format::Abgr8888;
        let (w, h) = (size.w as i32, size.h as i32);
        let buffer = pool.create_buffer(0, w, h, w * (RGBA as i32), fmt);
        let data = unsafe { MmapOptions::new().len(len).map_mut(&file).unwrap() };
        Self {
            pool_size: len,
            file,
            pool,
            buffer,
            data,
        }
    }

    pub fn resize(&mut self, size: Size) {
        let len = RGBA * size.w * size.h;
        if len > self.pool_size {
            self.file.set_len(len as u64).unwrap();
            self.pool.resize(len as i32);
            self.pool_size = len;
        }
        self.buffer.destroy();
        let fmt = Format::Abgr8888;
        let (w, h) = (size.w as i32, size.h as i32);
        self.buffer = self.pool.create_buffer(0, w, h, w * (RGBA as i32), fmt);
        self.data = unsafe { MmapOptions::new().len(len).map_mut(&self.file).unwrap() };
        self.data.fill(0);
    }
}

#[allow(dead_code)]
struct WaylandApp {
    pub display: Attached<WlDisplay>,
    pub global_manager: GlobalManager,
    pub compositor: Main<WlCompositor>,
    pub xdg_wm_base: Main<XdgWmBase>,
    pub shm: Main<WlShm>,
    pub frame_buffer: FrameBuffer,
    pub surface: Main<WlSurface>,
    pub xdg_surface: Main<XdgSurface>,
    pub xdg_toplevel: Main<XdgToplevel>,
    pub acrylic_app: Application,
    pub acrylic_app_dob: Instant,
    pub closed: bool,
    pub configured: bool,
    pub ready_to_draw: bool,
    pub size: Size,
    pub assets: String,
}

impl WaylandApp {
    pub fn new(acrylic_app: Application, assets: String) -> (Self, EventQueue) {
        let display = Display::connect_to_env().unwrap();
        let mut event_queue = display.create_event_queue();
        let display = display.attach(event_queue.token());
        let gm = GlobalManager::new(&display);
        event_queue
            .dispatch(&mut (), |_, _, _| unreachable!())
            .expect("Event dispatching Error!");

        let compositor = gm
            .instantiate_range::<WlCompositor>(0, WlCompositor::VERSION)
            .unwrap();
        let xdg_wm_base = gm
            .instantiate_range::<XdgWmBase>(0, XdgWmBase::VERSION)
            .unwrap();
        let shm = gm.instantiate_range::<WlShm>(0, WlShm::VERSION).unwrap();

        xdg_wm_base.quick_assign(|iface, event, _data| {
            if let XdgWmBaseEvent::Ping { serial } = event {
                iface.pong(serial);
            }
        });

        shm.quick_assign(|_iface, _event, _data| {
            // if let wayland_client::protocol::wl_shm::Event::Format { format } = event {
            // println!("{:?}, {}", format, format as u32);
            // }
        });

        let surface = compositor.create_surface();
        let xdg_surface = xdg_wm_base.get_xdg_surface(&surface);
        let xdg_toplevel = xdg_surface.get_toplevel();
        xdg_toplevel.quick_assign(|_iface, event, mut data| {
            let app = data.get::<WaylandApp>().unwrap();
            if let XdgToplevelEvent::Configure { width, height, .. } = event {
                if app.configured {
                    let (w, h) = match (width, height) {
                        (0, 0) => (DEFAULT_W, DEFAULT_H),
                        _ => (width as usize, height as usize),
                    };
                    app.size = Size::new(w, h);
                    let spot = (Point::zero(), app.size);
                    if app.acrylic_app.set_spot(spot) {
                        app.frame_buffer.resize(app.size);
                        app.surface.attach(Some(&app.frame_buffer.buffer), 0, 0);
                    }
                    app.ready_to_draw = true;
                    app.frame();
                    app.surface.commit();
                }
            } else if let XdgToplevelEvent::Close = event {
                app.closed = true;
            }
        });

        xdg_surface.quick_assign(|xdg_surface, event, mut data| {
            let app = data.get::<WaylandApp>().unwrap();
            if let XdgSurfaceEvent::Configure { serial } = event {
                // println!("surface cfg");
                app.configured = true;
                xdg_surface.ack_configure(serial);
                app.surface.attach(Some(&app.frame_buffer.buffer), 0, 0);
                app.surface.commit();
            }
        });

        let acrylic_app_dob = Instant::now();
        let size = Size::new(DEFAULT_W, DEFAULT_H);
        let frame_buffer = FrameBuffer::new(&shm, size);
        let app = WaylandApp {
            acrylic_app,
            acrylic_app_dob,
            display,
            global_manager: gm,
            compositor,
            xdg_wm_base,
            shm,
            frame_buffer,
            surface,
            xdg_surface,
            xdg_toplevel,
            size,
            closed: false,
            configured: false,
            ready_to_draw: false,
            assets,
        };
        app.surface.commit();
        (app, event_queue)
    }

    pub fn frame(&mut self) {
        let age = self.acrylic_app_dob.elapsed();
        self.acrylic_app.set_age(age.as_millis() as usize);
        while let Some(request) = self.acrylic_app.data_requests.pop() {
            println!("loading {}", request.name);
            let data = read(&format!("{}{}", &self.assets, request.name)).unwrap();
            let node = self.acrylic_app.get_node(&request.node).unwrap();
            let mut node = node.lock().unwrap();
            let _ = node.loaded(
                &mut self.acrylic_app,
                &request.node,
                &request.name,
                0,
                &data,
            );
        }
        self.acrylic_app.render();
        let blits = unsafe { BLITS_PIXELS.as_ref().unwrap() };
        let mut keys = blits.keys().collect::<Vec<&BlitKey>>();
        keys.sort();
        if let Some(bg_key) = keys.first() {
            if let Some(((_, size), pixels)) = blits.get(bg_key) {
                if self.size == *size {
                    self.frame_buffer.data.copy_from_slice(pixels);
                } else {
                    println!("wrong bg size: {:?}, {:?}", bg_key, size);
                }
            }
        }
        let keys = keys.get(1..).unwrap_or(&[]);
        let fb_pixels = &mut self.frame_buffer.data;
        for key in keys {
            if let Some((spot, src)) = blits.get(key) {
                let app_spot = (Point::zero(), self.size);
                if let Some(dst) = sub_spot(fb_pixels, 0, [app_spot, *spot]) {
                    let (slice, pitch) = dst;
                    let (mut a, mut b) = (0, 0);
                    let mut j = 0;
                    for_each_line(slice, spot.1, pitch, |_, line| {
                        for i in (0..line.len()).rev() {
                            let (dst, src) = (&mut line[i], &(src[j + i] as u32));
                            if (i % RGBA) == 3 {
                                a = *src as u32;
                                b = 255 - a;
                            }
                            *dst = ((*src * a + (*dst as u32) * b) / 255) as u8;
                        }
                        j += line.len();
                    });
                }
            }
        }
        self.ready_to_draw = true;
    }

    pub fn request_frame(&mut self) {
        self.surface.frame().quick_assign(|_iface, _event, mut data| {
            let app = data.get::<WaylandApp>().unwrap();
            app.frame();
            app.surface.attach(Some(&app.frame_buffer.buffer), 0, 0);
            app.surface.damage(0, 0, app.size.w as i32, app.size.h as i32);
            app.surface.commit();
        });
    }
}

pub static mut BLITS_PIXELS: Option<HashMap<BlitKey, (Spot, Vec<u8>)>> = None;

pub fn blit(spot: Spot, key: BlitKey) -> Option<(&'static mut [u8], usize, bool)> {
    let (_, size) = spot;
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
    *saved_spot = spot;
    Some((slice, 0, true))
}

pub fn run(mut app: Application, assets: &str) {
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
    unsafe { BLITS_PIXELS = Some(HashMap::new()) };
    let (mut app, mut event_queue) = WaylandApp::new(app, assets.into());
    while !app.closed {
        event_queue
            .sync_roundtrip(
                &mut app,
                |_a, _, _| println!("ignored {:?}", _a)
            )
            .expect("Event dispatching Error!");
        if app.ready_to_draw {
            app.ready_to_draw = false;
            app.request_frame();
        }
    }
}

#[macro_export]
macro_rules! app {
    ($path: literal, $init: block) => {
        fn main() {
            platform::run($init, $path);
        }
    };
}

pub fn log(message: &str) {
    println!("{}", message);
}
