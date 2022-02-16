#import outline::fullscreen

struct JumpDist {
    dist: u32;
};

struct Dimensions {
    width: u32;
    height: u32;
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
    [[builtin(position)]] position: vec4<f32>;
    [[location(0)]] texcoord: vec2<f32>;
};

[[stage(fragment)]]
fn fragment(in: FragmentIn) -> [[location(0)]] vec4<f32> {
    // Absolute x-offset for samples to the left and right
    //let dx = f32(jump_dist.dist) / f32(dims.width);
    // Absolute y-offset for samples to the top and bottom
    //let dy = f32(jump_dist.dist) / f32(dims.height);
    let inv_width = 1.0 / f32(dims.width);
    let inv_height = 1.0 / f32(dims.height);

    let texcoord = vec2<f32>(
        inv_width * in.position.x,
        inv_height * in.position.y,
    );

    var min_dist2: f32 = 1.0 / 0.0; // infinity
    var min_dist2_pos: vec2<f32> = vec2<f32>(-1.0, -1.0);
    for (var i: i32 = -1; i < 2; i = i + 1) {
        let x_dist = i32(jump_dist.dist) * i;
        let sample_x = inv_width * (in.position.x + f32(x_dist));

        for (var j: i32 = -1; j < 2; j = j + 1) {
            let y_dist = i32(jump_dist.dist) * j;
            let sample_y = inv_height * (in.position.y + f32(y_dist));

            let sample_texcoord = vec2<f32>(sample_x, sample_y);
            var sampled_pos = textureSample(src_buffer, src_sampler, sample_texcoord);
            let delta = texcoord - sampled_pos.xy;
            let dist2 = dot(delta, delta);

            // It doesn't seem as though there's a way to avoid this branch :(
            if (sampled_pos.x != -1.0 && dist2 < min_dist2) {
                min_dist2 = dist2;
                min_dist2_pos = sampled_pos.xy;
            }
        }
    }

    return vec4<f32>(min_dist2_pos, 0.0, 1.0);
}
