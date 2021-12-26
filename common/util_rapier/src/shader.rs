// Contains reusable shader settings that can be used to render meshes.

pub const VERTEX_SHADER: &str = r"
#version 450
layout(location = 0) in vec2 Vertex_Position;
layout(location = 1) in vec4 Vertex_Color;
layout(location = 1) out vec4 v_Color;
layout(set = 0, binding = 0) uniform CameraViewProj {
    mat4 ViewProj;
};
layout(set = 1, binding = 0) uniform Transform {
    mat4 Model;
};
void main() {
    v_Color = Vertex_Color;
    gl_Position = ViewProj * Model * vec4(Vertex_Position, 0.0, 1.0);
}
";

pub const FRAGMENT_SHADER: &str = r"
#version 450
layout(location = 1) in vec4 v_Color;
layout(location = 0) out vec4 o_Target;
void main() {
    o_Target = v_Color;
}
";
