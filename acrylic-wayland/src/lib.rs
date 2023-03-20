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

use simple_logger::SimpleLogger;

pub use acrylic::core::{app::Application, state::parse_state};
use acrylic::core::rgb::FromSlice as _;

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
    pub fn new(shm: &WlShm, size: (usize, usize)) -> Self {
        let len = 4 * size.0 * size.1;
        let file = tempfile().unwrap();
        file.set_len(len as u64).unwrap();
        let pool = shm.create_pool(file.as_raw_fd(), len as i32);
        let fmt = Format::Abgr8888;
        let (w, h) = (size.0 as i32, size.1 as i32);
        let buffer = pool.create_buffer(0, w, h, w * 4, fmt);
        let data = unsafe { MmapOptions::new().len(len).map_mut(&file).unwrap() };
        Self {
            pool_size: len,
            file,
            pool,
            buffer,
            data,
        }
    }

    pub fn resize(&mut self, size: (usize, usize)) {
        let len = 4 * size.0 * size.1;
        if len > self.pool_size {
            self.file.set_len(len as u64).unwrap();
            self.pool.resize(len as i32);
            self.pool_size = len;
        }
        self.buffer.destroy();
        let fmt = Format::Abgr8888;
        let (w, h) = (size.0 as i32, size.1 as i32);
        self.buffer = self.pool.create_buffer(0, w, h, w * 4, fmt);
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
    pub size: (usize, usize),
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
                    let new_size = match (width, height) {
                        (0, 0) => (DEFAULT_W, DEFAULT_H),
                        _ => (width as usize, height as usize),
                    };
                    if app.size != new_size {
                        app.size = new_size;
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
        let size = (DEFAULT_W, DEFAULT_H);
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
        /*let age = self.acrylic_app_dob.elapsed();
        self.acrylic_app.set_age(age.as_millis() as usize);*/

        while let Some(asset) = self.acrylic_app.requested() {
            println!("loading {}", asset);
            let data = read(&format!("{}{}", &self.assets, asset)).unwrap();
            self.acrylic_app.data_response(asset, data).unwrap();
        }

        let fb = self.frame_buffer.data.as_rgba_mut();
        self.acrylic_app.render(self.size, fb, 0, 0, 0, false);
        self.ready_to_draw = true;
    }

    pub fn request_frame(&mut self) {
        self.surface.frame().quick_assign(|_iface, _event, mut data| {
            let app = data.get::<WaylandApp>().unwrap();
            app.frame();
            app.surface.attach(Some(&app.frame_buffer.buffer), 0, 0);
            app.surface.damage(0, 0, app.size.0 as i32, app.size.1 as i32);
            app.surface.commit();
        });
    }
}

pub fn run(app: Application, assets: &str) {
    SimpleLogger::new().init().unwrap();

    let (mut app, mut event_queue) = WaylandApp::new(app, assets.into());
    while !app.closed {
        event_queue
            .sync_roundtrip(
                &mut app,
                |_a, _, _| ()// println!("ignored {:?}", _a)
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
    ($path:literal, $layout:expr, $initial_state:expr) => {
        fn main() {
            let app = $crate::Application::new($layout().into(), Vec::new());
            platform::run(app, $path);
        }
    };
}
