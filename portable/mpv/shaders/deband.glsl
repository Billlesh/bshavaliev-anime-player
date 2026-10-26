//!HOOK LUMA
//!BIND HOOKED
//!DESC Anime Player — light deband
//!WIDTH HOOKED.w
//!HEIGHT HOOKED.h

// Average toward neighbors only when the delta is below a banding threshold,
// then add 8-bit-scale dither. Light on purpose: lines stay sharp.

#define THRESHOLD 0.028
#define RANGE 14.0
#define ITERATIONS 2

float hash21(vec2 p) {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
}

vec4 hook() {
    vec2 uv = HOOKED_pos;
    float src = HOOKED_tex(uv).x;
    float acc = src;
    float wsum = 1.0;

    for (int i = 1; i <= ITERATIONS; i++) {
        float radius = RANGE * float(i);
        float ang = hash21(uv + float(i) * 1.37) * 6.2831853;
        vec2 dir = vec2(cos(ang), sin(ang));
        vec2 o1 = dir * radius * HOOKED_pt;
        vec2 o2 = vec2(-dir.y, dir.x) * radius * HOOKED_pt;
        float avg = 0.25 * (
            HOOKED_tex(uv + o1).x +
            HOOKED_tex(uv - o1).x +
            HOOKED_tex(uv + o2).x +
            HOOKED_tex(uv - o2).x
        );
        if (abs(avg - src) < THRESHOLD * float(i)) {
            acc += avg;
            wsum += 1.0;
        }
    }

    float dither = (hash21(uv * vec2(HOOKED_size)) - 0.5) * (1.5 / 255.0);
    return vec4(acc / wsum + dither, 0.0, 0.0, 1.0);
}
