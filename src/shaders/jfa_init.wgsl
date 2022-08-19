#import outline::fullscreen
#import outline::dimensions

// Jump flood initialization pass.
@group(1) @binding(0)
var mask_buffer: texture_2d<f32>;
@group(1) @binding(1)
var mask_sampler: sampler;

struct FragmentIn {
    @location(0) texcoord: vec2<f32>,
};

@fragment
fn fragment(in: FragmentIn) -> @location(0) vec4<f32> {
    let out_position = vec4<f32>(in.texcoord, 0.0, 1.0);

    // Scaling factor to convert framebuffer to pixel coordinates.
    let fb_to_pix = vec2<f32>(dims.width, dims.height);
    // Pixel coordinates of this fragment.
    let pix_coord = in.texcoord * vec2<f32>(dims.width, dims.height);

    // X- and Y-offsets in framebuffer space.
    let dx = dims.inv_width;
    let dy = dims.inv_height;

    // Fetch 9 samples in a 3x3 grid, jump_dist pixels apart.
    var samples: mat3x3<f32>;
    samples[0][0] = textureSample(mask_buffer, mask_sampler, in.texcoord + vec2<f32>(-dx, -dy)).x;
    samples[0][1] = textureSample(mask_buffer, mask_sampler, in.texcoord + vec2<f32>(-dx, 0.0)).x;
    samples[0][2] = textureSample(mask_buffer, mask_sampler, in.texcoord + vec2<f32>(-dx, dy)).x;
    samples[1][0] = textureSample(mask_buffer, mask_sampler, in.texcoord + vec2<f32>(0.0, -dy)).x;
    samples[1][1] = textureSample(mask_buffer, mask_sampler, in.texcoord).x;
    samples[1][2] = textureSample(mask_buffer, mask_sampler, in.texcoord + vec2<f32>(0.0, dy)).x;
    samples[2][0] = textureSample(mask_buffer, mask_sampler, in.texcoord + vec2<f32>(dx, -dy)).x;
    samples[2][1] = textureSample(mask_buffer, mask_sampler, in.texcoord + vec2<f32>(dx, 0.0)).x;
    samples[2][2] = textureSample(mask_buffer, mask_sampler, in.texcoord + vec2<f32>(dx, dy)).x;

    if (samples[1][1] > 0.99) {
        return out_position;
    }

    if (samples[1][1] < 0.01) {
        return vec4<f32>(-1.0, -1.0, 0.0, 1.0);
    }

    let sobel_x = samples[0][0] + 2.0 * samples[0][1] + samples[0][2] - samples[2][0] - 2.0 * samples[2][1] - samples[2][2];
    let sobel_y = samples[0][0] + 2.0 * samples[1][0] + samples[2][0] - samples[0][2] - 2.0 * samples[1][2] - samples[2][2];
    let dir = -vec2<f32>(sobel_x, sobel_y);

    if (abs(dir.x) < 0.005 && abs(dir.y) < 0.005) {
        return out_position;
    }

    let dir = normalize(dir);
    let offset = dir * (1.0 - samples[1][1]) * vec2<f32>(dx, dy);

    return out_position + vec4<f32>(offset, 0.0, 1.0);
}
