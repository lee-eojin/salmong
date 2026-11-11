import bevy_sprite_render::mesh2d::{
    vertex_output::VertexOutput,
    view_bindings::view,
};

@if(TONEMAP_IN_SHADER)
import bevy_core_pipeline::tonemapping;
@if(SRGB_OUTPUT)
import bevy_render::color_operations::linear_to_srgb;
@if(OKLAB_OUTPUT)
import bevy_render::color_operations::linear_rgb_to_oklab;

struct ShapeUniform {
    tint: vec4<f32>,
};

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> material: ShapeUniform;

const TAU: f32 = 6.2831855;

fn shape_distance(code: u32, p: vec2<f32>) -> f32 {
    if (code == 0u) {
        return length(p) - 1.0;
    }
    if (code == 1u) {
        return max(-p.y, 0.8660254 * abs(p.x) + 0.5 * p.y) - 0.5;
    }
    if (code == 3u) {
        let len = length(p);
        if (len < 0.000001) {
            return -1.0;
        }
        let sector = TAU / 5.0;
        let a = atan2(p.x, p.y);
        let shifted = a + sector * 0.5;
        let wrapped = shifted - sector * floor(shifted / sector);
        let folded = abs(wrapped - sector * 0.5);
        let q = vec2<f32>(sin(folded), cos(folded)) * len;
        let tip = vec2<f32>(0.0, 1.0);
        let inner = vec2<f32>(sin(sector * 0.5), cos(sector * 0.5)) * 0.45;
        let edge = inner - tip;
        let outward = normalize(vec2<f32>(-edge.y, edge.x));
        return dot(q - tip, outward);
    }
    return -1.0;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let code_f = round(in.uv.x / 4.0);
    let p = vec2<f32>(in.uv.x - 4.0 * code_f, in.uv.y);
    let d = shape_distance(u32(code_f), p);
    let cover = clamp(0.5 - d / max(fwidth(d), 0.000001), 0.0, 1.0);
    var output_color = in.color * material.tint;
    output_color.a = output_color.a * cover;
@if(TONEMAP_IN_SHADER)
    output_color = tonemapping::tone_mapping(output_color, view.color_grading);
@if(SRGB_OUTPUT)
    output_color = vec4(linear_to_srgb(output_color.rgb), output_color.a);
@if(OKLAB_OUTPUT)
    output_color = vec4(linear_rgb_to_oklab(output_color.rgb), output_color.a);
    return output_color;
}
