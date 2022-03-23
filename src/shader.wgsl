// Neonet shader.

let point_count: u32 = 200u;
let line_length: f32 = 200.0;

struct VertexData {
    position: vec2<f32>;
    color: vec3<f32>;
};

struct Vertices {
    vertices: array<VertexData, point_count>;
};

struct UniformData {
    screen_width: f32;
    screen_height: f32;
};

struct VertexIndex {
    [[location(0)]] me: u32;
    [[location(1)]] other: u32;
    [[location(2)]] distance_sqr: f32;
};

struct VertexOutput {
    [[builtin(position)]] position: vec4<f32>;
    [[location(0)]] color: vec4<f32>;
};

[[group(0), binding(0)]]
var<uniform> uniform_data: UniformData;

[[group(0), binding(1)]]
var<uniform> vertices: Vertices;

[[stage(vertex)]]
fn vert_main(index: VertexIndex) -> VertexOutput {
    var output: VertexOutput;
    var me = vertices.vertices[index.me];
    var x = me.position.x / uniform_data.screen_width * 2.0 - 1.0;
    var y = me.position.y / uniform_data.screen_height * 2.0 - 1.0;
    output.position = vec4<f32>(x, y, 0.0, 1.0);
    var distance = sqrt(index.distance_sqr);
    output.color = vec4<f32>(me.color, 1.0 - distance / line_length);
    return output;
}

[[stage(fragment)]]
fn frag_main(in: VertexOutput) -> [[location(0)]] vec4<f32> {
    return in.color;
}
