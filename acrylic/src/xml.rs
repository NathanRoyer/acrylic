use crate::app::Application;
use crate::app::DataRequest;
use crate::format;
use crate::lock;
use crate::node::rc_node;
use crate::node::Axis;
use crate::node::Container;
use crate::node::LengthPolicy;
use crate::node::NeedsRepaint;
use crate::node::Node;
use crate::node::NodePath;
use crate::node::RcNode;
use crate::Point;
use crate::Size;
use crate::Spot;
use crate::Status;

use xmlparser::ElementEnd;
use xmlparser::StrSpan;
use xmlparser::Token;
use xmlparser::Tokenizer;

use core::any::Any;
use core::fmt::Debug;
use core::fmt::Formatter;
use core::fmt::Result as FmtResult;
use core::mem::swap;

use std::collections::HashMap;
use std::string::String;
use std::sync::Arc;
use std::sync::Mutex;
use std::vec::Vec;

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

/// Utility function to react to unexpected attributes.
pub fn unexpected_attr(attr: &str) -> Result<(), String> {
    let mut errmsg = String::from("unexpected attribute: ");
    errmsg += attr;
    Err(errmsg)
}

/// Handle to a node-creating tag handler.
pub type RcHandler =
    Arc<Mutex<dyn Fn(&mut TreeParser, &[Attribute]) -> Result<Option<RcNode>, String>>>;

/// Wraps a function in a [`RcHandler`]
pub fn rc_handler<
    H: 'static + Fn(&mut TreeParser, &[Attribute]) -> Result<Option<RcNode>, String>,
>(
    handler: H,
) -> RcHandler {
    Arc::new(Mutex::new(handler))
}

/// This structure is used to parse an xml file
/// representing a view of an application.
#[derive(Clone)]
pub struct TreeParser {
    handlers: HashMap<String, RcHandler>,
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

fn err(msg: &str, arg: &str, xml: &str, span: StrSpan) -> Result<RcNode, String> {
    let addr = span.start();
    let line = xml[..addr]
        .match_indices("\n")
        .collect::<Vec<(usize, &str)>>()
        .len();
    Err(match arg.len() {
        0 => format!("[xml] {} [L{}]", msg, line),
        _ => format!("[xml] {}: {} [L{}]", msg, arg, line),
    })
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
        self.with("p", rc_handler(crate::text::xml_paragraph));
        #[cfg(feature = "png")]
        self.with("png", rc_handler(crate::png::xml_load_png));
        #[cfg(feature = "railway")]
        self.with("rwy", rc_handler(crate::railway::xml_load_railway));
        self.with("x", rc_handler(h_container))
            .with("y", rc_handler(v_container))
            .with("import", rc_handler(import))
            .with("inflate", rc_handler(spacer))
    }

    /// Add a tag handler to the parser
    pub fn with(&mut self, tag: &str, handler: RcHandler) -> &mut Self {
        self.handlers.insert(String::from(tag), handler);
        self
    }

