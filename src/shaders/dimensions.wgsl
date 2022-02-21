struct Dimensions {
    // Framebuffer width in pixels.
    width: f32;
    // Framebuffer height in pixels.
    height: f32;
    // Reciprocal of width.
    inv_width: f32;
    // Reciprocal of height.
    inv_height: f32;
};

[[group(0), binding(0)]]
var<uniform> dims: Dimensions;
