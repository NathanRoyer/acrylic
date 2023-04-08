use std::{fs::{read, File}, os::unix::prelude::AsRawFd};

use wayland_client::protocol::{
    wl_buffer, wl_compositor, wl_keyboard, wl_registry, wl_seat,
    wl_shm, wl_shm_pool, wl_surface, wl_pointer, wl_callback,
};
use wayland_client::{Connection, Dispatch, QueueHandle, WEnum};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

use memmap::{MmapMut, MmapOptions};
use tempfile::tempfile;

use simple_logger::SimpleLogger;

pub use acrylic::core::{app::Application, state::parse_state};
use acrylic::core::rgb::FromSlice as _;

pub fn run(app: Application, assets: &str) {
    SimpleLogger::new().init().unwrap();

    let conn = Connection::connect_to_env().unwrap();

    let mut event_queue = conn.new_event_queue();
    let qhandle = event_queue.handle();

    let display = conn.display();
    display.get_registry(&qhandle, ());

    let mut state = State {
        base_surface: None,
        pool: None,
        wm_base: None,
        xdg_surface: None,
        fb: None,
        assets: assets.into(),
        app,
        configured: false,
        running: true,
        clicked: false,
        mouse: (0, 0),
    };

    println!("Starting the example window app, press <ESC> to quit.");

    while state.running {
        event_queue.blocking_dispatch(&mut state).unwrap();
    }
}

const DEFAULT_W: usize = 1000;
const DEFAULT_H: usize = 800;

struct FrameBuffer {
    mapping: MmapMut,
    buffer: wl_buffer::WlBuffer,
    file: File,
    pool_size: usize,
    width: usize,
    height: usize,
}

struct State {
    base_surface: Option<wl_surface::WlSurface>,
    pool: Option<wl_shm_pool::WlShmPool>,
    wm_base: Option<xdg_wm_base::XdgWmBase>,
    xdg_surface: Option<(xdg_surface::XdgSurface, xdg_toplevel::XdgToplevel)>,
    fb: Option<FrameBuffer>,
    assets: String,
    app: Application,
    configured: bool,
    running: bool,
    clicked: bool,
    mouse: (usize, usize),
}

impl State {
    fn init_xdg_surface(&mut self, qh: &QueueHandle<State>) {
        let wm_base = self.wm_base.as_ref().unwrap();
        let base_surface = self.base_surface.as_ref().unwrap();

        let xdg_surface = wm_base.get_xdg_surface(base_surface, qh, ());
        let toplevel = xdg_surface.get_toplevel(qh, ());
        toplevel.set_title("A fantastic window!".into());

        base_surface.commit();

        self.xdg_surface = Some((xdg_surface, toplevel));
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global { name, interface, .. } = event {
            match &interface[..] {
                "wl_compositor" => {
                    let compositor =
                        registry.bind::<wl_compositor::WlCompositor, _, _>(name, 1, qh, ());
                    let surface = compositor.create_surface(qh, ());
                    state.base_surface = Some(surface);

                    if state.wm_base.is_some() && state.xdg_surface.is_none() {
                        state.init_xdg_surface(qh);
                    }
                }
                "wl_shm" => {
                    let shm = registry.bind::<wl_shm::WlShm, _, _>(name, 1, qh, ());

                    let len = DEFAULT_W * DEFAULT_H * 4;

                    let file = tempfile().unwrap();
                    file.set_len(len as u64).unwrap();

                    let pool = shm.create_pool(file.as_raw_fd(), len as i32, qh, ());

                    let (init_w, init_h) = (DEFAULT_W as i32, DEFAULT_H as i32);
                    let buffer = pool.create_buffer(0, init_w, init_h, init_w * 4, wl_shm::Format::Abgr8888, qh, ());

                    let mut fb_data = unsafe { MmapOptions::new().len(len).map_mut(&file).unwrap() };
                    fb_data.fill(0);

                    state.pool = Some(pool.clone());
                    state.fb = Some(FrameBuffer {
                        mapping: fb_data,
                        buffer: buffer.clone(),
                        file,
                        pool_size: len,
                        width: DEFAULT_W,
                        height: DEFAULT_H,
                    });

                    if state.configured {
                        let surface = state.base_surface.as_ref().unwrap();
                        surface.frame(qh, ());
                        surface.attach(Some(&buffer), 0, 0);
                        surface.commit();
                    }
                }
                "wl_seat" => {
                    registry.bind::<wl_seat::WlSeat, _, _>(name, 1, qh, ());
                }
                "xdg_wm_base" => {
                    let wm_base = registry.bind::<xdg_wm_base::XdgWmBase, _, _>(name, 1, qh, ());
                    state.wm_base = Some(wm_base);

                    if state.base_surface.is_some() && state.xdg_surface.is_none() {
                        state.init_xdg_surface(qh);
                    }
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<wl_compositor::WlCompositor, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_compositor::WlCompositor,
        _: wl_compositor::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // wl_compositor has no event
    }
}

impl Dispatch<wl_surface::WlSurface, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_surface::WlSurface,
        _: wl_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // we ignore wl_surface events in this example
    }
}

impl Dispatch<wl_shm::WlShm, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_shm::WlShm,
        _: wl_shm::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // we ignore wl_shm events in this example
    }
}

