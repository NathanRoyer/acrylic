use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;
use std::time::Instant;

use acrylic::flexbox;
use acrylic::node::NodeKey;
use acrylic::tree::Tree;
use acrylic::node::Hash;
use acrylic::node::Axis;
use acrylic::node::LengthPolicy;
use acrylic::node::PixelSource;
use acrylic::Point;
use acrylic::Size;

use acrylic::render::Bitmap;
use acrylic::render::Margin;
use acrylic::render::RGBA;
use acrylic::render::Context as RenderCtx;

use ab_glyph::FontRef;
use ab_glyph::PxScaleFont;
use ab_glyph::ScaleFont;
use ab_glyph::Font;

const TEXT: &'static str = "Mais, vous savez, moi je ne crois pas qu'il y ait de bonne ou de mauvaise situation. Moi, si je devais résumer ma vie aujourd'hui avec vous, je dirais que c'est d'abord des rencontres, Des gens qui m'ont tendu la main, peut-être à un moment où je ne pouvais pas, où j'étais seul chez moi. Et c'est assez curieux de se dire que les hasards, les rencontres forgent une destinée... Parce que quand on a le goût de la chose, quand on a le goût de la chose bien faite, Le beau geste, parfois on ne trouve pas l'interlocuteur en face, je dirais, le miroir qui vous aide à avancer. Alors ce n'est pas mon cas, comme je le disais là, puisque moi au contraire, j'ai pu ; Et je dis merci à la vie, je lui dis merci, je chante la vie, je danse la vie... Je ne suis qu'amour! Et finalement, quand beaucoup de gens aujourd'hui me disent : \"Mais comment fais-tu pour avoir cette humanité ?\", Eh bien je leur réponds très simplement, je leur dis que c'est ce goût de l'amour, Ce goût donc qui m'a poussé aujourd'hui à entreprendre une construction mécanique, Mais demain, qui sait, peut-être simplement à me mettre au service de la communauté, à faire le don, le don de soi...";

fn _hash(s: &str) -> Hash {
	let mut h = DefaultHasher::new();
	h.write(s.as_bytes());
	h.finish()
}

fn add_spacer(t: &mut Tree, p: &mut NodeKey, policy: LengthPolicy) {
	let mut c11 = t.new_child(Some(p), 3);
	t.set_node_policy(&mut c11, Some(policy));
	t.set_node_position(&mut c11, Some(Point::new(0, 0)));
	t.set_node_size(&mut c11, Some(Size::new(0, 0)));
}

fn char_bmp(ctx: &mut RenderCtx, font: &PxScaleFont<&FontRef>, c: char) -> Option<(PixelSource, f64)> {
	let key = (0, c as usize);
	let ratio = if let Some(bmp) = ctx.bmp_store.get(&key) {
		let size = bmp.size();
		(size.h as f64) / (size.w as f64)
	} else {
		if let Some(q) = font.outline_glyph(font.scaled_glyph(c)) {
			let r1 = q.px_bounds();
			let r2 = font.glyph_bounds(q.glyph());
			let top = (r1.min.y - r2.min.y) as isize;
			let left = (r1.min.x - r2.min.x) as isize;
			let box_w = r2.width().ceil() as isize;
			let box_h = r2.height().ceil() as isize;
			let glyph_w = r1.width().ceil() as isize;
			let glyph_h = r1.height().ceil() as isize;

			let bmpsz = Size::new(glyph_w as usize, glyph_h as usize);
			let mut bmp = Bitmap::new(bmpsz, RGBA);
			bmp.margin = Margin {
				top,
				left,
				right: box_w - (left + glyph_w),
				bottom: box_h - (top + glyph_h),
			};

			q.draw(|x, y, c| {
				let (x, y) = (x as usize, y as usize);
				let i = (y * bmpsz.w + x) * RGBA;
				let a = (255.0 * c) as u8;
				if let Some(slice) = bmp.pixels.get_mut(i..(i + RGBA)) {
					slice.copy_from_slice(&[255, 255, 255, a]);
				}
			});
			ctx.bmp_store.insert(key, bmp);
			(box_h as f64) / (box_w as f64)
		} else {
			let id = font.glyph_id(c);
			let h = font.height().ceil() as usize;
			let w = font.h_advance(id);
			(h as f64) / (w as f64)
		}
	};
	Some((PixelSource::Bitmap(key.0, key.1), ratio))
}

