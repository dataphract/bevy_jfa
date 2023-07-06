struct Vertex {
    pos: vec2<f32>,
    texcoord: vec2<f32>,
};

// This forms a CCW triangle larger than the screen:
//
// |`-. NDC [-1, 3] / Texcoord [0, -1]
// |   `-.
// |      `-.
// |         `-.
// |            `-.
// |_______________`-.
// |               |  `-.
// |               |     `-.
// |  Framebuffer  |        `-.
// |               |           `-.
// |_______________|______________`-. NDC [3, -1] / Texcoord [2, 1]
//  \
//   NDC [-1, -1] / Texcoord [0, 1]
//
const VERTICES: array<Vertex, 3> = array<Vertex, 3>(
    // Bottom left
    Vertex(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(0.0, 1.0),
    ),
    // Bottom right
    Vertex(
        vec2<f32>(3.0, -1.0),
        vec2<f32>(2.0, 1.0),
    ),
    // Top left
    Vertex(
        vec2<f32>(-1.0, 3.0),
        vec2<f32>(0.0, -1.0),
    )
);

struct VertexOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) texcoord: vec2<f32>,
};

@vertex
fn vertex(@builtin(vertex_index) idx: u32) -> VertexOut {
    var v: Vertex;
    switch (idx % 3u) {
        case 0u: {
            v = VERTICES[0];
        }
        case 1u: {
            v = VERTICES[1];
        }
        case 2u: {
            v = VERTICES[2];
        }

        // Won't occur, but must be provided.
        default: {
            v = VERTICES[0];
        }
    }

    var out: VertexOut;
    out.pos = vec4<f32>(v.pos, 0.0, 1.0);
    out.texcoord = v.texcoord;
    return out;
}
