//! Style, Theme, style_index, Color

use crate::bitmap::RGBA;

use microjson::JSONValue;

use alloc::string::String;
use alloc::vec::Vec;

/// A color represented as four bytes.
pub type Color = [u8; RGBA];

fn parse_color(string: &str) -> Option<Color> {
    let len = string.len();
    let (double, grain, times) = match len {
        3 | 4 => Some((true, 1, len)),
        6 | 8 => Some((false, 2, len / 2)),
        _ => None,
    }?;
    let mut color = [0, 0, 0, 255];
    for i in 0..times {
        let sub = &string[i * grain..][..grain];
        let mut c = u8::from_str_radix(sub, 16).ok()?;
        if double {
            c |= c << 4;
        }
        color[i] = c;
    }
    Some(color)
}

/// Represent a node's visual style.
#[derive(Debug, Copy, Clone)]
pub struct Style {
    pub background: Color,
    pub foreground: Color,
    pub outline: Color,
}

/// A theme which can be used by the app.
#[derive(Debug, Clone)]
pub struct Theme {
    pub name: String,
    pub styles: Vec<Style>,
}

const V1_STYLES: [&'static str; 10] = [
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

pub fn style_index(name: &str) -> Option<usize> {
    V1_STYLES.iter().position(|&n| n == name)
}

impl Theme {
    pub fn parse(theme_json: &str) -> Option<Self> {
        let theme = JSONValue::parse_and_verify(theme_json).ok()?;
        let compliance_obj = theme.get_key_value("compliance").ok()?;
        if compliance_obj.read_integer().ok()? == 1 {
            let name_obj = theme.get_key_value("theme").ok()?;
            let name = name_obj.read_string().ok()?.into();
            let styles_obj = theme.get_key_value("styles").ok()?;
            let mut styles = Vec::with_capacity(V1_STYLES.len());
            for name in V1_STYLES {
                let obj = styles_obj.get_key_value(name).ok()?;
                let bg_obj = obj.get_key_value("background").ok()?;
                let fg_obj = obj.get_key_value("foreground").ok()?;
                let ol_obj = obj.get_key_value("outline").ok()?;
                let background = parse_color(bg_obj.read_string().ok()?)?;
                let foreground = parse_color(fg_obj.read_string().ok()?)?;
                let outline = parse_color(ol_obj.read_string().ok()?)?;
                styles.push(Style {
                    background,
                    foreground,
                    outline,
                });
            }
            Some(Self {
                name,
                styles,
            })
        } else {
            None
        }
    }
}
