#import bevy_pbr::forward_io::VertexOutput

// Must match MAX_LAYERS in composite.rs
const MAX_LAYERS: u32 = 8u;

struct LayerData {
    atlas_uv_min:   vec2<f32>,  // offset  0
    atlas_uv_max:   vec2<f32>,  // offset  8
    canvas_offset:  vec2<f32>,  // offset 16  (top-left of this layer in canvas pixels)
    layer_size:     vec2<f32>,  // offset 24  (pixel dimensions of this layer's frame)
}

struct CompositeData {
    canvas_size:    vec2<f32>,  // offset  0
    layer_count:    u32,        // offset  8
    _pad:           u32,        // offset 12
    layers:         array<LayerData, 8>,  // offset 16
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0)
var textures: binding_array<texture_2d<f32>>;

@group(#{MATERIAL_BIND_GROUP}) @binding(1)
var nearest_sampler: sampler;

@group(#{MATERIAL_BIND_GROUP}) @binding(2)
var<storage, read> composite: CompositeData;

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    // mesh.uv goes (0,0) top-left → (1,1) bottom-right of the canvas quad
    let canvas_px = mesh.uv * composite.canvas_size;
    var result = vec4<f32>(0.0, 0.0, 0.0, 0.0);

    for (var i = 0u; i < composite.layer_count; i++) {
        let layer = composite.layers[i];

        // Position of this pixel relative to the layer's top-left corner
        let layer_px = canvas_px - layer.canvas_offset;

        if layer_px.x >= 0.0 && layer_px.y >= 0.0
                && layer_px.x < layer.layer_size.x
                && layer_px.y < layer.layer_size.y {

            let layer_uv = layer.atlas_uv_min
                + (layer_px / layer.layer_size)
                    * (layer.atlas_uv_max - layer.atlas_uv_min);

            let color = textureSample(textures[i], nearest_sampler, layer_uv);

            // Alpha-over composite: each successive layer draws on top
            let a = color.a + result.a * (1.0 - color.a);
            if a > 0.0001 {
                result = vec4<f32>(
                    (color.rgb * color.a + result.rgb * result.a * (1.0 - color.a)) / a,
                    a,
                );
            }
        }
    }

    return result;
}
