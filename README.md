## 🎨 acrylic

This is a **work-in-progress**, easily portable, small, single-thread, web-inspired user interface toolkit.

## 🪂 Features

- feels familiar to web developers
- support for templating
- extensible
- anti-aliased
- input API designed for improved accessibility
- pure and safe rust
- Fully `no_std`

## 🏗️ Progress (core crate)

- ☑ XML parsing
- ☑ flexbox-like layout
- ☑ PNG images
- ☐ [Railway](https://lib.rs/railway) images [WiP]
- ☑ full `no_std` support
- ☑ textual nodes
- ☑ round containers
- ☐ input events [WiP]
- ☐ event handlers [WiP]
- ☐ text editing
- ☐ scrolling [WiP]
- ☐ rich text
- ☐ external links
- ☐ video playback
- ☐ sound playback

## 🪀 Live Demos

- [demo of version 0.3.0](https://l0.pm/acrylic/)

> more coming soon!

## 🧱 Supported platforms

| platform | Link | Rendering | Asset Loading | Event Handling |
|---|---|---|---|---|
| web | [acrylic-web](https://lib.rs/acrylic-web) | ☑ | ☑ | ☑ |
| wayland | [acrylic-wayland](https://lib.rs/acrylic-wayland) | Glitchy | ☑ |  |
| x11 |  |  |  |  |
| gdi |  |  |  |  |
| fbdev |  |  |  |  |
| drmkms |  |  |  |  |

## ⚡️ Quickstart

### Project structure:

```
.
├── Cargo.toml
├── assets
│   ├── ferris.png
│   └── default.xml
└── src
    └── app.rs
```

### An asset: ferris.png

You can get it [here](https://rustacean.net/assets/rustacean-flat-happy.png).
Place it in `assets/`.

### The view layout: default.xml

```xml
<x rem="1" style="default">
    <inflate />
    <y fixed="400" gap="10">
        <inflate />
        <png src="rustacean-flat-happy.png" />
        <x fixed="40" gap="10">
            <inflate />
            <p txt="Rust rocks!" />
            <inflate />
        </x>
        <inflate />
    </y>
    <inflate />
</x>
```

### The code: app.rs

```rust
use std::rc::Rc;
use platform::app;

fn layout_selector() -> Rc<String> {
    Rc::new("default.xml".into())
}

app!("./assets/", layout_selector, "default.json");
```

### The manifest: Cargo.toml

```toml
[package]
name = "my-app"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = [ "cdylib" ]
path = "src/app.rs"

[dependencies]
acrylic = "0.3.1"

# building for the web
platform = { package = "acrylic-web", version = "0.3.1" }
```

### Building

```bash
cargo build --release --target wasm32-unknown-unknown
```

#### Install a web server

`httpserv` is tiny and good enough for this demo.

```bash
cargo install httpserv
```

#### Download the HTML file which starts your app

You can get it [here](https://raw.githubusercontent.com/NathanRoyer/acrylic/main/acrylic-web/index.html).
Place it at the root of your project, next to the cargo manifest.

#### Start the web server

From the root of your project:

```bash
# normal start:
httpserv

# quiet + in the background
httpserv > /dev/null &
```

Then open http://localhost:8080/#release

### Expected Result

![quickstart.png](https://docs.rs/crate/acrylic/0.3.1/source/quickstart.png)

## ☕ Contact & Contributions

### Contact

You can contact me via [email](mailto:nathan.royer.pro@gmail.com)
or on Discord: `bitsneak#1889`.

You can use these for any question regarding this project.

### Contributions

We gladly accept all contributions via Github PRs.

If you contribute rust code, please put all dependencies
behind features; adding tens of dependencies to this crate
or another one of this project might be a reason for not
merging your PR.

## 👉 See Also

* [egui](https://lib.rs/egui)
* [slint](https://lib.rs/slint)

## 🕯️ License

* MIT for the code
* SIL Open Font License for the embedded Noto Font
