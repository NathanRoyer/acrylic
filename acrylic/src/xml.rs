//! Attribute, Spacer, TreeParser, ViewLoader, import, Handler...

use crate::app::Application;
use crate::app::DataRequest;
use crate::style::style_index;
use crate::node::node_box;
use crate::node::Axis;
use crate::node::LengthPolicy;
use crate::node::RenderReason;
use crate::node::Node;
use crate::node::NodePathSlice;
use crate::node::NodeBox;
use crate::container::Container;
use crate::Size;
use crate::Status;

use xmlparser::ElementEnd;
use xmlparser::StrSpan;
use xmlparser::Token;
use xmlparser::Tokenizer;

use log::error;

use core::any::Any;
use core::mem::replace;
use core::fmt::Debug;
use core::fmt::Formatter;
use core::fmt::Result as FmtResult;
use core::mem::swap;

use hashbrown::hash_map::HashMap;
use alloc::string::String;
use alloc::boxed::Box;
use alloc::vec::Vec;

/// An XML Attribute
///
/// Note: During parsing, prefixes (namespaces) are
/// stripped from attributes before these structures
/// are created.
#[derive(Debug, Clone)]
pub struct Attribute {
    pub name: String,
    pub value: String,
}

/// Utility function to handle unexpected attributes.
pub fn unexpected_attr(line: usize, tag: &str, attr: &str) -> Result<(), ()> {
    Err(error!("line {}: <{}> unexpected attribute: {}", line, tag, attr))
}

/// Utility function to handle missing attributes.
pub fn check_attr<T>(line: usize, tag: &str, attr: &str, value: Option<T>) -> Result<T, ()> {
    value.ok_or_else(|| error!("line {}: <{}> missing attribute: {}", line, tag, attr))
}

/// Utility function to handle invalid attribute values.
pub fn invalid_attr_val(line: usize, tag: &str, attr: &str, value: &str) -> () {
    error!("line {}: <{}> invalid attribute value: {}={:?}", line, tag, attr, value);
}

/// Handle to a node-creating tag handler.
pub type Handler = Box<dyn Fn(&mut TreeParser, usize, Vec<Attribute>) -> Result<Option<NodeBox>, ()>>;

/// This structure is used to parse an xml file
/// representing a view of an application.
pub struct TreeParser {
    handlers: HashMap<String, Handler>,
    parameters: HashMap<String, String>,
}

impl Debug for TreeParser {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("TreeParser")
            .field("parameters", &self.parameters)
            .field("tags", &self.handlers.keys().collect::<Vec<&String>>())
            .finish()
    }
}

fn span_line(xml: &str, span: StrSpan) -> usize {
    1 + xml[..span.start()].match_indices("\n").count()
}

fn err(msg: &str, arg: &str, xml: &str, span: StrSpan) -> Result<NodeBox, ()> {
    let line = span_line(xml, span);
    match arg.len() {
        0 => error!("[xml] {} (near line {})", msg, line),
        _ => error!("[xml] {}: {} (near line {})", msg, arg, line),
    };
    Err(())
}

impl TreeParser {
    /// Create a parser for an xml view.
    ///
    /// `params` is used when tags of this view
    /// reference a parameter in their attribute.
    /// This allows for simple templating.
    pub fn new(params: Vec<Attribute>) -> Self {
        let mut parameters = HashMap::new();
        for Attribute { name, value } in params {
            parameters.insert(name, value);
        }
        Self {
            handlers: HashMap::new(),
            parameters,
        }
    }

    /// Add all tags built in this toolkit to the parser.
    ///
    /// This includes:
    /// * `p` → [`xml_paragraph`](`crate::text::xml_paragraph`)
    /// * `png` → [`xml_load_png`](`crate::png::xml_load_png`)
    /// * `rwy` → [`xml_load_railway`](`crate::railway::xml_load_railway`)
    /// * `x` → [`h_container`]
    /// * `y` → [`v_container`]
    /// * `import` → [`import`]
    /// * `inflate` → [`spacer`]
    ///
    /// See their documentation for a list of respective attributes.
    pub fn with_builtin_tags(&mut self) -> &mut Self {
        #[cfg(feature = "text")]
        self.with("p", Box::new(crate::text::xml_paragraph));
        #[cfg(feature = "png")]
        self.with("png", Box::new(crate::png::xml_load_png));
        #[cfg(feature = "railway")]
        self.with("rwy", Box::new(crate::railway::xml_load_railway));
        self.with("x", Box::new(h_container))
            .with("y", Box::new(v_container))
            .with("import", Box::new(import))
            .with("inflate", Box::new(spacer))
    }

