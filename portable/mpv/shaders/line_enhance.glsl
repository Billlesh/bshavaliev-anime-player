//!HOOK LUMA
//!BIND HOOKED
//!DESC Anime Player — line enhance
//!WIDTH HOOKED.w
//!HEIGHT HOOKED.h

// Adaptive unsharp on luma edges only. Flats stay flat; thin lines pick up contrast.
// Original preset (Anime4K-style intent, not a copy of Anime4K source).

#define STRENGTH 0.52
#define CLAMP_AMT 0.055
#define GATE_LO 0.018
#define GATE_HI 0.11

vec4 hook() {
    float c  = HOOKED_texOff(vec2( 0.0,  0.0)).x;
    float n  = HOOKED_texOff(vec2( 0.0, -1.0)).x;
    float s  = HOOKED_texOff(vec2( 0.0,  1.0)).x;
    float e  = HOOKED_texOff(vec2( 1.0,  0.0)).x;
    float w  = HOOKED_texOff(vec2(-1.0,  0.0)).x;
    float ne = HOOKED_texOff(vec2( 1.0, -1.0)).x;
    float nw = HOOKED_texOff(vec2(-1.0, -1.0)).x;
    float se = HOOKED_texOff(vec2( 1.0,  1.0)).x;
    float sw = HOOKED_texOff(vec2(-1.0,  1.0)).x;

    float blur = (4.0 * c + 2.0 * (n + s + e + w) + (ne + nw + se + sw)) / 16.0;
    float detail = c - blur;
    float contrast = abs(n - s) + abs(e - w) + 0.5 * (abs(ne - sw) + abs(nw - se));
    float gate = smoothstep(GATE_LO, GATE_HI, contrast);
    float outp = c + detail * STRENGTH * gate;
    outp = clamp(outp, c - CLAMP_AMT, c + CLAMP_AMT);
    return vec4(outp, 0.0, 0.0, 1.0);
}