fn _debug(t: &Tree, k: NodeKey, d: usize) -> Option<()> {
	let position = t.get_node_position(k)?;
	let size = t.get_node_size(k)?;
	println!("{}{}: {}x{} at {}x{}", "\t".repeat(d), k, size.w, size.h, position.x, position.y);
	for i in t.children(k) {
		_debug(t, i, d + 1);
	}
	None
}

fn main() {
	let mut t = Tree::new();

	let mut p = t.new_child(None, 10);
	t.set_node_container(&mut p, Some(Axis::Vertical));
	t.set_node_position(&mut p, Some(Point::new(0, 0)));
	t.set_node_size(&mut p, Some(Size::new(600, 800)));

	add_spacer(&mut t, &mut p, LengthPolicy::Fixed(30));

	let mut c1 = t.new_child(Some(&mut p), 3);
	t.set_node_container(&mut c1, Some(Axis::Horizontal));
	t.set_node_policy(&mut c1, Some(LengthPolicy::AspectRatio(0.33)));
	t.set_node_position(&mut c1, Some(Point::new(0, 0)));
	t.set_node_size(&mut c1, Some(Size::new(0, 0)));

	add_spacer(&mut t, &mut c1, LengthPolicy::Available(0.5));

	let mut c12 = t.new_child(Some(&mut c1), 3);
	t.set_node_pixel_source(&mut c12, Some(PixelSource::Bitmap(0, 0)));
	t.set_node_policy(&mut c12, Some(LengthPolicy::AspectRatio(1.0)));
	t.set_node_position(&mut c12, Some(Point::new(0, 0)));
	t.set_node_size(&mut c12, Some(Size::new(0, 0)));

	add_spacer(&mut t, &mut c1, LengthPolicy::Available(0.5));

	add_spacer(&mut t, &mut p, LengthPolicy::Fixed(30));

	// let mut c2 = t.new_child(Some(&mut p), 3);
	// t.set_node_container(&mut c2, Some(Axis::Horizontal));
	// t.set_node_policy(&mut c2, Some(LengthPolicy::Fixed(100)));
	// t.set_node_position(&mut c2, Some(Point::new(0, 0)));
	// t.set_node_size(&mut c2, Some(Size::new(0, 0)));
// 
	// add_spacer(&mut t, &mut c2, LengthPolicy::Available(0.5));

	let line_height = 20;

	let mut line = t.new_child(Some(&mut p), 10);
	t.set_node_policy(&mut line, Some(LengthPolicy::Chunks(line_height)));
	t.set_node_position(&mut line, Some(Point::new(0, 0)));
	t.set_node_size(&mut line, Some(Size::new(0, 0)));
	t.set_node_container(&mut line, Some(Axis::Horizontal));

	// add_spacer(&mut t, &mut c2, LengthPolicy::Available(0.5));

	let mut rdr_ctx = RenderCtx::new();
	rdr_ctx.bmp_store.insert((0, 0), read_png("rsc/castle-in-the-sky.png"));

	let font = FontRef::try_from_slice(include_bytes!("../rsc/font.ttf")).unwrap();
	let font = font.as_scaled(line_height as f32);

	for ch in TEXT.chars() {
		if let Some((key, ratio)) = char_bmp(&mut rdr_ctx, &font, ch) {
			let mut c = t.new_child(Some(&mut line), 3);
			t.set_node_pixel_source(&mut c, Some(key));
			t.set_node_policy(&mut c, Some(LengthPolicy::AspectRatio(ratio)));
			t.set_node_position(&mut c, Some(Point::new(0, 0)));
			t.set_node_size(&mut c, Some(Size::new(0, 0)));
		}
	}

	// _debug(&t, p, 0);

	flexbox::compute_tree(&mut t, p);

	rdr_ctx.render(&t, p);

	let timer = Instant::now();
	let runs = 100;
	for _ in 0..runs {
		rdr_ctx.render(&t, p);
	}
	let elapsed = timer.elapsed().as_secs_f64();
	let avg_fps = ((runs as f64) / elapsed) as usize;
	println!("rendered {} frames in {}s ({} fps)", runs, elapsed as usize, avg_fps);

	save_png(rdr_ctx.get_output());

	println!("Tree uses {}B", t.memory_usage());
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
		margin: Margin::zero(),
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
