//! Style, Theme, style_index, Color

use rgb::RGBA8;
use lmfu::json::{JsonFile, JsonValue, JsonPath};
use crate::{Error, error, ArcStr, Vec};

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
    pub name: ArcStr,
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
    /// This expects a JSON theme with the following structure:
    ///
    /// ```json
    /// {
    ///     "name": "My Beautiful Theme",
    ///     "version": 0,
    ///     "styles": {
    ///         "default": {
    ///             "background": "222F",
    ///             "foreground": "EEEF",
    ///             "outline": "999F"
    ///         },
    ///         "menu-1": {
    ///             "background": "333F",
    ///             "foreground": "EEEF",
    ///             "outline": "999F"
    ///         },
    ///         ...
    ///     }
    /// }
    /// ```
    ///
    /// And the following styles:
    /// - `default`,
    /// - `menu-1`,
    /// - `menu-2`,
    /// - `menu-3`,
    /// - `neutral-inert`,
    /// - `neutral-focus`,
    /// - `warn-inert`,
    /// - `warn-focus`,
    /// - `incite-inert`,
    /// - `incite-focus`
    ///
    pub fn parse(theme_json: &str) -> Result<Self, Error> {
        let theme = JsonFile::parse(theme_json).map_err(|e| error!("JSON Style: parsing error: {:?}", e))?;

        macro_rules! expect {
            ($theme:ident, $path:expr) => {
                match &$theme[$path] {
                    JsonValue::String(string) => string,
                    _ => return Err(error!("JSON Style: missing {:?} (or it's not a string)", stringify!($path))),
                }
            }
        }

        let version = &theme[["version"]];
        if version == &JsonValue::Number(0.0) {
            let name = expect!(theme, ["name"]).clone();
            let mut styles = Vec::with_capacity(V0_STYLES.len());

            for style in V0_STYLES {
                let path: JsonPath = ["styles", style].into();
                styles.push(Style {
                    background: parse_color(&expect!(theme, path.clone().index_str("background")))?,
                    foreground: parse_color(&expect!(theme, path.clone().index_str("foreground")))?,
                    outline:    parse_color(&expect!(theme, path.clone().index_str("outline")))?,
                });
            }

            Ok(Self {
                name,
                styles,
            })
        } else {
            Err(error!("JSON Style: Unsupported theme version: {:?}", version))
        }
    }

    pub fn get(&self, name: &str) -> Option<Style> {
        Some(self.styles[V0_STYLES.iter().position(|&n| n == name)?])
    }
}
