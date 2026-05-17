// Animated swirl plasma.
//
// This shader carries an intentional GLSL gotcha used in the shadereye demo:
// `1 / 2` is integer division and evaluates to 0, which collapses every wave
// term to a flat colour wash. The fix is `1.0 / 2.0` (float division).
// See docs / README for the compile-see-fix walkthrough.
void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = (fragCoord - 0.5 * iResolution.xy) / iResolution.y;
    float t = iTime;

    // BUG: integer division yields 0, flattening every wave term below.
    float amp = 1 / 2;            // intended 0.5  (fix: 1.0 / 2.0)

    float a = atan(uv.y, uv.x);
    float r = length(uv);

    float v = 0.0;
    v += sin(r * 10.0 - t * 1.5);
    v += sin(a * 5.0 + t);
    v += sin((uv.x + uv.y) * 8.0 + t * 1.3);
    v += sin(r * 18.0 - t * 2.0) * 0.5;
    v *= amp;                     // amp == 0 -> v == 0 -> flat wash (the bug)

    vec3 col = 0.5 + 0.5 * cos(v * 3.14159 + vec3(0.0, 2.1, 4.2) + t * 0.3);
    col *= 0.55 + 0.45 * smoothstep(1.15, 0.05, r);   // soft vignette
    fragColor = vec4(col, 1.0);
}
