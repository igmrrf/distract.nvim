// Two passes.
//
// 1. `vs_sprite`/`fs_sprite` draw one instanced quad per visible entity,
//    sampling a sprite atlas that is uploaded once. Per frame this costs a few
//    dozen bytes of instance data instead of re-uploading a full-screen
//    framebuffer.
// 2. `vs_resolve`/`fs_resolve` copy the composited scene to the swapchain,
//    converting the premultiplied result to whatever alpha convention the
//    surface asked for.

struct Uniforms {
    // Sprite pass: viewport size in physical pixels. Resolve pass: unused.
    viewport: vec2<f32>,
    // x != 0 in the resolve pass means the surface wants straight (non
    // premultiplied) alpha, so the composited colour has to be divided back out.
    flags: vec2<f32>,
};

@group(0) @binding(0) var t_source: texture_2d<f32>;
@group(0) @binding(1) var s_source: sampler;
@group(0) @binding(2) var<uniform> u: Uniforms;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
};

// ---------------------------------------------------------------------------
// Sprite pass
// ---------------------------------------------------------------------------

@vertex
fn vs_sprite(
    // Unit quad corner, 0..1 on both axes.
    @location(0) corner: vec2<f32>,
    // Per-instance placement, in physical pixels with the origin top-left.
    @location(1) pos: vec2<f32>,
    @location(2) size: vec2<f32>,
    // Atlas rectangle. Mirroring is expressed by handing in uv_min.x > uv_max.x
    // rather than by keeping a second flipped copy of every frame.
    @location(3) uv_min: vec2<f32>,
    @location(4) uv_max: vec2<f32>,
) -> VertexOutput {
    var out: VertexOutput;
    let px = pos + corner * size;
    let ndc = vec2<f32>(
        px.x / u.viewport.x * 2.0 - 1.0,
        1.0 - px.y / u.viewport.y * 2.0,
    );
    out.clip_position = vec4<f32>(ndc, 0.0, 1.0);
    out.tex_coords = mix(uv_min, uv_max, corner);
    return out;
}

@fragment
fn fs_sprite(in: VertexOutput) -> @location(0) vec4<f32> {
    let c = textureSample(t_source, s_source, in.tex_coords);
    // Premultiply so the pipeline's One / OneMinusSrcAlpha blend composites
    // overlapping sprites correctly.
    return vec4<f32>(c.rgb * c.a, c.a);
}

// ---------------------------------------------------------------------------
// Resolve pass
// ---------------------------------------------------------------------------

@vertex
fn vs_resolve(@builtin(vertex_index) idx: u32) -> VertexOutput {
    // Oversized triangle covering the whole target; cheaper than a quad and
    // needs no vertex buffer.
    var out: VertexOutput;
    let uv = vec2<f32>(f32((idx << 1u) & 2u), f32(idx & 2u));
    out.tex_coords = uv;
    out.clip_position = vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
    // The scene texture has its origin at the top, the clip space at the
    // bottom, so flip vertically here rather than in the sampler.
    out.tex_coords.y = 1.0 - out.tex_coords.y;
    return out;
}

@fragment
fn fs_resolve(in: VertexOutput) -> @location(0) vec4<f32> {
    let c = textureSample(t_source, s_source, in.tex_coords);
    if (u.flags.x > 0.5 && c.a > 0.0031) {
        // Surface wants straight alpha; undo the premultiply. Skipping this is
        // what made every semi-transparent pixel composite at rgb * a^2.
        return vec4<f32>(c.rgb / c.a, c.a);
    }
    return c;
}
