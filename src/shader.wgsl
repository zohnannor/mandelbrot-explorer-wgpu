struct Uniforms {
    resolution: vec2<f64>,
    time: f64,
    zooms: f64,
    offset: vec2<f64>,
    mouse_position: vec2<f64>,
    is_mandelbrot: f32,
    rotate_colors: f32,
    max_iter: u32,
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

struct Interpolators {
    @builtin(position) pos: vec4<f32>,
    @location(0) resolution: vec2<f64>,
    @location(1) time: f64,
    @location(2) zoom: f64,
    @location(3) offset: vec2<f64>,
    @location(4) mouse_position: vec2<f64>,
    @location(5) is_mandelbrot: f32,
    @location(6) rotate_colors: f32,
    @location(7) max_iter: u32,
}

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
) -> Interpolators {
    let vert = array(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
    );
    return Interpolators(
        vec4<f32>(vert[vertex_index] * 4 - 1, 0.0, 1.0),
        uniforms.resolution,
        uniforms.time,
        exp(uniforms.zooms / 10.0),
        uniforms.offset,
        uniforms.mouse_position,
        uniforms.is_mandelbrot,
        uniforms.rotate_colors,
        uniforms.max_iter,
    );
}

@fragment
fn fg_main(i: Interpolators) -> @location(0) vec4f {
    let res = i.resolution;
    let time = i.time;
    let offset = i.offset;
    let is_mandelbrot = i.is_mandelbrot == 1.0;
    let rotate_colors = i.rotate_colors == 1.0;
    let max_iter = i.max_iter;
    let zoom = i.zoom;
    let pos = i.pos;

    let px = vec2<f64>(pos.xy);
    let uv = vec2<f64>(px.x, res.y - px.y);
    let p = (uv * 2.0 - res) / res.x;


    let c_zoom = select(f64(2.5), zoom, is_mandelbrot);
    // let c = p * automatic_zoom(time);
    let c = p * c_zoom;
    let mouse_position = (i.mouse_position) * vec2<f64>(1.0, -1.0) * zoom;

    // let iters = mandelbrot(vec2<f64>(c) + vec2<f64>(-1.253441321, 0.38469378));
    // let iters = mandelbrot(vec2<f64>(c) + vec2<f64>(-1.768778837, 0.001738939));
    // let iters = mandelbrot(vec2<f64>(c) + vec2<f64>(-0.3435595, -0.610793536));
    // let iters = mandelbrot(vec2<f64>(c) + vec2<f64>(-1.940157343, 0.00000008));

    var col = vec3<f32>(0.0);
    let iters = mandelbrot(c, offset, mouse_position, is_mandelbrot, max_iter);
    let rot = select(f32(1.0), f32(time), rotate_colors);
    if iters > 0.5 {
        col = 0.5 + 0.5 * cos(3.0 + f32(iters) * 0.15 * 0.5 + vec3f(0.0, 0.6, 1.0) * rot * 8);
    }
    return vec4f(col, 1.0);
}

fn automatic_zoom(t: f32) -> f32 {
    return pow(0.67 + 0.5 * cos(0.21 * t), 8.0);
}

fn mandelbrot(c: vec2<f64>, offset: vec2<f64>, mouse_position: vec2<f64>, is_mandelbrot: bool, max_iter: u32) -> f64 {
    let z0 = select(c, vec2<f64>(0), is_mandelbrot);
    let c0 = select(mouse_position + offset, c + offset, is_mandelbrot);

    if is_mandelbrot {
        // Cardioid and circle optimization
        let c2 = dot(c0, c0);
        if 256.0 * c2 * c2 - 96.0 * c2 + 32.0 * c0.x - 3.0 < 0.0 { return 0.0; }
        if 16.0 * (c2 + 2.0 * c0.x + 1.0) - 1.0 < 0.0 { return 0.0; }
    }

    return mandelbrot_inner(z0, c0, max_iter);
}

fn mandelbrot_inner(z0: vec2<f64>, c: vec2<f64>, max_iter: u32) -> f64 {
    var iter: f64 = 0.0;
    var z = z0;
    var dotz: f64;
    for (var i = 0u; i < max_iter; i++) {
        z = vec2<f64>(
            z.x * z.x - z.y * z.y,
            2.0 * z.x * z.y
        ) + c;
        dotz = dot(z, z);
        if u32(dotz) > max_iter {
            return iter - f64(log2(log2(f32(dotz)))) + 4.0;
        }
        iter += 1.0;
    }
    return 0.0;
}
