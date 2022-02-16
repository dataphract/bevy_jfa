#import outline::fullscreen

// Jump flood initialization pass.

struct FragmentIn {
    [[location(0)]] texcoord: vec2<f32>;
};

[[stage(fragment)]]
fn fragment(in: FragmentIn) -> [[location(0)]] vec4<f32> {
    return vec4<f32>(in.texcoord, 0.0, 1.0);
}
