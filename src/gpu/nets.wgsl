// One decision step of many networks at once. One workgroup per row (a tank in
// a match); `row_param[row]` selects which parameter set (candidate) it uses.
// Weight matrices are stored transposed (column-major) so that neighbouring
// threads read neighbouring addresses.

const N_IN: u32 = 34u;
const N_OUT: u32 = 5u;

struct Meta {
    stride: u32,
    hidden: u32,
    n_layers: u32,
    _pad: u32,
    // MLP: dims[l] -> dims[l + 1] with weights at w_off[l], biases at b_off[l].
    // Elman: w_off = [wx, wh, wo], b_off = [b, bo].
    // GRU:   w_off = [wx_z, wx_r, wx_n, wo], b_off = [b_z, b_r, b_n, bo], u_off = [wh_z, wh_r, wh_n].
    dims: vec4<u32>,
    dims_last: vec4<u32>,
    w_off: vec4<u32>,
    b_off: vec4<u32>,
    u_off: vec4<u32>,
}

@group(0) @binding(0) var<storage, read> params: array<f32>;
@group(0) @binding(1) var<storage, read> obs: array<f32>;
@group(0) @binding(2) var<storage, read_write> hidden: array<f32>;
@group(0) @binding(3) var<storage, read_write> outs: array<f32>;
@group(0) @binding(4) var<storage, read> row_param: array<u32>;
@group(0) @binding(5) var<uniform> m: Meta;

var<workgroup> a: array<f32, 64>;
var<workgroup> b: array<f32, 64>;
var<workgroup> c: array<f32, 64>;

fn stanh(x: f32) -> f32 {
    return tanh(clamp(x, -15.0, 15.0));
}

fn sigm(x: f32) -> f32 {
    return 1.0 / (1.0 + exp(-clamp(x, -30.0, 30.0)));
}

fn dim(l: u32) -> u32 {
    if (l < 4u) {
        return m.dims[l];
    }
    return m.dims_last[l - 4u];
}

@compute @workgroup_size(64)
fn mlp(@builtin(workgroup_id) wg: vec3<u32>, @builtin(local_invocation_index) j: u32) {
    let row = wg.x;
    let base = row_param[row] * m.stride;
    if (j < N_IN) {
        a[j] = obs[row * N_IN + j];
    }
    workgroupBarrier();
    for (var l = 0u; l < m.n_layers; l++) {
        let n_in = dim(l);
        let n_out = dim(l + 1u);
        var acc = 0.0;
        if (j < n_out) {
            let w = base + m.w_off[l];
            acc = params[base + m.b_off[l] + j];
            for (var k = 0u; k < n_in; k++) {
                acc += params[w + k * n_out + j] * a[k];
            }
        }
        workgroupBarrier();
        if (j < n_out) {
            a[j] = stanh(acc);
        }
        workgroupBarrier();
    }
    if (j < N_OUT) {
        outs[row * N_OUT + j] = a[j];
    }
}

@compute @workgroup_size(64)
fn elman(@builtin(workgroup_id) wg: vec3<u32>, @builtin(local_invocation_index) j: u32) {
    let row = wg.x;
    let base = row_param[row] * m.stride;
    let h = m.hidden;
    if (j < N_IN) {
        a[j] = obs[row * N_IN + j];
    }
    if (j < h) {
        b[j] = hidden[row * h + j];
    }
    workgroupBarrier();
    var acc = 0.0;
    if (j < h) {
        acc = params[base + m.b_off[0] + j];
        for (var k = 0u; k < N_IN; k++) {
            acc += params[base + m.w_off[0] + k * h + j] * a[k];
        }
        for (var k = 0u; k < h; k++) {
            acc += params[base + m.w_off[1] + k * h + j] * b[k];
        }
    }
    workgroupBarrier();
    if (j < h) {
        let v = stanh(acc);
        c[j] = v;
        hidden[row * h + j] = v;
    }
    workgroupBarrier();
    if (j < N_OUT) {
        var o = params[base + m.b_off[1] + j];
        for (var k = 0u; k < h; k++) {
            o += params[base + m.w_off[2] + k * N_OUT + j] * c[k];
        }
        outs[row * N_OUT + j] = stanh(o);
    }
}

@compute @workgroup_size(64)
fn gru(@builtin(workgroup_id) wg: vec3<u32>, @builtin(local_invocation_index) j: u32) {
    let row = wg.x;
    let base = row_param[row] * m.stride;
    let h = m.hidden;
    if (j < N_IN) {
        a[j] = obs[row * N_IN + j];
    }
    if (j < h) {
        b[j] = hidden[row * h + j];
    }
    workgroupBarrier();
    var z = 0.0;
    var r = 0.0;
    var n = 0.0;
    if (j < h) {
        z = params[base + m.b_off[0] + j];
        r = params[base + m.b_off[1] + j];
        n = params[base + m.b_off[2] + j];
        for (var k = 0u; k < N_IN; k++) {
            let x = a[k];
            z += params[base + m.w_off[0] + k * h + j] * x;
            r += params[base + m.w_off[1] + k * h + j] * x;
            n += params[base + m.w_off[2] + k * h + j] * x;
        }
        for (var k = 0u; k < h; k++) {
            let hv = b[k];
            z += params[base + m.u_off[0] + k * h + j] * hv;
            r += params[base + m.u_off[1] + k * h + j] * hv;
        }
        z = sigm(z);
        r = sigm(r);
        c[j] = r * b[j];
    }
    workgroupBarrier();
    var hn = 0.0;
    if (j < h) {
        for (var k = 0u; k < h; k++) {
            n += params[base + m.u_off[2] + k * h + j] * c[k];
        }
        hn = (1.0 - z) * stanh(n) + z * b[j];
        hidden[row * h + j] = hn;
    }
    workgroupBarrier();
    if (j < h) {
        a[j] = hn;
    }
    workgroupBarrier();
    if (j < N_OUT) {
        var o = params[base + m.b_off[3] + j];
        for (var k = 0u; k < h; k++) {
            o += params[base + m.w_off[3] + k * N_OUT + j] * a[k];
        }
        outs[row * N_OUT + j] = stanh(o);
    }
}
