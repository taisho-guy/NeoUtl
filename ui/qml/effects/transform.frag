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
    // 1. 中心基準の座標系にシフト
    vec2 uv = qt_TexCoord0 - vec2(0.5);

    // 2. 回転・拡大縮小の中心（ピボット）の適用
    vec2 pivot = vec2(cx / targetWidth, -cy / targetHeight);
    uv -= pivot;

    // 3. 拡大縮小の逆変換 (縮小で拡大サンプリング、拡大で縮小サンプリング)
    if (scale > 0.0) {
        uv /= scale;
    }

    // 4. 回転の逆変換
    float cosAngle = cos(-rotationZ);
    float sinAngle = sin(-rotationZ);
    mat2 rot = mat2(cosAngle, -sinAngle, sinAngle, cosAngle);
    uv = rot * uv;

    // 5. ピボットの復元
    uv += pivot;

    // 6. 移動の逆変換 (AviUtl 座標系と Qt の Y 軸方向の補正)
    vec2 offset = vec2(translationX / targetWidth, -translationY / targetHeight);
    uv -= offset;

    // 7. テクスチャ座標系 [0.0 ~ 1.0] に戻す
    uv += vec2(0.5);

    // 8. 変形して画像境界外になった領域は透明にする
    if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) {
        fragColor = vec4(0.0);
    } else {
        fragColor = texture(source, uv) * opacityValue * qt_Opacity;
    }
}
