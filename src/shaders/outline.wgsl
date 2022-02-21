#import outline::fullscreen
#import outline::dimensions

struct Params {
    color: vec4<f32>;
    // Inverse aspect ratio (height / width).
    inv_aspect: f32;
    // Outline weight in pixels.
    weight: f32;
};

[[group(1), binding(0)]]
var<uniform> params: Params;
[[group(1), binding(1)]]
var jfa_buffer: texture_2d<f32>;
[[group(1), binding(2)]]
var jfa_sampler: sampler;

struct FragmentIn {
    [[location(0)]] texcoord: vec2<f32>;
};

[[stage(fragment)]]
fn fragment(in: FragmentIn) -> [[location(0)]] vec4<f32> {
    let fb_jfa_pos = textureSample(jfa_buffer, jfa_sampler, in.texcoord).xy;
    let fb_to_pix = vec2<f32>(dims.width, dims.height);

    // Fragment position in pixel space.
    let pix_coord = in.texcoord * fb_to_pix;
    // Closest initial fragment in pixel space.
    let pix_jfa_pos = fb_jfa_pos * fb_to_pix;

    let delta = pix_coord - pix_jfa_pos;
    let mag2 = dot(delta, delta);

    // Computed texcoord and stored texcoord are likely to differ even if they
    // represent the same position due to storage as fp16, so an epsilon is
    // needed.
    let weight2 = pow(params.weight, 2.0);
    let fade = clamp((weight2 - mag2) / params.weight, 0.0, 1.0);
    if (mag2 >= 1.0 && fade != 0.0) {
        return vec4<f32>(params.color.rgb, fade);
    } else {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }
}
