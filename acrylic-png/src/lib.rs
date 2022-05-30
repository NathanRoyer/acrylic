use acrylic::app::Application;
use acrylic::app::Style;
use acrylic::bitmap::RGBA;
use acrylic::node::NodePath;
use acrylic::Point;
use acrylic::Size;
use acrylic::Spot;

use std::fs::read;
use std::fs::write;
use std::thread::sleep;
use std::time::Duration;
use std::time::Instant;

pub fn log(s: &str) {
    println!("{}", s);
}

use png::BitDepth::Eight;
use png::ColorType::Rgba;
use png::Encoder;

const PRE_RENDER: usize = 10;
const WIDTH: usize = 2000;
const HEIGHT: usize = 1200;
const DURATION: u64 = 5000;
const TARGET_FPS: u64 = 100000;
const PNG_NAME: &'static str = "output.png";

static mut PIXELS: [u8; WIDTH * HEIGHT * 4] = [127; WIDTH * HEIGHT * 4];

pub fn blit<'a>(
    spot: &'a Spot,
    _path: Option<&'a NodePath>,
) -> Option<(&'a mut [u8], usize, bool)> {
    let (position, size) = *spot;
    let (x, y) = (position.x as usize, position.y as usize);
    let pitch = 4 * (WIDTH - size.w);
    unsafe { Some((&mut PIXELS[4 * (x + HEIGHT * y)..], pitch, false)) }
}

pub fn run(assets: &str, mut app: Application) {
    app.set_styles(vec![
        Style {
            background: [50, 50, 50, 255],
            foreground: [0; RGBA],
            border: [0; RGBA],
        },
        Style {
            background: [100, 100, 100, 255],
            foreground: [0; RGBA],
            border: [0; RGBA],
        },
        Style {
            background: [50, 50, 250, 255],
            foreground: [0; RGBA],
            border: [0; RGBA],
        },
    ]);
    let size = Size::new(WIDTH, HEIGHT);
    app.set_spot((Point::zero(), size));

    for _ in 0..PRE_RENDER {
        app.render();
        while let Some(request) = app.data_requests.pop() {
            println!("loading {}{}", assets, request.name);
            let data = read(&format!("{}{}", assets, request.name)).unwrap();
            let node = app.get_node(&request.node).unwrap();
            let mut node = node.lock().unwrap();
            let _ = node.loaded(&mut app, &request.node, &request.name, 0, &data);
        }
    }

    let duration = Duration::from_millis(DURATION);
    let start = Instant::now();
    let mut then = start;
    let target_frame_time = 1000 / TARGET_FPS;
    let mut frames = 0;
    while (then - start) < duration {
        app.render();
        while let Some(request) = app.data_requests.pop() {
            println!("loading {}{}", assets, request.name);
            let data = read(&format!("{}{}", assets, request.name)).unwrap();
            let node = app.get_node(&request.node).unwrap();
            let mut node = node.lock().unwrap();
            let _ = node.loaded(&mut app, &request.node, &request.name, 0, &data);
        }
        let now = Instant::now();
        let elapsed = (now - then).as_millis() as u64;
        if elapsed < target_frame_time {
            let remaining = Duration::from_millis(target_frame_time - elapsed);
            sleep(remaining);
        }
        then = now;
        frames += 1;
    }

    println!("avg: {}ms", ((then - start) / frames).as_millis());

    let mut png_buf = Vec::new();
    {
        let mut encoder = Encoder::new(&mut png_buf, WIDTH as u32, HEIGHT as u32);
        encoder.set_color(Rgba);
        encoder.set_depth(Eight);
        let mut writer = encoder.write_header().unwrap();
        unsafe {
            writer.write_image_data(&PIXELS).unwrap();
        }
    }
    write(PNG_NAME, &png_buf).unwrap();
}

#[macro_export]
macro_rules! app {
    ($path: literal, $init: block) => {
        fn main() {
            platform::run($path, $init);
        }
    };
}
