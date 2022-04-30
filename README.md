# Acrylic

Acrylic is a work-in-progress user interface toolkit.

- ◔ Familiar to web developers (DOM / XML / JSON)
- ☑ Built-in template system
- ☑ Platform-agnostic
- ☑ Easy to create new elements
- ☑ Small library for easier maintenance
- ☑ Designed with accessibility in mind
- ☑ Great performance
- ☑ Low RAM use

![output.png](https://docs.rs/crate/acrylic/0.1.7/source/output.png)

## Platform support?

As-is, this library can only render elements to an in-memory pixel buffer or png files.
We aim at supporting webassembly and linux framebuffer platforms before Q3 2022.

## XML Parsing?

It is a planned feature.
You would be able to add nodes to the tree based on an xml file which would represent your UI.
This would result in a system very close to how web pages are handled in web browsers.
