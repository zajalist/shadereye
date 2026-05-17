void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    float v = sin(uv.x * 10.0 + iTime) + sin(uv.y * 10.0 + iTime);
    fragColor = vec4(0.5 + 0.5 * cos(v + vec3(0.0, 2.0, 4.0)), 1.0);
}