    /// Add a tag handler to the parser
    pub fn with(&mut self, tag: &str, handler: Handler) -> &mut Self {
        self.handlers.insert(String::from(tag), handler);
        self
    }

    /// Try to parse the xml
    pub fn parse(&mut self, xml: &str) -> Result<NodeBox, ()> {
        let mut attributes = Vec::new();
        let mut stack = Vec::new();
        let mut tree: Vec<Option<NodeBox>> = Vec::new();
        let mut root = None;
        for token in Tokenizer::from(xml) {
            let token = token.map_err(|e| error!("{:?}", e))?;
            match token {
                Token::ElementStart {
                    prefix,
                    local,
                    span,
                } => {
                    if prefix.len() > 0 {
                        return err("unexpected prefix", &prefix, xml, span);
                    }
                    let name = String::from(local.as_str());
                    if let None = self.handlers.get(&name) {
                        return err("unknown tag", &local, xml, span);
                    }
                    stack.push(name);
                }
                Token::Attribute {
                    prefix,
                    local,
                    value,
                    span,
                } => {
                    let value = String::from(value.as_str());
                    let value = match prefix.as_str() {
                        "" => Some(value),
                        "param" => self.parameters.get(&value).map(|s| s.clone()),
                        _ => return err("unexpected prefix", &prefix, xml, span),
                    };
                    if let Some(value) = value {
                        attributes.push(Attribute {
                            name: String::from(local.as_str()),
                            value,
                        });
                    }
                }
                Token::ElementEnd { end, span } => {
                    let mut pop = false;
                    match end {
                        ElementEnd::Close(prefix, local) => {
                            if prefix.len() > 0 {
                                return err("unexpected prefix", &prefix, xml, span);
                            }
                            let str_local = String::from(local.as_str());
                            let mut expected = None;
                            if let Some(name) = stack.pop() {
                                expected = Some(name);
                            }
                            if Some(str_local) != expected {
                                return err("unexpected close tag", &local, xml, span);
                            }
                            pop = true;
                        }
                        _ => {
                            let name = match stack.last() {
                                Some(name) => name,
                                None => return err("unexpected tag end", "", xml, span),
                            };
                            let handler = self.handlers.remove(name).unwrap();

                            let attributes = replace(&mut attributes, Vec::new());
                            let line = span_line(xml, span);
                            let node = handler(self, line, attributes)?;

                            self.handlers.insert(name.clone(), handler);
                            tree.push(node);
                            if let ElementEnd::Empty = end {
                                pop = true;
                                stack.pop().unwrap();
                            }
                        }
                    }
                    if pop {
                        if let Some(node) = tree.pop().unwrap() {
                            if let Some(parent) = tree.last_mut() {
                                if let Some(parent) = parent {
                                    parent.add_node(node)?;
                                } else {
                                    return err("parent is not a container", "", xml, span);
                                }
                            } else {
                                root = Some(node);
                            }
                        } // else error maybe?
                    }
                }
                _ => (/* do nothing */),
            }
        }
        match root {
            Some(root) => Ok(root),
            None => Err(error!("[xml] empty view file?")),
        }
    }
}

/// [`Node`] implementor which makes a request to
/// the contained source, parses the response then
/// replaces itself with the parsed node.
#[derive(Debug, Clone)]
pub struct ViewLoader {
    pub source: String,
    pub parameters: Vec<Attribute>,
}

impl ViewLoader {
    /// Create a new [`ViewLoader`] with no parameters
    pub fn new(source: &str) -> Self {
        Self {
            source: String::from(source),
            parameters: Vec::new(),
        }
    }
}

impl Node for ViewLoader {
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }

    fn please_clone(&self) -> NodeBox {
        node_box(self.clone())
    }

    fn describe(&self) -> String {
        String::from("Loading Template image...")
    }

    fn initialize(&mut self, app: &mut Application, path: NodePathSlice) -> Result<(), ()> {
        app.data_requests.push(DataRequest {
            node: path.to_vec(),
            name: self.source.clone(),
            range: None,
        });
        Ok(())
    }

    #[allow(unused)]
    fn loaded(
        &mut self,
        app: &mut Application,
        path: NodePathSlice,
        _: &str,
        _: usize,
        data: &[u8],
    ) -> Status {
        let xml = String::from_utf8(data.to_vec());

        let mut parameters = Vec::new();
        swap(&mut self.parameters, &mut parameters);

        let mut parser = TreeParser::new(parameters);
        parser.with_builtin_tags();

        let result = match xml {
            Ok(xml) => match parser.parse(&xml) {
                Ok(node) => Ok(app.replace_kidnapped(path, node)),
                Err(()) => Err("Error during XML parsing"),
            },
            Err(_) => Err("Could not parse xml as UTF8 text"),
        };

        if let Err(msg) = result {
            error!("TemplateLoader: {}", msg);
        }

        Ok(())
    }
}

