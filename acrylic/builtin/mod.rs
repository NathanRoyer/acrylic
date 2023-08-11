//! Built-in XML Tags: Containers, Images, Text
//!
//! # Implementation of built-in containers
//!
//! ## Attributes common to all container types
//!
//! - `for`, `in`: see Iterating Containers
//! - `style`: name of the style to apply to this container
//! - `hover`: style override when the container is hovered by a cursor
//! - `margin`: node margin, as a number of pixels
//! - `border-width`: node border width, as a number of pixels
//! - `border-radius`: node border radius, as a number of pixels
//! - `gap`: gap between children, as a number of pixels
//! - `on-quick-action`: callback to call when the container receives a QuickAction1 user event
//!
//! ## Iterating Containers
//!
//! Containers all have special `for` & `in` attributes which
//! allows the layout to produce one child for each item of a
//! a JSON State list. The container will create a new local
//! state namespace available to all its children, named like
//! the value in the `for` attribute. The list on which the
//! container iterates is specified a the JSON state path
//! in the `in` attribute. For example:
//!
//! ```xml
//! <v-wrap for="person" in="root:club.members">
//!     <h-fixed length="40">
//!         <label person:text="name" />
//!     </h-fixed>
//! </v-wrap>
//! ```
//!
//! This will result in a list of club members, displaying the `name`
//! field of each object in the list at `club` / `members` in the root
//! (main) JSON state namespace.
//!
//! Technically, the container will produce as many children nodes as
//! required by the list and subscribe to that list. Then, when the
//! children nodes are initialized, they will subscribe to individual
//! list items as part of their attribute value lookup. If or When the
//! list is modified (either a specific item or the length of the list),
//! the subscribed nodes will either be replaced by new ones or updated
//! accordingly.
//!
//! ## List of tags
//!
//! ### Wrapping Containers
//!
//! - `<h-wrap>`: horizontal containers wrapping their content
//! - `<v-wrap>`: vertical containers wrapping their content
//!
//! ### Fixed-Length Containers
//!
//! - `<h-fixed>`: fixed-length horizontal containers
//! - `<v-fixed>`: fixed-length vertical containers
//!
//! Special Attribute: `length` (length, in pixels, no default)
//!
//! ### Containers Filling Remaining Space
//!
//! - `<v-rem>`: vertical containers filling remaining space
//! - `<h-rem>`: horizontal containers filling remaining space
//!
//! Special Attribute: `weight` (relative weight, no unit, defaults to 1.0)
//!
//! ### Containers with carriage returns
//!
//! - `<h-chunks>`: horizontal containers with overflow carriage-returns
//! - `<v-chunks>`: vertical containers with overflow carriage-returns
//!
//! Special Attribute: `row` (row length, in pixels, no default)
//!
//! ### Fixed-Aspect-Ratio Containers
//!
//! - `<h-ratio>`: fixed-aspect-ratio horizontal containers
//! - `<v-ratio>`: fixed-aspect-ratio vertical containers
//!
//! Special Attribute: `ratio` (aspect ratio, no unit, defaults to 1.0)
//!
//! # Transparent node taking all remaining space: `<inflate>`
//!
//! The `<inflate>` tag will result in an empty/transparent node taking all
//! remaining space.
//!
//! # Embedding another layout file: `<import>`
//!
//! Embeds another XML layout file into the current one.
//!
//! Special Attribute: `file` (name of the asset, no default)
//!
//! TODO: allow JSON state lookups from nodes in the asset to
//! to start at some path in the JSON state of the app:
//!
//! ```xml
//! <import file="video-player.xml" state="root:videos.5643" />
//! ```
//!
//! Here, when tags in `video-player.xml` refer to `root:something`, they'd be in
//! fact referring to `root:videos.5643.something`.
//!
//! # Textual Nodes: Label & Paragraph
//!
//! ## Attributes common to all textual tags
//!
//! - `text`: the text to be displayed; no default
//! - `font`: asset name for the font, defaults to `default`
//! - `editable`: whether or not to allow text edition; defaults to `false`
//!
//! ## `<label>`
//!
//! Text displayed in a single line.
//!
//! ### Special Attribute: `weight`
//!
//! If this attribute is present, it should specify the relative weight
//! of this label in its container. If it's absent, the label takes as
//! much space as required for its content.
//!
//! ## `<p>`
//!
//! Text displayed as a paragraph, with automatic carriage returns.
//!
//! Special Attribute: `size` (font-size, defaults to 24 pixels)
//!
//! # PNG Images
//!
//! # `<png>`
//!
//! A simple node displaying an image decoded from the PNG format.
//!
//! Special Attribute: `file` (name of the asset, no default)

pub mod container;
pub mod inflate;
pub mod png;
pub mod railway;
pub mod paragraph;
pub mod label;
pub mod import;
