//! XML Layout Parsing

use crate::{error, Error, format, String, Vec, Rc};
use super::app::{Mutator, MutatorIndex, OptionalMutatorIndex};
use core::{ops::Deref, str::from_utf8 as str_from_utf8};
use xmlparser::{Tokenizer, Token, StrSpan};
use oakwood::{NoCookie, index, tree};
use super::KeyValueStore;

index!(LineNumber, OptionalLineNumber);
index!(FileIndex, OptionalFileIndex);

tree!(XmlNodeTree, XmlNode, XmlNodeKey, XmlNodeIndex, OptionalXmlNodeIndex, NoCookie);

/// An XML Node extracted from the layout file
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct XmlNode {
    pub attributes: KeyValueStore,            // 6x4
    pub factory: OptionalMutatorIndex,        // 1x4
    pub file: OptionalFileIndex,              // 1x4
    pub line: OptionalLineNumber,             // 1x4
    // padding                                // 1x4
}

/// Parses an XML Layout file and adds it as a new independant tree in `XmlNodeTree`.
pub fn parse_xml_tree(
    mutators: &mut Vec<Mutator>,
    tree: &mut XmlNodeTree,
    xml_bytes: &[u8],
) -> Result<XmlNodeKey, Error> {
    use Token::*;

    let xml = str_from_utf8(xml_bytes).map_err(|e| error!("xml_bytes: {:?}", e))?;
    let line = |span: StrSpan| xml[..span.start()].lines().count();
    let unexpected = |thing, as_str, span| error!("Unexpected {}: {:?} (line {})", thing, as_str, line(span));
    let unknown = |thing, as_str, span| error!("Unknown {}: {:?} (line {})", thing, as_str, line(span));
    let mutator = |tree: &XmlNodeTree, current, span| {
        let factory = Option::<MutatorIndex>::from(tree[current].factory);
        match factory {
            Some(f) => Ok(&mutators[usize::from(f)]),
            None => Err(error!("malformed XML (line {})", line(span))),
        }
    };

    let mut current = tree.create();
    for token in Tokenizer::from(xml) {
        let token = token.map_err(|e| error!("XML token error: {:?}", e))?;

        /**/ if let ElementStart { prefix, local, span } = token {
            let prefix = prefix.as_str();
            let local = local.as_str();

            if prefix != "" {
                return Err(unexpected("prefix", prefix, span));
            }

            let mut mutator_index = 0;
            for mutator in &*mutators {
                if let Some(tag) = &mutator.xml_tag {
                    if tag.deref() == local {
                        break;
                    }
                }
                mutator_index += 1;
            }
            
            if mutator_index == mutators.len() {
                return Err(unknown("XML tag", local, span));
            }

            let new_node = tree.create();
            tree[new_node].factory = Some(mutator_index.into()).into();
            tree[new_node].line = Some(line(span).into()).into();
            tree.append_children(new_node, current);

            current = new_node;
        }

        else if let Attribute { prefix, local, value, span } = token {
            let value = value.as_str();
            let local = local.as_str();

            if let Some(xml_attr_set) = mutator(tree, current, span)?.xml_attr_set {
                if let None = xml_attr_set.iter().find(|v| *v == &local) {
                    return Err(unknown("attribute", local, span));
                }
            }

            let value_rc = Rc::new(String::from(value));
            let name = match prefix.as_str() {
                "" => local.into(),
                prefix => format!("{}:{}", local, prefix),
            };

            tree[current].attributes.push(name, value_rc);
        }

        else if let ElementEnd { end, span } = token {
            let current_tag = match Option::<MutatorIndex>::from(tree[current].factory) {
                Some(f) => match &mutators[usize::from(f)].xml_tag {
                    Some(tag) => Some(tag.deref()),
                    None => None,
                },
                None => None,
            };

            let pop = if let xmlparser::ElementEnd::Close(prefix, local) = end {
                // "</tag>"

                if !mutator(tree, current, local)?.xml_accepts_children {
                    return Err(unexpected("children", &local, local));
                }

                let prefix = prefix.as_str();
                let local = local.as_str();

                if prefix != "" {
                    return Err(unexpected("prefix", prefix, span));
                }

                if Some(local) != current_tag {
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
                if let Some(node) = tree.parent(current) {
                    current = node;
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
