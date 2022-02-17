#import outline::fullscreen

struct JumpDist {
    dist: u32;
};

struct Dimensions {
    inv_width: f32;
    inv_height: f32;
};

[[group(0), binding(0)]]
var<uniform> jump_dist: JumpDist;
[[group(0), binding(1)]]
var<uniform> dims: Dimensions;
[[group(0), binding(2)]]
var src_buffer: texture_2d<f32>;
[[group(0), binding(3)]]
var src_sampler: sampler;

struct FragmentIn {
    [[location(0)]] texcoord: vec2<f32>;
};

[[stage(fragment)]]
fn fragment(in: FragmentIn) -> [[location(0)]] vec4<f32> {
    let dx = dims.inv_width * f32(jump_dist.dist);
    let dy = dims.inv_height * f32(jump_dist.dist);

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

    // TODO: this is actually the largest finite f32. WGSL doesn't seem to have
    // a way to write an infinity float literal.
    let infinity = 0x1.FFFFFp127;

    var min_dist2: f32 = infinity;
    var min_dist2_pos: vec2<f32> = vec2<f32>(-1.0, -1.0);
    for (var i: i32 = 0; i < 9; i = i + 1) {
        let sample = samples[i];
        let delta = in.texcoord - sample;
        let dist2 = dot(delta, delta);

        // It doesn't seem as though there's a way to avoid this branch :(
        if (sample.x != -1.0 && dist2 < min_dist2) {
            min_dist2 = dist2;
            min_dist2_pos = sample;
        }
    }

    return vec4<f32>(min_dist2_pos, 0.0, 1.0);
}
