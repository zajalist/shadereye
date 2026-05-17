void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    float t = 1 / 2;          // BUG: integer division -> 0.0
    fragColor = vec4(uv, t, 1.0);
}
