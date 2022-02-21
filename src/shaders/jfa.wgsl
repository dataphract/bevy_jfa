#import outline::fullscreen
#import outline::dimensions

// Bind group 0 imported from outline::dimensions

struct JumpDist {
    dist: u32;
};

[[group(1), binding(0)]]
var<uniform> jump_dist: JumpDist;
[[group(1), binding(1)]]
var src_buffer: texture_2d<f32>;
[[group(1), binding(2)]]
var src_sampler: sampler;

struct FragmentIn {
    [[location(0)]] texcoord: vec2<f32>;
};

[[stage(fragment)]]
fn fragment(in: FragmentIn) -> [[location(0)]] vec4<f32> {
    // Scaling factor to convert framebuffer to pixel coordinates.
    let fb_to_pix = vec2<f32>(dims.width, dims.height);
    // Pixel coordinates of this fragment.
    let pix_coord = in.texcoord * vec2<f32>(dims.width, dims.height);

    // X- and Y-offsets in framebuffer space.
    let dx = dims.inv_width * f32(jump_dist.dist);
    let dy = dims.inv_height * f32(jump_dist.dist);

    // TODO: this is actually the largest finite f32. WGSL doesn't seem to have
    // a way to write an infinity float literal.
    let infinity = 0x1.FFFFFp127;
    // Minimum pixel-space distance between this fragment and one of the initial fragments.
    var min_dist2: f32 = infinity;
    // The framebuffer-space position of the closest initial fragment.
    var min_dist2_pos: vec2<f32> = vec2<f32>(-1.0, -1.0);

    // Fetch 9 samples in a 3x3 grid, jump_dist pixels apart.
    var samples: array<vec2<f32>, 9>;
    samples[0] = textureSample(src_buffer, src_sampler, in.texcoord + vec2<f32>(-dx, -dy)).xy;
    samples[1] = textureSample(src_buffer, src_sampler, in.texcoord + vec2<f32>(-dx, 0.0)).xy;
    samples[2] = textureSample(src_buffer, src_sampler, in.texcoord + vec2<f32>(-dx, dy)).xy;
    samples[3] = textureSample(src_buffer, src_sampler, in.texcoord + vec2<f32>(0.0, -dy)).xy;
    samples[4] = textureSample(src_buffer, src_sampler, in.texcoord).xy;
    samples[5] = textureSample(src_buffer, src_sampler, in.texcoord + vec2<f32>(0.0, dy)).xy;
    samples[6] = textureSample(src_buffer, src_sampler, in.texcoord + vec2<f32>(dx, -dy)).xy;
    samples[7] = textureSample(src_buffer, src_sampler, in.texcoord + vec2<f32>(dx, 0.0)).xy;
    samples[8] = textureSample(src_buffer, src_sampler, in.texcoord + vec2<f32>(dx, dy)).xy;

    for (var i: i32 = 0; i < 9; i = i + 1) {
        let fb_sample = samples[i];
        let valid = fb_sample.x != -1.0;

        // Convert sample to pixel coordinates when computing distance.
        let pix_sample = fb_sample * fb_to_pix;
        let delta = pix_coord - pix_sample;
        let dist2 = dot(delta, delta);

        // It doesn't seem as though there's a way to avoid this branch :(
        if (valid && dist2 < min_dist2) {
            min_dist2 = dist2;
            min_dist2_pos = fb_sample;
        }
    }

    return vec4<f32>(min_dist2_pos, 0.0, 1.0);
}
