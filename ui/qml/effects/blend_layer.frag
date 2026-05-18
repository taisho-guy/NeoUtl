#version 440
layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(binding = 1) uniform sampler2D source;      // 前景 (現在のレイヤー)
layout(binding = 2) uniform sampler2D background;  // 背景 (前の合成結果)

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    int blendMode;
    float opacityValue;
};

// 🚀 高精度ブレンド演算
vec3 blendMultiply(vec3 base, vec3 blend) {
    return base * blend;
}

vec3 blendScreen(vec3 base, vec3 blend) {
    return 1.0 - (1.0 - base) * (1.0 - blend);
}

vec3 blendOverlay(vec3 base, vec3 blend) {
    return mix(2.0 * base * blend, 1.0 - 2.0 * (1.0 - base) * (1.0 - blend), step(0.5, base));
}

vec3 blendAdd(vec3 base, vec3 blend) {
    return min(base + blend, vec3(1.0));
}

vec3 blendSubtract(vec3 base, vec3 blend) {
    return max(base - blend, vec3(0.0));
}

vec3 blendLighten(vec3 base, vec3 blend) {
    return max(base, blend);
}

vec3 blendDarken(vec3 base, vec3 blend) {
    return min(base, blend);
}

void main() {
    vec4 fg = texture(source, qt_TexCoord0);
    vec4 bg = texture(background, qt_TexCoord0);

    // 不透明度の適用
    float activeOpacity = opacityValue * qt_Opacity;
    fg.a *= activeOpacity;
    fg.rgb *= fg.a; // ストレートアルファから事前乗算アルファ (Premultiplied Alpha) へ

    // 前景が完全に透明なら背景をそのまま返す
    if (fg.a <= 0.0) {
        fragColor = bg;
        return;
    }

    vec3 blendedColor = fg.rgb;
    
    // 合成モードの判定
    if (blendMode == 1) {        // スクリーン
        blendedColor = blendScreen(bg.rgb, fg.rgb);
    } else if (blendMode == 2) { // 乗算
        blendedColor = blendMultiply(bg.rgb, fg.rgb);
    } else if (blendMode == 3) { // オーバーレイ
        blendedColor = blendOverlay(bg.rgb, fg.rgb);
    } else if (blendMode == 4) { // 加算
        blendedColor = blendAdd(bg.rgb, fg.rgb);
    } else if (blendMode == 5) { // 減算
        blendedColor = blendSubtract(bg.rgb, fg.rgb);
    } else if (blendMode == 6) { // 比較明 (Lighten)
        blendedColor = blendLighten(bg.rgb, fg.rgb);
    } else if (blendMode == 7) { // 比較暗 (Darken)
        blendedColor = blendDarken(bg.rgb, fg.rgb);
    }

    // 🚀 アルファブレンディング公式 (アルファ・RGBの正確な重ね合わせ)
    vec4 result;
    result.a = fg.a + bg.a * (1.0 - fg.a);
    if (result.a > 0.0) {
        result.rgb = mix(bg.rgb, blendedColor, fg.a);
    } else {
        result.rgb = vec3(0.0);
    }

    fragColor = result;
}