impl Dispatch<wl_shm_pool::WlShmPool, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_shm_pool::WlShmPool,
        _: wl_shm_pool::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // we ignore wl_shm_pool events in this example
    }
}

impl Dispatch<wl_buffer::WlBuffer, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_buffer::WlBuffer,
        _: wl_buffer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // we ignore wl_buffer events in this example
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for State {
    fn event(
        _: &mut Self,
        wm_base: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            wm_base.pong(serial);
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, ()> for State {
    fn event(
        state: &mut Self,
        xdg_surface: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial, .. } = event {
            xdg_surface.ack_configure(serial);
            state.configured = true;
            let surface = state.base_surface.as_ref().unwrap();
            if let Some(fb) = &state.fb {
                surface.frame(qh, ());
                surface.attach(Some(&fb.buffer), 0, 0);
                surface.commit();
            }
        }
    }
}

impl Dispatch<xdg_toplevel::XdgToplevel, ()> for State {
    fn event(
        state: &mut Self,
        _: &xdg_toplevel::XdgToplevel,
        event: xdg_toplevel::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let xdg_toplevel::Event::Close {} = event {
            state.running = false;
        }

        if let xdg_toplevel::Event::Configure { width, height, .. } = event {
            let (fb, pool) = (state.fb.as_mut().unwrap(), state.pool.as_mut().unwrap());

            fb.width = width as usize;
            fb.height = height as usize;
            let len = fb.width * fb.height * 4;
            if len != 0 {
                if len > fb.pool_size {
                    fb.file.set_len(len as u64).unwrap();
                    pool.resize(len as i32);
                    fb.pool_size = len;
                }

                fb.buffer.destroy();

                let (w, h) = (width as i32, height as i32);
                fb.buffer = pool.create_buffer(0, w, h, w * 4, wl_shm::Format::Abgr8888, qh, ());

                fb.mapping = unsafe { MmapOptions::new().len(len).map_mut(&fb.file).unwrap() };
                fb.mapping.fill(0);
            }
        }
    }
}

impl Dispatch<wl_callback::WlCallback, ()> for State {
    fn event(
        state: &mut Self,
        _: &wl_callback::WlCallback,
        _event: wl_callback::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let Some(fb) = &mut state.fb {
            while let Some(asset) = state.app.requested() {
                println!("loading {}", asset);
                let data = read(&format!("{}{}", &state.assets, asset)).unwrap();
                state.app.data_response(asset, data.into_boxed_slice()).unwrap();
            }

            let size = (fb.width, fb.height);
            let (mx, my) = state.mouse;
            let damages = state.app.render(size, fb.mapping.as_rgba_mut(), mx, my, 0, state.clicked).unwrap();
            state.clicked = false;

            let surface = state.base_surface.as_ref().unwrap();
            surface.frame(qh, ());
            surface.attach(Some(&fb.buffer), 0, 0);

            for (position, size) in damages {
                // the render list checks that boundaries are respected
                let x = position.x.to_num();
                let y = position.y.to_num();
                let w = size.w.to_num();
                let h = size.h.to_num();
                surface.damage(x, y, w, h);
            }

            surface.commit();
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for State {
    fn event(
        _: &mut Self,
        seat: &wl_seat::WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities { capabilities: WEnum::Value(capabilities) } = event {
            if capabilities.contains(wl_seat::Capability::Keyboard) {
                seat.get_keyboard(qh, ());
            }
            if capabilities.contains(wl_seat::Capability::Pointer) {
                seat.get_pointer(qh, ());
            }
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for State {
    fn event(
        state: &mut Self,
        _: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_pointer::Event::Motion { surface_x, surface_y, .. } => {
                state.mouse = (surface_x as usize, surface_y as usize);
            },
            wl_pointer::Event::Button { button: 272, state: WEnum::Value(wl_pointer::ButtonState::Pressed), .. } => {
                state.clicked = true;
            },
            _ => println!("WlPointer: {:?}", event),
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for State {
    fn event(
        state: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_keyboard::Event::Key { key, .. } = event {
            if key == 1 {
                // ESC key
                state.running = false;
            }
        }
    }
}

#[macro_export]
macro_rules! app {
    ($path:literal, $layout:expr, $initial_state:expr) => {
        fn main() {
            $crate::run($crate::Application::new($layout().into(), []), $path);
        }
    };
}