/// XML tag for template import.
///
/// Pass this to [`TreeParser::with`].
///
/// For example, if `templates/fake-button.xml` contains this:
/// ```xml
/// <p margin="10" param:txt="button-text" />
/// ```
///
/// You would import the template like so:
///
/// ```xml
/// <import tag="fake-button" src="templates/fake-button.xml" />
/// ...
/// <fake-button button-text="can't click me!" />
/// ```
///
/// Notice how the template mapped the `button-text` parameter
/// to the `txt` attribute.
///
/// The `tag` attribute is mandatory and will be a valid tag name after this line.
///
/// The `src` attribute is mandatory and must point to an xml view.
pub fn import(parser: &mut TreeParser, line: usize, attributes: Vec<Attribute>) -> Result<Option<NodeBox>, ()> {
    const TN: &'static str = "import";

    let mut tag = None;
    let mut source = None;

    for Attribute { name, value } in attributes {
        match name.as_str() {
            "tag" => tag = Some(value),
            "src" => source = Some(value),
            _ => unexpected_attr(line, TN, &name)?,
        }
    }

    let tag = check_attr(line, TN, "tag", tag)?;
    let source = check_attr(line, TN, "src", source)?;

    parser.with(
        &tag,
        Box::new(move |_tree_parser, _line, parameters| {
            Ok(Some(node_box(ViewLoader {
                source: source.clone(),
                parameters: parameters.to_vec(),
            })))
        }),
    );

    Ok(None)
}

/// An invisible [`Node`] implementor which
/// a length policy of Remaining(1.0), making
/// it take available space.
#[derive(Debug, Clone)]
pub struct Spacer {
    pub spot_size: Size,
}

impl Node for Spacer {
    fn please_clone(&self) -> NodeBox {
        node_box(self.clone())
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }

    fn describe(&self) -> String {
        String::from("Spacer")
    }

    fn policy(&self) -> LengthPolicy {
        LengthPolicy::Remaining(1.0)
    }

    fn get_spot_size(&self) -> Size {
        self.spot_size
    }

    fn set_spot_size(&mut self, size: Size) {
        self.spot_size = size;
    }
}

/// XML tag for vertical containers.
///
/// Pass this to [`TreeParser::with`].
///
/// Results in a [`Container`] node.
///
/// ```xml
/// <x rem="1" style="0" gap="10" margin="10" radius="10" onclick="my_handler">
///     ...
/// </x>
/// ```
///
/// One of these attributes must be present:
/// * `fixed="N"` → maps to [`LengthPolicy::Fixed`]
/// * `  rem="N"` → maps to [`LengthPolicy::Remaining`]
/// * `hunks="N"` → maps to [`LengthPolicy::Chunks`]
/// * `ratio="N"` → maps to [`LengthPolicy::AspectRatio`]
/// * ` wrap="" ` → maps to [`LengthPolicy::WrapContent`]
///
/// The `style` attribute is optional and references a style.
/// Note: This is in early state of development, it is not defined
/// how much is the maximum for this attribute.
///
/// The `focus` attribute is optional and references a style
/// which is only applied when the node is focused.
/// Note: This is in early state of development, it is not defined
/// how much is the maximum for this attribute.
///
/// The `gap` attribute is optional and defines the space
/// between consecutive children of this container.
///
/// The `margin` attribute is optional and specifies an empty
/// space around the content.
///
/// The `radius` attribute is optional and specify that the
/// container should have round corners of such a radius.
///
/// The `on_click` attribute is optional and specifies an
/// event handler to call when the node receives an
/// [`Event::QuickAction1`](`crate::node::Event::QuickAction1`).
/// See [`Application::add_handler`] to set event handlers up.
///
pub fn v_container(_: &mut TreeParser, line: usize, attributes: Vec<Attribute>) -> Result<Option<NodeBox>, ()> {
    container(Axis::Vertical, line, attributes)
}

