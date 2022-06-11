# bevy_jfa

The Jump Flooding Algorithm (JFA) for Bevy.

## Features

This crate provides an `OutlinePlugin` that can be used to add outlines to
Bevy meshes. See the `examples/` directory for examples of API usage.

## Setup

To add an outline to a mesh:

1. Add the `OutlinePlugin` to the base `App`.
2. Add the desired `OutlineStyle` as an `Asset`.
3. Add a `CameraOutline` component with the desired `OutlineStyle` to the
   camera which should render the outline. Currently, outline styling is
   tied to the camera rather than the mesh.
4. Add an `Outline` component to the mesh with `enabled: true`.

## License

Licensed under either of

- Apache License, Version 2.0
  ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license
  ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
