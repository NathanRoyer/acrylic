# Acrylic

Acrylic is a work-in-progress user interface toolkit.

![output.png](https://docs.rs/crate/acrylic/0.1.1/source/output.png)

## What can you do with it?

With it you can lay out glyphs (textual characters), bitmaps and [railway pictures](https://lib.rs/railway).
It has a tree model where you're meant to insert nodes, inspired by the Web DOM.

## Dependencies?

The goal is to keep this project's runtime dependencies to a low amount (< 50).

## Platform support?

As-is, this library can only render elements to an in-memory pixel buffer.
We aim at supporting webassembly and linux framebuffer platforms before Q3 2022.

## XML Parsing?

It is a planned feature.
You would be able to add nodes to the tree based on an xml file which would represent your UI.
This would result in a system very close to how web pages are handled in web browsers.
