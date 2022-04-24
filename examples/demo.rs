use std::time::Instant;
use std::collections::HashMap;

use acrylic::flexbox;
use acrylic::tree::NodeKey;
use acrylic::tree::Tree;
use acrylic::tree::Axis;
use acrylic::tree::LengthPolicy;
use acrylic::text::Paragraph;
use acrylic::text::Font;
use acrylic::Point;
use acrylic::Size;
use acrylic::bitmap::Bitmap;
use acrylic::bitmap::RGBA;
use acrylic::application::rc_widget;
use acrylic::application::Application;

const TEXT: &'static str = "Mais, vous savez, moi je ne crois pas qu'il y ait de bonne ou de mauvaise situation. Moi, si je devais résumer ma vie aujourd'hui avec vous, je dirais que c'est d'abord des rencontres, Des gens qui m'ont tendu la main, peut-être à un moment où je ne pouvais pas, où j'étais seul chez moi. Et c'est assez curieux de se dire que les hasards, les rencontres forgent une destinée... Parce que quand on a le goût de la chose, quand on a le goût de la chose bien faite, Le beau geste, parfois on ne trouve pas l'interlocuteur en face, je dirais, le miroir qui vous aide à avancer. Alors ce n'est pas mon cas, comme je le disais là, puisque moi au contraire, j'ai pu ; Et je dis merci à la vie, je lui dis merci, je chante la vie, je danse la vie... Je ne suis qu'amour! Et finalement, quand beaucoup de gens aujourd'hui me disent : \"Mais comment fais-tu pour avoir cette humanité ?\", Eh bien je leur réponds très simplement, je leur dis que c'est ce goût de l'amour, Ce goût donc qui m'a poussé aujourd'hui à entreprendre une construction mécanique, Mais demain, qui sait, peut-être simplement à me mettre au service de la communauté, à faire le don, le don de soi...";
// const TEXT: &'static str = "Lol";

fn add_spacer(t: &mut Tree, p: &mut NodeKey, policy: LengthPolicy) {
	let mut c11 = t.add_node(Some(p), 3);
	t.set_node_policy(&mut c11, Some(policy));
	t.set_node_spot(&mut c11, Some((Point::zero(), Size::zero())));
}

fn main() {
	let mut app = Application::new(None, ());

	let font = Font::from_bytes(include_bytes!("../rsc/font.ttf"));

	let mut bmp_store = HashMap::new();
	let widget = rc_widget(read_png("rsc/castle-in-the-sky.png"));
	bmp_store.insert((0, 0), widget.clone());

	let mut p = app.tree.add_node(None, 10);
	app.tree.set_node_container(&mut p, Some(Axis::Vertical));
	app.tree.set_node_spot(&mut p, Some((Point::zero(), Size::new(1200, 1300))));

	add_spacer(&mut app.tree, &mut p, LengthPolicy::Fixed(60));

	let mut c1 = app.tree.add_node(Some(&mut p), 3);
	app.tree.set_node_container(&mut c1, Some(Axis::Horizontal));
	app.tree.set_node_policy(&mut c1, Some(LengthPolicy::AspectRatio(3.0)));
	app.tree.set_node_spot(&mut c1, Some((Point::zero(), Size::zero())));

	add_spacer(&mut app.tree, &mut c1, LengthPolicy::Available(0.5));

	let mut c12 = app.tree.add_node(Some(&mut c1), 3);
	app.tree.set_node_widget(&mut c12, Some(widget));
	app.tree.set_node_policy(&mut c12, Some(LengthPolicy::AspectRatio(1.0)));
	app.tree.set_node_spot(&mut c12, Some((Point::zero(), Size::zero())));

	add_spacer(&mut app.tree, &mut c1, LengthPolicy::Available(0.5));

	add_spacer(&mut app.tree, &mut p, LengthPolicy::Fixed(60));

	let mut c2 = app.tree.add_node(Some(&mut p), 3);
	app.tree.set_node_container(&mut c2, Some(Axis::Horizontal));
	app.tree.set_node_policy(&mut c2, Some(LengthPolicy::WrapContent(0, 10000)));
	app.tree.set_node_spot(&mut c2, Some((Point::zero(), Size::zero())));

	add_spacer(&mut app.tree, &mut c2, LengthPolicy::Fixed(100));

	let mut c2mid = app.tree.add_node(Some(&mut c2), 3);
	app.tree.set_node_container(&mut c2mid, Some(Axis::Vertical));
	app.tree.set_node_policy(&mut c2mid, Some(LengthPolicy::Available(1.0)));
	app.tree.set_node_spot(&mut c2mid, Some((Point::zero(), Size::zero())));

	let line_height = 40;

	let paragraph = Paragraph {
		parts: vec![((0, 0, 0, 0, 0, 0), String::from(TEXT))],
		font: font.clone(),
		up_to_date: false,
	};

	let mut line = app.tree.add_node(Some(&mut c2mid), 10);
	app.tree.set_node_policy(&mut line, Some(LengthPolicy::Chunks(line_height)));
	app.tree.set_node_spot(&mut line, Some((Point::zero(), Size::zero())));
	app.tree.set_node_container(&mut line, Some(Axis::Horizontal));

	app.tree.set_node_widget(&mut line, Some(rc_widget(paragraph)));

	add_spacer(&mut app.tree, &mut c2, LengthPolicy::Fixed(100));

	// _debug(&t, p, 0);

	flexbox::compute_tree(&mut app.tree, p);

	app.render(p);

	let timer = Instant::now();
	let runs = 100;
	for _ in 0..runs {
		app.render(p);
	}
	let elapsed = timer.elapsed().as_secs_f64();
	let avg_fps = ((runs as f64) / elapsed) as usize;
	println!("rendered {} frames in {}s ({} fps)", runs, elapsed as usize, avg_fps);

	for pixel in app.output.pixels.chunks_mut(RGBA) {
		if pixel[3] == 0 {
			pixel[..3].fill(0);
			pixel[3] = 255;
		}
	}

	save_png(&app.output);

	println!("Tree uses {}B", app.tree.memory_usage());

	// app.tree.show(p);
}

use std::path::Path;
use std::fs::File;
use std::io::BufWriter;

fn read_png(path: &str) -> Bitmap {
	let decoder = png::Decoder::new(File::open(path).unwrap());
	let mut reader = decoder.read_info().unwrap();
	let mut buf = vec![0; reader.output_buffer_size()];
	let info = reader.next_frame(&mut buf).unwrap();
	let len = (info.width * info.height) as usize;
	let pixels = match info.color_type {
		png::ColorType::Rgb => {
			let mut pixels = Vec::with_capacity(len * 4);
			for i in 0..len {
				let j = i * 3;
				pixels.push(buf[j + 0]);
				pixels.push(buf[j + 1]);
				pixels.push(buf[j + 2]);
				pixels.push(u8::MAX);
			}
			pixels
		},
		png::ColorType::Rgba => buf,
		_ => panic!("unsupported img"),
	};
	Bitmap {
		size: Size::new(info.width as usize, info.height as usize),
		channels: RGBA,
		pixels,
	}
}

fn save_png(img: &Bitmap) {
	let path = Path::new(r"output.png");
	let file = File::create(path).unwrap();
	let ref mut w = BufWriter::new(file);
	let mut encoder = png::Encoder::new(w, img.size.w as u32, img.size.h as u32); // Width is 2 pixels and height is 1.
	encoder.set_color(png::ColorType::Rgba);
	encoder.set_depth(png::BitDepth::Eight);
	let mut writer = encoder.write_header().unwrap();
	writer.write_image_data(&img.pixels).unwrap();
}
