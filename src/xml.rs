use std::collections::HashMap;

use crate::application::Application;
use crate::tree::LengthPolicy;
use crate::tree::NodeKey;
use crate::tree::Axis;
use crate::Point;
use crate::Size;

use xmlparser::ElementEnd;
use xmlparser::Tokenizer;
use xmlparser::StrSpan;
use xmlparser::Token;

/// An XML Attribute
#[derive(Debug, Clone)]
pub struct Attribute {
	/// If a namespace was specified in the xml file,
	/// `name` will contain both namespace and local
	/// name as in: `"[namespace]:[local name]"`.
	pub name: String,
	pub value: String,
}

type Handler = &'static dyn Fn(&mut Application, Option<&mut NodeKey>, &[Attribute]) -> Result<NodeKey, String>;

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
	pub fn parse(&self, app: &mut Application, xml: &str) -> Result<NodeKey, String> {
		let mut root = Err(String::from("Empty XML"));
		let mut parents = Vec::new();
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
							let this = parents.pop().unwrap();
							if parents.len() == 0 {
								root = Ok(this);
							}
						},
						_ => {
							let name = names.last().unwrap();
							let new_el = match self.handlers.get(name) {
								Some(handler) => handler(app, parents.last_mut(), &attributes)?,
								None => Err(format!("unknown element: {} [@{}]", name, addr))?,
							};
							parents.push(new_el);
							attributes.clear();
						},
					}
					if let ElementEnd::Empty = end {
						let _ = names.pop();
						let this = parents.pop().unwrap();
						if parents.len() == 0 {
							root = Ok(this);
						}
					}
				}
				_ => println!("xml: ignoring {:?}", token.unwrap()),
			}
		}
		root
	}
}

/// tag parser for a vertical container.
pub fn v_container(app: &mut Application, parent: Option<&mut NodeKey>, attributes: &[Attribute]) -> Result<NodeKey, String> {
	container(app, Axis::Vertical, parent, attributes)
}

/// tag parser for an horizontal container.
pub fn h_container(app: &mut Application, parent: Option<&mut NodeKey>, attributes: &[Attribute]) -> Result<NodeKey, String> {
	container(app, Axis::Horizontal, parent, attributes)
}

fn container(app: &mut Application, axis: Axis, parent: Option<&mut NodeKey>, attributes: &[Attribute]) -> Result<NodeKey, String> {
	let mut policy = Err(String::from("missing policy attribute"));

	for Attribute { name, value } in attributes {
		match name.as_str() {
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

	let mut node = app.tree.add_node(parent, 3);
	app.tree.set_node_container(&mut node, Some(axis));
	app.tree.set_node_policy(&mut node, Some(policy?));
	app.tree.set_node_spot(&mut node, Some((Point::zero(), Size::zero())));

	Ok(node)
}