/// XML tag for horizontal containers.
///
/// Pass this to [`TreeParser::with`].
///
/// Results in a [`Container`] node.
///
/// ```xml
/// <y rem="1" style="0" gap="10" margin="10" radius="10" onclick="my_handler">
///     ...
/// </y>
/// ```
///
/// One of these attributes must be present:
/// * `fixed="N"` → maps to [`LengthPolicy::Fixed`]
/// * `  rem="N"` → maps to [`LengthPolicy::Remaining`]
/// * `hunks="N"` → maps to [`LengthPolicy::Chunks`]
/// * `ratio="N"` → maps to [`LengthPolicy::AspectRatio`]
/// * ` wrap="" ` → maps to [`LengthPolicy::WrapContent`]
///
/// The `style` attribute is optional and references a style.
/// Note: This is in early state of development, it is not defined
/// how much is the maximum for this attribute.
///
/// The `focus` attribute is optional and references a style
/// which is only applied when the node is focused.
/// Note: This is in early state of development, it is not defined
/// how much is the maximum for this attribute.
///
/// The `gap` attribute is optional and defines the space
/// between consecutive children of this container.
///
/// The `margin` attribute is optional and specifies an empty
/// space around the content.
///
/// The `radius` attribute is optional and specifies that the
/// container should have round corners of such a radius.
///
/// The `on-click` attribute is optional and specifies an
/// event handler to call when the node receives an
/// [`Event::QuickAction1`](`crate::node::Event::QuickAction1`).
/// See [`Application::add_handler`] to set event handlers up.
///
pub fn h_container(_: &mut TreeParser, line: usize, attributes: Vec<Attribute>) -> Result<Option<NodeBox>, ()> {
    container(Axis::Horizontal, line, attributes)
}

/// XML tag for a spacer.
///
/// Pass this to [`TreeParser::with`].
///
/// Results in a [`Spacer`] node.
///
/// ```xml
/// <inflate />
/// ```
///
/// These tags allow no attributes.
///
pub fn spacer(_: &mut TreeParser, line: usize, attributes: Vec<Attribute>) -> Result<Option<NodeBox>, ()> {
    for Attribute { name, .. } in attributes {
        unexpected_attr(line, "inflate", &name)?;
    }

    Ok(Some(node_box(Spacer {
        spot_size: Size::zero(),
    })))
}

fn container(axis: Axis, line: usize, attributes: Vec<Attribute>) -> Result<Option<NodeBox>, ()> {
    const TN: &'static str = "x/y";
    let mut policy = None;
    let mut margin = None;
    let mut radius = None;
    let mut normal_style = None;
    let mut focus_style = None;
    let mut on_click = None;
    let mut gap = 0;

    let parse = |line, name: &str, value: &str| -> Result<f64, ()> {
        value
            .parse()
            .map_err(|_| invalid_attr_val(line, TN, name, value))
    };

    for Attribute { name, value } in attributes {
        match name.as_str() {
            "on-click" => on_click = Some(value),
            "margin" => margin = Some(parse(line, &name, &value)? as usize),
            "radius" => radius = Some(parse(line, &name, &value)? as usize),
            "gap" => gap = parse(line, &name, &value)? as usize,
            "fixed" => {
                policy = Some(LengthPolicy::Fixed(
                    parse(line, &name, &value)? as usize,
                ))
            },
            "rem" => {
                policy = Some(LengthPolicy::Remaining(
                    parse(line, &name, &value)?,
                ))
            },
            "chunks" => {
                policy = Some(LengthPolicy::Chunks(
                    parse(line, &name, &value)? as usize,
                ))
            },
            "ratio" => {
                policy = Some(LengthPolicy::AspectRatio(
                    parse(line, &name, &value)?,
                ))
            },
            "wrap" => policy = Some(LengthPolicy::WrapContent),
            "style" => {
                let s = style_index(&value).ok_or(());
                normal_style = Some(s.map_err(|_| invalid_attr_val(line, TN, &name, &value))?)
            },
            "focus" => {
                let s = style_index(&value).ok_or(());
                focus_style = Some(s.map_err(|_| invalid_attr_val(line, TN, &name, &value))?)
            },
            _ => unexpected_attr(line, TN, &name)?,
        }
    }

    let spot_size = Size::zero();
    let container = node_box(Container {
        children: Vec::new(),
        policy: check_attr(line, "x/y", "fixed/rem/chunks/wrap/ratio", policy)?,
        on_click,
        spot_size,
        margin,
        radius,
        axis,
        gap,
        normal_style,
        focus_style,
        focused: false,
        #[cfg(feature = "railway")]
        style_rwy: None,
        render_cache: [None, None],
        render_reason: RenderReason::Resized,
    });

    Ok(Some(container))
}
