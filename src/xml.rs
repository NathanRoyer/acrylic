use std::collections::HashMap;

use crate::app::Application;
use crate::node::LengthPolicy;
use crate::node::Axis;
use crate::node::Margin;
use crate::node::NodePath;
use crate::node::rc_node;
use crate::node::Container;
use crate::Point;
use crate::Size;
use crate::format;

use xmlparser::ElementEnd;
use xmlparser::Tokenizer;
use xmlparser::StrSpan;
use xmlparser::Token;

use std::string::String;
use std::vec::Vec;
use std::println;
#[cfg(not(feature = "std"))]
use std::print;

/// An XML Attribute
#[derive(Debug, Clone)]
pub struct Attribute {
	/// If a namespace was specified in the xml file,
	/// `name` will contain both namespace and local
	/// name as in: `"[namespace]:[local name]"`.
	pub name: String,
	pub value: String,
}

pub fn unexpected_attr(attr: &str) -> Result<(), String> {
	let mut errmsg = String::from("unexpected attribute: ");
	errmsg += attr;
	Err(errmsg)
}

type Handler = &'static dyn Fn(&mut Application, &mut NodePath, &[Attribute]) -> Result<(), String>;

/// This structure is used to parse an xml file
/// representing a view of an application.
#[derive(Clone)]
pub struct TreeParser {
	handlers: HashMap<String, Handler>,
}

fn concat(prefix: StrSpan, local: StrSpan) -> String {
	match prefix.as_str().len() {
		0 => String::from(local.as_str()),
		_ => format!("{}:{}", &prefix, &local),
	}
}

impl TreeParser {
	pub fn new() -> Self {
		Self {
			handlers: HashMap::new(),
		}
	}

	pub fn with_builtin_tags(&mut self) -> &mut Self {
		#[cfg(feature = "text")]
		self.with("p", &crate::text::paragraph);
		#[cfg(feature = "png")]
		self.with("png", &crate::png::xml_handler);
		#[cfg(feature = "railway")]
		self.with("rwy", &crate::railway::xml_handler);
		self.with("x", &h_container)
			.with("y", &v_container)
			.with("inflate", &spacer)
	}

	/// Add a tag handler to the parser
	pub fn with(&mut self, tag: &str, handler: Handler) -> &mut Self {
		self.handlers.insert(String::from(tag), handler);
		self
	}

	/// Try to parse the xml
	pub fn parse(&self, app: &mut Application, path: &mut NodePath, xml: &str) -> Result<(), String> {
		let mut names = Vec::new();
		let mut attributes = Vec::new();
		for token in Tokenizer::from(xml) {
			match token.map_err(|e| format!("{:?}", e))? {
				Token::ElementStart { prefix, local, .. } => {
					names.push(concat(prefix, local));
				},
				Token::Attribute { prefix, local, value, .. } => {
					attributes.push(Attribute {
						name: concat(prefix, local),
						value: String::from(value.as_str()),
					});
				},
				Token::ElementEnd { end, span } => {
					let addr = span.start();
					match end {
						ElementEnd::Close(prefix, local) => {
							if names.pop() != Some(concat(prefix, local)) {
								Err(format!("unexpected close tag [@{}]", addr))?;
							}
							path.pop().unwrap();
						},
						_ => {
							let name = names.last().unwrap();
							// handler is meant to push to path
							match self.handlers.get(name) {
								Some(handler) => handler(app, path, &attributes)?,
								None => Err(format!("unknown element: {} [@{}]", name, addr))?,
							};
							attributes.clear();
						},
					}
					if let ElementEnd::Empty = end {
						let _ = names.pop();
						path.pop().unwrap();
					}
				}
				_ => println!("xml: ignoring {:?}", token.unwrap()),
			}
		}
		Ok(())
	}
}

/// tag parser for a vertical container.
pub fn v_container(app: &mut Application, path: &mut NodePath, attributes: &[Attribute]) -> Result<(), String> {
	container(app, Some(Axis::Vertical), path, attributes)
}

/// tag parser for an horizontal container.
pub fn h_container(app: &mut Application, path: &mut NodePath, attributes: &[Attribute]) -> Result<(), String> {
	container(app, Some(Axis::Horizontal), path, attributes)
}

pub fn spacer(app: &mut Application, path: &mut NodePath, attributes: &[Attribute]) -> Result<(), String> {
	for Attribute { name, .. } in attributes {
		Err(format!("unexpected attribute: {}", name))?;
	}

	app.add_node(path, rc_node(Container {
		children: Vec::new(),
		policy: LengthPolicy::Remaining(1.0),
		spot: (Point::zero(), Size::zero()),
		margin: None,
		axis: None,
		gap: 0,
	}))?;

	Ok(())
}

fn container(app: &mut Application, axis: Option<Axis>, path: &mut NodePath, attributes: &[Attribute]) -> Result<(), String> {
	let mut policy = Err(String::from("missing policy attribute"));
	let mut margin = None;
	let mut gap = 0;

	for Attribute { name, value } in attributes {
		match name.as_str() {
			"margin"        => {
				let m = value.parse().map_err(|_| format!("bad value: {}", value))?;
				margin = Some(Margin {
					left: m,
					top: m,
					bottom: m,
					right: m,
				});
			},
			"gap"         => gap = value.parse().map_err(|_| format!("bad value: {}", value))?,
			"fixed"   => policy = Ok(LengthPolicy::Fixed      (value.parse().map_err(|_| format!("bad value: {}", value))?)),
			"rem"     => policy = Ok(LengthPolicy::Remaining  (value.parse().map_err(|_| format!("bad value: {}", value))?)),
			"chunks"  => policy = Ok(LengthPolicy::Chunks     (value.parse().map_err(|_| format!("bad value: {}", value))?)),
			"ratio"   => policy = Ok(LengthPolicy::AspectRatio(value.parse().map_err(|_| format!("bad value: {}", value))?)),
			"wrap"    => policy = Ok(LengthPolicy::WrapContent),
			_ => Err(format!("unexpected attribute: {}", name))?,
		}
	}

	app.add_node(path, rc_node(Container {
		children: Vec::new(),
		policy: policy?,
		spot: (Point::zero(), Size::zero()),
		margin,
		axis,
		gap,
	}))?;

	Ok(())
}
