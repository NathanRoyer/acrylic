use std::collections::HashMap;

use crate::app::Application;
use crate::node::LengthPolicy;
use crate::node::Axis;
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
	container(app, Axis::Vertical, path, attributes)
}

/// tag parser for an horizontal container.
pub fn h_container(app: &mut Application, path: &mut NodePath, attributes: &[Attribute]) -> Result<(), String> {
	container(app, Axis::Horizontal, path, attributes)
}

fn container(app: &mut Application, axis: Axis, path: &mut NodePath, attributes: &[Attribute]) -> Result<(), String> {
	let mut policy = Err(String::from("missing policy attribute"));
	let mut gap = 0;

	for Attribute { name, value } in attributes {
		match name.as_str() {
			"gap"           => gap = value.parse().map_err(|_| format!("bad value: {}", value))?,
			"pol:fixed"     => policy = Ok(LengthPolicy::Fixed      (value.parse().map_err(|_| format!("bad value: {}", value))?)),
			"pol:available" => policy = Ok(LengthPolicy::Available  (value.parse().map_err(|_| format!("bad value: {}", value))?)),
			"pol:chunks"    => policy = Ok(LengthPolicy::Chunks     (value.parse().map_err(|_| format!("bad value: {}", value))?)),
			"pol:ratio"     => policy = Ok(LengthPolicy::AspectRatio(value.parse().map_err(|_| format!("bad value: {}", value))?)),
			"pol:wrap" => {
				let (min, max) = value.split_once('-').ok_or(format!("bad value: {}", value))?;
				let min = min.parse().map_err(|_| format!("bad value: {}", value))?;
				let max = max.parse().map_err(|_| format!("bad value: {}", value))?;
				policy = Ok(LengthPolicy::WrapContent(min, max));
			},
			_ => Err(format!("unexpected attribute: {}", name))?,
		}
	}

	path.push(app.add_node(path, rc_node(Container {
		children: Vec::new(),
		policy: policy?,
		spot: (Point::zero(), Size::zero()),
		axis,
		gap,
	}))?);

	Ok(())
}
