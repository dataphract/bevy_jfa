#import outline::fullscreen
#import outline::dimensions

struct Params {
    color: vec4<f32>,
    // Outline weight in pixels.
    weight: f32,
};

@group(1) @binding(0)
var jfa_buffer: texture_2d<f32>;
@group(1) @binding(1)
var mask_buffer: texture_2d<f32>;
@group(1) @binding(2)
var nearest_sampler: sampler;

@group(2) @binding(0)
var<uniform> params: Params;

struct FragmentIn {
    @location(0) texcoord: vec2<f32>,
};

@fragment
fn fragment(in: FragmentIn) -> @location(0) vec4<f32> {
    let fb_jfa_pos = textureSample(jfa_buffer, nearest_sampler, in.texcoord).xy;
    let fb_to_pix = vec2<f32>(dims.width, dims.height);

    let mask_value = textureSample(mask_buffer, nearest_sampler, in.texcoord).r;

    // Fragment position in pixel space.
    let pix_coord = in.texcoord * fb_to_pix;
    // Closest initial fragment in pixel space.
    let pix_jfa_pos = fb_jfa_pos * fb_to_pix;

    let delta = pix_coord - pix_jfa_pos;
    let mag = sqrt(dot(delta, delta));

    // Computed texcoord and stored texcoord are likely to differ even if they
    // represent the same position due to storage as fp16, so an epsilon is
    // needed.
    if (mask_value < 1.0) {
        if (mask_value > 0.0) {
            return vec4<f32>(params.color.rgb, 1.0 - mask_value);
        } else {
            let fade = clamp(params.weight - mag, 0.0, 1.0);
            return vec4<f32>(params.color.rgb, fade);
        }
    } else {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }
}
