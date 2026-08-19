// Voxel mesh pass.
//
// One instanced draw per (asset, frame) group: the mesh comes out of the shared
// vertex buffer at a range the CPU picked, and every entity showing that frame
// arrives as an instance. Depth-tested rather than sorted, which is the whole
// reason this is a separate pass from the sprite one.

struct Uniforms3d {
    view_proj: mat4x4<f32>,
    // xyz: unit direction the light travels. w: ambient floor.
    light: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms3d;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) colour: vec4<f32>,
    @location(1) normal: vec3<f32>,
};

// A yaw about the model's own vertical axis. The model is centred on x and z, so
// this turns the pet without also moving it.
fn yaw_about_y(point: vec3<f32>, yaw: f32) -> vec3<f32> {
    let sine = sin(yaw);
    let cosine = cos(yaw);
    return vec3<f32>(
        point.x * cosine + point.z * sine,
        point.y,
        -point.x * sine + point.z * cosine,
    );
}

@vertex
fn vs_mesh(
    // Model space, one unit per voxel.
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) colour: vec4<f32>,
    // Per instance: where the model's top centre goes, in pixels, plus its yaw.
    @location(3) placement: vec4<f32>,
    // Per instance: pixels per voxel on each axis, plus the model's opacity.
    @location(4) scaling: vec4<f32>,
) -> VertexOutput {
    var out: VertexOutput;

    let turned = yaw_about_y(position, placement.w);
    let world = placement.xyz + turned * scaling.xyz;
    out.clip_position = u.view_proj * vec4<f32>(world, 1.0);

    // Scaling a face along its own axes never turns it, so yawing the normal is
    // exact here and an inverse-transpose would be the same matrix.
    out.normal = yaw_about_y(normal, placement.w);
    out.colour = vec4<f32>(colour.rgb, colour.a * scaling.w);
    return out;
}

@fragment
fn fs_mesh(in: VertexOutput) -> @location(0) vec4<f32> {
    let facing = normalize(in.normal);
    let lambert = max(dot(facing, -u.light.xyz), 0.0);
    let shade = u.light.w + (1.0 - u.light.w) * lambert;
    let lit = in.colour.rgb * shade;
    // Premultiplied, matching the sprite pass: both composite into one scene
    // texture and the target's blend is One / OneMinusSrcAlpha.
    return vec4<f32>(lit * in.colour.a, in.colour.a);
}
