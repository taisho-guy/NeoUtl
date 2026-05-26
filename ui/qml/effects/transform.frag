#version 440
layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;
layout(binding = 1) uniform sampler2D source;
layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float translationX;
    float translationY;
    float scale;
    float rotationZ; // ラジアン
    float cx;
    float cy;
    float opacityValue;
    float targetWidth;
    float targetHeight;
    int blendMode;
};

void main() {
    vec2 uv = qt_TexCoord0 - vec2(0.5);

    vec2 pivot = vec2(cx / targetWidth, cy / targetHeight);
    uv -= pivot;

    if (scale > 0.0) {
        uv /= scale;
    }

    float cosAngle = cos(-rotationZ);
    float sinAngle = sin(-rotationZ);
    mat2 rot = mat2(cosAngle, -sinAngle, sinAngle, cosAngle);
    uv = rot * uv;

    uv += pivot;

    vec2 offset = vec2(translationX / targetWidth, translationY / targetHeight);
    uv -= offset;

    uv += vec2(0.5);

    if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) {
        fragColor = vec4(0.0);
    } else {
        fragColor = texture(source, uv) * opacityValue * qt_Opacity;
    }
}
