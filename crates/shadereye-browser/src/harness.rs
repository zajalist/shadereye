//! Generates the self-contained WebGL2 / GLSL-ES-3.00 harness page that runs a
//! Shadertoy-style shader and reports compile logs + console output.

/// Build an HTML page: a WebGL2 canvas running the Shadertoy body for `frames`
/// frames, logging every shader/program info log and GL error to console, then
/// printing the sentinel `SHADEREYE_DONE`.
pub fn generate_html(user_body: &str, w: u32, h: u32, frames: u32) -> String {
    // The fragment shader: GLSL ES 3.00 with Shadertoy uniforms.
    let frag = format!(
        "#version 300 es\nprecision highp float;\nout vec4 _st;\n\
         uniform vec3 iResolution; uniform float iTime; uniform float iTimeDelta;\n\
         uniform int iFrame; uniform vec4 iMouse;\n{user_body}\n\
         void main(){{ vec4 c; mainImage(c, gl_FragCoord.xy); _st = c; }}"
    );
    let vert = "#version 300 es\nin vec2 p; void main(){ gl_Position = vec4(p,0.0,1.0); }";
    let frag_js = serde_json::to_string(&frag).unwrap();
    let vert_js = serde_json::to_string(&vert).unwrap();
    format!(
        r#"<!doctype html><html><head><meta charset="utf-8"></head><body>
<canvas id="c" width="{w}" height="{h}"></canvas>
<script>
(function(){{
  function fail(msg){{ console.error("SHADEREYE_GL_ERROR: "+msg); console.log("SHADEREYE_DONE"); }}
  var cv=document.getElementById('c');
  var gl=cv.getContext('webgl2');
  if(!gl){{ fail("webgl2 unavailable"); return; }}
  var vsrc={vert_js}, fsrc={frag_js};
  function sh(type,src){{
    var s=gl.createShader(type); gl.shaderSource(s,src); gl.compileShader(s);
    var log=gl.getShaderInfoLog(s);
    if(log) console.warn("SHADEREYE_SHADER_LOG["+(type===gl.VERTEX_SHADER?"vert":"frag")+"]: "+log);
    if(!gl.getShaderParameter(s,gl.COMPILE_STATUS)){{ fail("shader compile failed"); return null; }}
    return s;
  }}
  var vs=sh(gl.VERTEX_SHADER,vsrc); if(!vs) return;
  var fs=sh(gl.FRAGMENT_SHADER,fsrc); if(!fs) return;
  var pr=gl.createProgram(); gl.attachShader(pr,vs); gl.attachShader(pr,fs); gl.linkProgram(pr);
  var plog=gl.getProgramInfoLog(pr); if(plog) console.warn("SHADEREYE_PROGRAM_LOG: "+plog);
  if(!gl.getProgramParameter(pr,gl.LINK_STATUS)){{ fail("program link failed"); return; }}
  gl.useProgram(pr);
  var buf=gl.createBuffer(); gl.bindBuffer(gl.ARRAY_BUFFER,buf);
  gl.bufferData(gl.ARRAY_BUFFER,new Float32Array([-1,-1,3,-1,-1,3]),gl.STATIC_DRAW);
  var loc=gl.getAttribLocation(pr,"p"); gl.enableVertexAttribArray(loc);
  gl.vertexAttribPointer(loc,2,gl.FLOAT,false,0,0);
  var uRes=gl.getUniformLocation(pr,"iResolution"), uT=gl.getUniformLocation(pr,"iTime"),
      uF=gl.getUniformLocation(pr,"iFrame"), uM=gl.getUniformLocation(pr,"iMouse"),
      uD=gl.getUniformLocation(pr,"iTimeDelta");
  var n=0;
  function frame(){{
    gl.uniform3f(uRes,{w}.0,{h}.0,1.0); gl.uniform1f(uT,n/60.0);
    gl.uniform1f(uD,1.0/60.0); gl.uniform1i(uF,n); gl.uniform4f(uM,0,0,0,0);
    gl.viewport(0,0,{w},{h}); gl.drawArrays(gl.TRIANGLES,0,3);
    var e=gl.getError(); if(e!==gl.NO_ERROR) console.error("SHADEREYE_GL_ERROR: code "+e);
    n++;
    if(n<{frames}) requestAnimationFrame(frame);
    else {{ console.log("SHADEREYE_DONE"); }}
  }}
  requestAnimationFrame(frame);
}})();
</script></body></html>"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_embeds_user_body_and_reporters() {
        let html = generate_html(
            "void mainImage(out vec4 o, in vec2 fc){ o=vec4(1.0); }",
            32,
            32,
            3,
        );
        assert!(html.contains("<canvas"));
        assert!(html.contains("getShaderInfoLog"));
        assert!(html.contains("getProgramInfoLog"));
        assert!(html.contains("mainImage"));
        assert!(html.contains("SHADEREYE_DONE")); // completion sentinel for the driver
        assert!(html.contains("300 es")); // GLSL ES 3.00
    }
}
