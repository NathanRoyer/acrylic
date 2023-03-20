//! Style, Theme, style_index, Color

use rgb::RGBA8;
use serde_json::Value as JsonValue;
use crate::{Error, error, String, Vec};

fn parse_color(string: &str) -> Result<RGBA8, Error> {
    let len = string.len();
    let (double, grain, times) = match len {
        3 | 4 => Ok((true, 1, len)),
        6 | 8 => Ok((false, 2, len / 2)),
        _ => Err(error!("Theme JSON: Invalid color: {:?}", string)),
    }?;

    let mut color = [0, 0, 0, 255];
    for i in 0..times {
        let sub = &string[i * grain..][..grain];
        let mut c = u8::from_str_radix(sub, 16).map_err(|_| {
            error!("Theme JSON: Invalid color: {:?}", string)
        })?;

        if double {
            c |= c << 4;
        }

        color[i] = c;
    }
    Ok(color.into())
}

/// Represent a node's visual style.
#[derive(Debug, Copy, Clone)]
pub struct Style {
    pub background: RGBA8,
    pub foreground: RGBA8,
    pub outline: RGBA8,
}

pub const DEFAULT_STYLE: &'static str = "default";

/// A theme which can be used by the app.
#[derive(Debug, Clone)]
pub struct Theme {
    pub name: String,
    pub styles: Vec<Style>,
}

const V0_STYLES: [&'static str; 10] = [
    "default",
    "menu-1",
    "menu-2",
    "menu-3",
    "neutral-inert",
    "neutral-focus",
    "warn-inert",
    "warn-focus",
    "incite-inert",
    "incite-focus",
];

impl Theme {
    pub fn parse(theme_json: &str) -> Result<Self, Error> {
        let errmsg = "Error while parsing theme JSON";
        let mut theme: JsonValue = serde_json::from_str(theme_json).map_err(|e| error!("{}: {:?}", errmsg, e))?;
        let expect = |obj: &mut JsonValue, key: &'static str| {
            if let Some(obj) = obj.as_object_mut() {
                match obj.remove(key) {
                    Some(JsonValue::String(string)) => Ok(string),
                    _ => Err(error!("{}: missing {:?}", errmsg, key)),
                }
            } else {
                Err(error!("{}: incorrect structure", errmsg))
            }
        };

        let version = &theme["version"];
        if version.as_u64() == Some(0) {
            let name = expect(&mut theme, "name")?;
            let mut styles = Vec::with_capacity(V0_STYLES.len());

            for name in V0_STYLES {
                let obj = &mut theme["styles"][name];
                let background = parse_color(&expect(obj, "background")?)?;
                let foreground = parse_color(&expect(obj, "foreground")?)?;
                let outline    = parse_color(&expect(obj, "outline")?)?;
                styles.push(Style {
                    background,
                    foreground,
                    outline,
                });
            }

            Ok(Self {
                name,
                styles,
            })
        } else {
            Err(error!("Unsupported theme JSON version: {:?}", version))
        }
    }

    pub fn get(&self, name: &str) -> Option<Style> {
        Some(self.styles[V0_STYLES.iter().position(|&n| n == name)?])
    }
}
