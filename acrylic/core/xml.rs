//! XML Layout Parsing

use crate::{error, Error, String, Vec, vec, CheapString, cheap_string, HashMap};
use super::app::{Mutator, MutatorIndex, OptionalMutatorIndex};
use super::visual::{Ratio, Pixels, SignedPixels};
use core::{ops::Deref, str::from_utf8 as str_from_utf8};
use xmlparser::{Tokenizer, Token, StrSpan};
use oakwood::{NoCookie, index, tree};

index!(LineNumber, OptionalLineNumber);
index!(FileIndex, OptionalFileIndex);

tree!(XmlNodeTree, XmlNode, XmlNodeKey, XmlNodeIndex, OptionalXmlNodeIndex, NoCookie);

/// Parsing parameters for an XML Tag
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct XmlTagParameters {
    pub tag_name: CheapString,
    /// (xml_name, type, optional_default_value)
    pub attr_set: &'static [(&'static str, AttributeValueType, Option<&'static str>)],
    pub accepts_children: bool,
}

/// An XML Node extracted from the layout file
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct XmlNode {
    pub attributes: AttributeValueVec,
    pub factory: OptionalMutatorIndex,
    pub file: OptionalFileIndex,
    pub line: OptionalLineNumber,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
#[repr(transparent)]
pub struct AttributeValueVec(Vec<AttributeValue>);

/// Parses an XML Layout file and adds it as a new independant tree in `XmlNodeTree`.
pub fn parse_xml_tree(
    mutators_params: HashMap<str, (&XmlTagParameters, MutatorIndex)>,
    ordered: &[Mutator],
    tree: &mut XmlNodeTree,
    xml_bytes: &[u8],
) -> Result<XmlNodeKey, Error> {
    use Token::*;

    let xml = str_from_utf8(xml_bytes).map_err(|e| error!("xml_bytes: {:?}", e))?;
    let line = |span: StrSpan| xml[..span.start()].lines().count();
    let unexpected = |thing, as_str, span| error!("Unexpected {}: {:?} (line {})", thing, as_str, line(span));
    let unknown = |thing, as_str, span| error!("Unknown {}: {:?} (line {})", thing, as_str, line(span));

    let mut current = tree.create();
    let mut xml_params = mutators_params.get("import").unwrap().0;
    for token in Tokenizer::from(xml) {
        let token = token.map_err(|e| error!("XML token error: {:?}", e))?;

        /**/ if let ElementStart { prefix, local, span } = token {
            let prefix = prefix.as_str();
            let local = local.as_str();

            if prefix != "" {
                return Err(unexpected("prefix", prefix, span));
            }

            let (new_xml_params, index) = match mutators_params.get(local) {
                Some(tuple) => tuple,
                None => return Err(unknown("XML tag", local, span)),
            };

            let new_node = tree.create();
            tree[new_node].factory = Some(*index).into();
            tree[new_node].line = Some(line(span).into()).into();
            tree[new_node].attributes = AttributeValueVec::new(new_xml_params);
            tree.append_children(new_node, current);

            current = new_node;
            xml_params = new_xml_params;
        }

        else if let Attribute { prefix, local, value, span } = token {
            let value = value.as_str();
            let local = local.as_str();

            // attr.0 is the xml name of the attribute
            let index = match xml_params.attr_set.iter().position(|attr| attr.0 == local) {
                Some(index) => index,
                None => return Err(unknown("attribute", local, span)),
            };

            let value_type = xml_params.attr_set[index].1;
            let value_rc = String::from(value);

            tree[current].attributes.0[index] = match prefix.as_str() {
                "" => AttributeValue::parse(&value_rc.into(), value_type)?,
                prefix => AttributeValue::StateLookup { 
                    namespace: String::from(prefix).into(),
                    path: value_rc.into(),
                    value_type,
                },
            };
        }

        else if let ElementEnd { end, span } = token {
            let current_tag = xml_params.tag_name.deref();

            let pop = if let xmlparser::ElementEnd::Close(prefix, local) = end {
                // "</tag>"

                if !xml_params.accepts_children {
                    return Err(unexpected("children", &local, local));
                }

                let prefix = prefix.as_str();
                let local = local.as_str();

                if prefix != "" {
                    return Err(unexpected("prefix", prefix, span));
                }

                if local != current_tag {
                    return Err(unexpected("close tag", local, span));
                }

                true
            } else if let xmlparser::ElementEnd::Empty = end {
                // "/>"
                true
            } else {
                // ">"
                false
            };

            if pop {
                if let Some(i) = tree[current].attributes.0.iter().position(|a| a == &AttributeValue::Unset) {
                    let (attr_name, attr_type, _) = xml_params.attr_set[i];
                    if required(attr_type) {
                        return Err(error!("Missing XML attribute: {} (line {})", attr_name, line(span)));
                    }
                }

                if let Some(node) = tree.parent(current) {
                    current = node;
                    if let Some(index) = tree[current].factory.get() {
                        xml_params = ordered[usize::from(index)].xml_params.as_ref().unwrap();
                    }
                } else {
                    return Err(error!("malformed XML: {:?} (line {})", current_tag, line(span)));
                }
            }
        }

        else if let Comment { .. } = token {
            // ignore comments
        }

        else if let Text { text } = token {
            let text_str = text.as_str().trim();
            if text_str != "" {
                return Err(unexpected("text", text_str, text));
            }
        }

        else {
            return Err(error!("Unknown token: {:?}", token));
        }
    }

    let node = match tree.detach_children(current) {
        Some(node) => Ok(node),
        None => Err(error!("XML file appears to be empty; at least one node is required.")),
    }?;

    tree.delete(current);

    Ok(node)
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum AttributeValueType {
    SignedPixels = 0,
    Pixels,
    Ratio,
    Other,
    OptSignedPixels,
    OptPixels,
    OptRatio,
    OptOther,
}

const fn required(t: AttributeValueType) -> bool {
    (t as u8) < 4
}

/// A Parsed XML Attribute value
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AttributeValue {
    OptSignedPixels(Option<SignedPixels>),
    SignedPixels(SignedPixels),
    OptPixels(Option<Pixels>),
    Pixels(Pixels),
    OptRatio(Option<Ratio>),
    Ratio(Ratio),
    OptOther(Option<CheapString>),
    Other(CheapString),
    StateLookup { 
        namespace: CheapString,
        path: CheapString,
        value_type: AttributeValueType,
    },
    Unset,
}

impl AttributeValue {
    /// Tries to parse an XML Value as some value type
    pub fn parse(xml_value: &CheapString, attr_type: AttributeValueType) -> Result<Self, Error> {
        use AttributeValueType::*;

        macro_rules! parse_attr {
            ($xml_value:ident, $variant:ident, $msg:literal, true) => {
                match $xml_value.deref().parse() {
                    Ok(num) => Ok(Self::$variant(num)),
                    Err(e) => Err(error!("Couldn't parse {} as {}: {}", $xml_value, $msg, e)),
                }
            };
            ($xml_value:ident, $variant:ident, $msg:literal, false) => {
                match $xml_value.deref().parse() {
                    Ok(num) => Ok(Self::$variant(Some(num))),
                    Err(e) => Err(error!("Couldn't parse {} as {}: {}", $xml_value, $msg, e)),
                }
            };
        }

        match attr_type {
            OptSignedPixels => parse_attr!(xml_value, OptSignedPixels, "a signed number of pixels", false),
            SignedPixels => parse_attr!(xml_value, SignedPixels, "a signed number of pixels", true),
            OptPixels => parse_attr!(xml_value, OptPixels, "an unsigned number of pixels", false),
            Pixels => parse_attr!(xml_value, Pixels, "an unsigned number of pixels", true),
            OptRatio => parse_attr!(xml_value, OptRatio, "a ratio", false),
            Ratio => parse_attr!(xml_value, Ratio, "a ratio", true),
            OptOther => Ok(Self::OptOther(Some(xml_value.clone()))),
            Other => Ok(Self::Other(xml_value.clone())),
        }
    }
}

impl AttributeValueVec {
    pub(crate) fn new_import(layout_asset: CheapString) -> Self {
        Self(vec![ AttributeValue::Other(layout_asset) ])
    }

    pub fn new(params: &XmlTagParameters) -> Self {
        let mut vec = Vec::with_capacity(params.attr_set.len());

        for (_name, attr_type, default_value) in params.attr_set {
            vec.push(match default_value {
                Some(xml_value) => AttributeValue::parse(&cheap_string(xml_value), *attr_type).unwrap(),
                None => AttributeValue::Unset,
            });
        }

        Self(vec)
    }

    pub fn get(&self, index: usize) -> &AttributeValue {
        self.0.get(index).expect("Invalid Attribute Definition")
    }
}

macro_rules! impl_try_from {
    ($dst_type:ty, $variant:ident) => {
        impl TryFrom<AttributeValue> for $dst_type {
            type Error = Error;
            fn try_from(value: AttributeValue) -> Result<Self, Self::Error> {
                match value {
                    AttributeValue::$variant(inner) => Ok(inner),
                    _ => panic!("Invalid Attribute Configuration"),
                }
            }
        }
    }
}

macro_rules! impl_try_from_opt {
    ($dst_type:ty, $variant:ident) => {
        impl TryFrom<AttributeValue> for $dst_type {
            type Error = Error;
            fn try_from(value: AttributeValue) -> Result<Self, Self::Error> {
                match value {
                    AttributeValue::$variant(inner) => Ok(inner),
                    AttributeValue::Unset => Ok(None),
                    _ => panic!("Invalid Attribute Configuration"),
                }
            }
        }
    }
}

impl_try_from_opt!(Option<SignedPixels>, OptSignedPixels);
impl_try_from_opt!(Option<Pixels>, OptPixels);
impl_try_from_opt!(Option<Ratio>, OptRatio);
impl_try_from_opt!(Option<CheapString>, OptOther);

impl_try_from!(SignedPixels, SignedPixels);
impl_try_from!(Pixels, Pixels);
impl_try_from!(Ratio, Ratio);
impl_try_from!(CheapString, Other);
