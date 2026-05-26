#version 440
layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;
layout(binding = 1) uniform sampler2D source;
layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float centerX;
    float centerY;
    float angle;
    float clipWidth;
    float blur;
    float targetWidth;
    float targetHeight;
};

#define PI 3.14159265359

void main() {
    vec2 tc = qt_TexCoord0;
    // ピクセル座標への変換
    vec2 px = tc * vec2(targetWidth, targetHeight);
    
    // オブジェクト中心
    vec2 center = vec2(targetWidth * 0.5, targetHeight * 0.5);
    
    // クリッピング中心（ユーザー指定のオフセットを加算）
    vec2 origin = center + vec2(centerX, centerY);
    
    // 中心からの相対座標
    vec2 delta = px - origin;
    
    // 角度をラジアンに変換
    float rad = radians(angle);
    float c = cos(rad);
    float s = sin(rad);
    
    // 回転後のX座標（ラインからの距離に相当）
    float dist = delta.x * c + delta.y * s;
    
    float alphaFactor = 0.0;
    float blurVal = max(blur, 0.001); // 0除算防止

    if (clipWidth == 0.0) {
        // 通常の斜めクリッピング (片側を切り取る)
        // dist > 0 の領域を表示 (smoothstepでぼかす)
        alphaFactor = smoothstep(-blurVal, 0.0, dist);
    } else if (clipWidth > 0.0) {
        // 幅指定 (正): 中心部分を表示
        // distの絶対値が clipWidth / 2 より小さい場合に表示
        float halfWidth = clipWidth * 0.5;
        alphaFactor = smoothstep(0.0, blurVal, halfWidth - abs(dist));
    } else {
        // 幅指定 (負): 中心部分を切り取る
        // distの絶対値が -clipWidth / 2 より大きい場合に表示
        float halfWidth = -clipWidth * 0.5;
        alphaFactor = smoothstep(0.0, blurVal, abs(dist) - halfWidth);
    }

    fragColor = texture(source, tc) * (qt_Opacity * alphaFactor);
}