    /// Try to parse the xml
    pub fn parse(&mut self, xml: &str) -> Result<RcNode, String> {
        let mut attributes = Vec::new();
        let mut stack = Vec::new();
        let mut tree: Vec<Option<RcNode>> = Vec::new();
        let mut root = None;
        for token in Tokenizer::from(xml) {
            match token.map_err(|e| format!("{:?}", e))? {
                Token::ElementStart {
                    prefix,
                    local,
                    span,
                } => {
                    if prefix.len() > 0 {
                        return err("unexpected prefix", &prefix, xml, span);
                    }
                    let name = String::from(local.as_str());
                    let handler = match self.handlers.get(&name) {
                        Some(tuple) => tuple,
                        None => return err("unknown tag", &local, xml, span),
                    };
                    stack.push((name, handler.clone()));
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
                    match end {
                        ElementEnd::Close(prefix, local) => {
                            if prefix.len() > 0 {
                                return err("unexpected prefix", &prefix, xml, span);
                            }
                            let str_local = String::from(local.as_str());
                            let mut expected = None;
                            if let Some((name, _)) = stack.pop() {
                                expected = Some(name);
                            }
                            if Some(str_local) != expected {
                                return err("unexpected close tag", &local, xml, span);
                            }
                            root = tree.pop();
                        }
                        _ => {
                            let (_, handler) = match stack.last() {
                                Some(tuple) => tuple,
                                None => return err("unexpected tag end", "", xml, span),
                            };
                            let handler = lock(&handler).unwrap();
                            let node = match handler(self, &attributes) {
                                Ok(node) => node,
                                Err(msg) => return err(&msg, "", xml, span),
                            };
                            if let (Some(node), Some(parent)) = (&node, tree.last()) {
                                if let Some(parent) = parent {
                                    let mut parent = lock(&parent).unwrap();
                                    parent.add_node(node.clone())?;
                                } else {
                                    return err("parent is not a container", "", xml, span);
                                }
                            }
                            tree.push(node);
                            attributes.clear();
                        }
                    }
                    if let ElementEnd::Empty = end {
                        root = tree.pop();
                        stack.pop().unwrap();
                    }
                }
                _ => (/* do nothing */),
            }
        }
        match root {
            Some(Some(root)) => Ok(root),
            _ => Err(format!("[xml] empty view file")),
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

    fn describe(&self) -> String {
        String::from("Loading Template image...")
    }

    fn initialize(&mut self, app: &mut Application, path: &NodePath) -> Result<(), String> {
        app.data_requests.push(DataRequest {
            node: path.clone(),
            name: self.source.clone(),
            range: None,
        });
        Ok(())
    }

    #[allow(unused)]
    fn loaded(
        &mut self,
        app: &mut Application,
        path: &NodePath,
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
            Ok(xml) => parser.parse(&xml).map(|n| app.replace_node(path, n)),
            Err(_) => Err(String::from("Could not parse xml as UTF8 text")),
        };

        if let Err(msg) = result {
            app.log(&format!("TemplateLoader: {}", msg));
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
pub fn import(parser: &mut TreeParser, attributes: &[Attribute]) -> Result<Option<RcNode>, String> {
    let mut tag = Err(String::from("missing tag attribute"));
    let mut source = Err(String::from("missing source attribute"));

    for Attribute { name, value } in attributes {
        match name.as_str() {
            "tag" => tag = Ok(value),
            "src" => source = Ok(value.clone()),
            _ => Err(format!("unexpected attribute: {}", name))?,
        }
    }

    let (tag, source) = (tag?, source?);

    parser.with(
        &tag,
        rc_handler(move |_, parameters| {
            Ok(Some(rc_node(ViewLoader {
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
#[derive(Debug)]
pub struct Spacer {
    pub spot: Spot,
}

impl Node for Spacer {
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }

    fn describe(&self) -> String {
        String::from("Spacer")
    }

    fn policy(&self) -> LengthPolicy {
        LengthPolicy::Remaining(1.0)
    }

    fn get_spot(&self) -> Spot {
        self.spot
    }

    fn set_spot(&mut self, spot: Spot) {
        self.spot = spot;
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
pub fn v_container(_: &mut TreeParser, attributes: &[Attribute]) -> Result<Option<RcNode>, String> {
    container(Axis::Vertical, attributes)
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
/// The `onclick` attribute is optional and specifies an
/// event handler to call when the node receives an
/// [`Event::QuickAction1`](`crate::node::Event::QuickAction1`).
/// See [`Application::add_handler`] to set event handlers up.
///
pub fn h_container(_: &mut TreeParser, attributes: &[Attribute]) -> Result<Option<RcNode>, String> {
    container(Axis::Horizontal, attributes)
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
pub fn spacer(_: &mut TreeParser, attributes: &[Attribute]) -> Result<Option<RcNode>, String> {
    for Attribute { name, .. } in attributes {
        Err(format!("unexpected attribute: {}", name))?;
    }

    Ok(Some(rc_node(Spacer {
        spot: (Point::zero(), Size::zero()),
    })))
}

fn container(axis: Axis, attributes: &[Attribute]) -> Result<Option<RcNode>, String> {
    let mut policy = Err(String::from("missing policy attribute"));
    let mut margin = None;
    let mut radius = None;
    let mut normal_style = None;
    let mut focus_style = None;
    let mut on_click = None;
    let mut gap = 0;

    for Attribute { name, value } in attributes {
        match name.as_str() {
            "onclick" => on_click = Some(value.clone()),
            "margin" => margin = Some(value.parse().map_err(|_| format!("bad value: {}", value))?),
            "radius" => radius = Some(value.parse().map_err(|_| format!("bad value: {}", value))?),
            "gap" => gap = value.parse().map_err(|_| format!("bad value: {}", value))?,
            "fixed" => {
                policy = Ok(LengthPolicy::Fixed(
                    value.parse().map_err(|_| format!("bad value: {}", value))?,
                ))
            }
            "rem" => {
                policy = Ok(LengthPolicy::Remaining(
                    value.parse().map_err(|_| format!("bad value: {}", value))?,
                ))
            }
            "chunks" => {
                policy = Ok(LengthPolicy::Chunks(
                    value.parse().map_err(|_| format!("bad value: {}", value))?,
                ))
            }
            "ratio" => {
                policy = Ok(LengthPolicy::AspectRatio(
                    value.parse().map_err(|_| format!("bad value: {}", value))?,
                ))
            }
            "wrap" => policy = Ok(LengthPolicy::WrapContent),
            "style" => {
                normal_style = Some(value.parse().map_err(|_| format!("bad value: {}", value))?)
            }
            "focus" => {
                focus_style = Some(value.parse().map_err(|_| format!("bad value: {}", value))?)
            }
            _ => Err(format!("unexpected attribute: {}", name))?,
        }
    }

    let spot = (Point::zero(), Size::zero());
    let container = rc_node(Container {
        children: Vec::new(),
        policy: policy?,
        on_click,
        spot,
        prev_spot: spot,
        margin,
        radius,
        axis,
        gap,
        normal_style,
        focus_style,
        focused: false,
        repaint: NeedsRepaint::all(),
        #[cfg(feature = "railway")]
        style_rwy: None,
    });

    Ok(Some(container))
}
