//! A work-in-progress, easily portable,
//! small, web-inspired user interface toolkit.
//! 
//! ### Example project structure:
//! 
//! ```text
//! ├── Cargo.toml
//! ├── assets
//! │   ├── ferris.png
//! │   └── default.xml
//! └── src
//!     └── app.rs
//! ```
//! 
//! #### An asset: ferris.png
//! 
//! You can get it [here](https://rustacean.net/assets/rustacean-flat-happy.png)
//! 
//! #### The view layout: default.xml
//! 
//! ```xml
//! <x rem="1" style="default">
//!     <inflate />
//!     <y fixed="400" gap="10">
//!         <inflate />
//!         <png src="ferris.png" />
//!         <x fixed="40" gap="10">
//!             <inflate />
//!             <p txt="Rust rocks!" />
//!             <inflate />
//!         </x>
//!         <inflate />
//!     </y>
//!     <inflate />
//! </x>
//! ```
//! 
//! #### The code: app.rs
//!
//! ```rust
//! use platform::app;
//! use acrylic::app::Application;
//! use acrylic::xml::ViewLoader;
//! 
//! app!("assets/", {
//!     let loader = ViewLoader::new("default.xml");
//!     Application::new((), loader)
//! });
//! ```
//! 
//! #### The manifest: Cargo.toml
//! 
//! ```toml
//! [package]
//! name = "my-app"
//! version = "0.1.0"
//! edition = "2021"
//! 
//! [lib]
//! crate-type = [ "cdylib" ]
//! path = "src/app.rs"
//! 
//! [dependencies]
//! acrylic = "0.2.0"
//! 
//! # building for the web
//! platform = { package = "acrylic-web", version = "0.2.0" }
//! ```
//! 
//! #### Building
//! 
//! ```bash
//! cargo build --target wasm32-unknown-unknown
//! ```
//! 
//! #### Install a web server
//! 
//! `httpserv` is tiny and good enough for this demo.
//! 
//! ```bash
//! cargo install httpserv
//! ```
//! 
//! #### Start the web server
//! 
//! ```bash
//! # normal start:
//! httpserv
//! 
//! # quiet + in the background
//! httpserv > /dev/null &
//! ```
//! 
//! Then open http://localhost:8080/#release
//! 
//! #### Expected Result
//! 
//! ![quickstart.png](https://docs.rs/crate/acrylic/0.2.1/source/quickstart.png)
//! 
//! ### app.rs code walkthrough
//!
//! ```rust
//! // this is macro import.
//! // each platform implementation is required
//! // to provide an `app` macro which either
//! // maps to a `main` function or to whatever
//! // is an entry point for that platform.
//! use platform::app;
//! 
//! // A structure which owns fonts, views,
//! // event handlers, assets: the state of
//! // an `acrylic` application.
//! use acrylic::app::Application;
//! 
//! // A structure capable of mapping an XML
//! // file to a view layout.
//! use acrylic::xml::ViewLoader;
//! 
//! // This translates to an entry point.
//! // We also specify the location of our
//! // assets.
//! app!("assets/", {
//! 
//!     // instanciate our view layout
//!     let loader = ViewLoader::new("default.xml");
//! 
//!     // creates an Application object
//!     // The second parameter is our model
//!     // of this real-world application.
//!     // We can store any Sized data here
//!     // and get it back with Application::model().
//!     Application::new((), loader)
//! 
//!     // Before returning the Application to the
//!     // platform, where it will be managed in an
//!     // event loop, we could also add named event
//!     // handlers using Application::add_handler().
//!     // On certain element types, you can redirect
//!     // events to these handlers; for instance,
//!     // acrylic::text::xml_paragraph accepts handlers
//!     // for text edition and submission.
//! });
//! ```
//! 
//! ### List of built-in XML tags
//! 
//! Please refer to [`with_builtin_tags`](`crate::xml::TreeParser::with_builtin_tags`).
//! 

#![no_std]

extern crate alloc;

pub mod app;
pub mod bitmap;
pub mod flexbox;
pub mod geometry;
pub mod node;
pub mod container;
pub mod style;

#[cfg(feature = "text")]
pub mod text;

#[cfg(feature = "text")]
pub mod font;

#[cfg(feature = "xml")]
pub mod xml;

#[cfg(feature = "png")]
pub mod png;

#[cfg(feature = "railway")]
pub mod railway;

/// General-purpose position structure
pub type Point = geometry::Point;

/// General-purpose size structure
pub type Size = geometry::Size;

/// A framebuffer geographical window
pub type Spot<'a> = geometry::Spot<'a>;

/// Non-verbose result
pub type Status = Result<(), ()>;

/// Transforms an `Option<T>` to a `Result<T, ()>`
/// which is compatible with [`Status`]
pub fn status<T>(option: Option<T>) -> Result<T, ()> {
    option.ok_or(())
}

/// `no_std`-friendly [`std::format`](https://doc.rust-lang.org/std/macro.format.html)
#[macro_export]
macro_rules! format {
    ($($arg:tt)*) => {{
        let mut string = alloc::string::String::new();
        core::fmt::write(&mut string, core::format_args!($($arg)*)).unwrap();
        string
    }}
}

#[macro_export]
macro_rules! round {
    ($arg:expr, $in:ty, $out:ty) => {{
        // given float > 0
        let mut float = $arg;
        let integer = float as $out;
        float -= integer as $in;
        match float > 0.5 {
            true => integer + 1,
            false => integer,
        }
    }}
}
