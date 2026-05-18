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

// 🚀 標準ブレンド演算
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

// 🚀 高度なデジタル合成モード

// 8: 色反転 (Invert)
vec3 blendInvert(vec3 base, vec3 blend) {
    return 1.0 - base;
}

// 9: ソフトライト (Soft Light)
float blendSoftLight(float base, float blend) {
    return (blend <= 0.5) ? (base - (1.0 - 2.0 * blend) * base * (1.0 - base)) :
           (((base <= 0.25) ? (((16.0 * base - 12.0) * base + 4.0) * base) : sqrt(base)) - base) * (2.0 * blend - 1.0) + base;
}
vec3 blendSoftLight(vec3 base, vec3 blend) {
    return vec3(blendSoftLight(base.r, blend.r), blendSoftLight(base.g, blend.g), blendSoftLight(base.b, blend.b));
}

// 10: ハードライト (Hard Light)
float blendHardLight(float base, float blend) {
    return (blend < 0.5) ? (2.0 * base * blend) : (1.0 - 2.0 * (1.0 - base) * (1.0 - blend));
}
vec3 blendHardLight(vec3 base, vec3 blend) {
    return vec3(blendHardLight(base.r, blend.r), blendHardLight(base.g, blend.g), blendHardLight(base.b, blend.b));
}

// 11: 差の絶対値 (Difference)
vec3 blendDifference(vec3 base, vec3 blend) {
    return abs(base - blend);
}

// 🚀 W3C/Photoshop HSL (色相・彩度・カラー・輝度) ブレンド演算

float getLuminosity(vec3 c) {
    return 0.3 * c.r + 0.59 * c.g + 0.11 * c.b;
}

vec3 setLuminosity(vec3 c, float l) {
    float d = l - getLuminosity(c);
    vec3 res = c + vec3(d);
    float lMin = min(min(res.r, res.g), res.b);
    float lMax = max(max(res.r, res.g), res.b);
    if (lMin < 0.0) {
        res = l + ((res - l) * l) / (l - lMin);
    }
    if (lMax > 1.0) {
        res = l + ((res - l) * (1.0 - l)) / (lMax - l);
    }
    return res;
}

float getSaturation(vec3 c) {
    return max(max(c.r, c.g), c.b) - min(min(c.r, c.g), c.b);
}

void setSaturationMinMax(inout float cMin, inout float cMid, inout float cMax, float s) {
    if (cMax > cMin) {
        cMid = (((cMid - cMin) * s) / (cMax - cMin));
        cMax = s;
    } else {
        cMid = 0.0;
        cMax = 0.0;
    }
    cMin = 0.0;
}

vec3 setSaturation(vec3 c, float s) {
    vec3 res = c;
    if (res.r <= res.g) {
        if (res.g <= res.b) {
            setSaturationMinMax(res.r, res.g, res.b, s);
        } else if (res.r <= res.b) {
            setSaturationMinMax(res.r, res.b, res.g, s);
        } else {
            setSaturationMinMax(res.b, res.r, res.g, s);
        }
    } else {
        if (res.r <= res.b) {
            setSaturationMinMax(res.g, res.r, res.b, s);
        } else if (res.g <= res.b) {
            setSaturationMinMax(res.g, res.b, res.r, s);
        } else {
            setSaturationMinMax(res.b, res.g, res.r, s);
        }
    }
    return res;
}

// 12: 色相 (Hue)
vec3 blendHue(vec3 base, vec3 blend) {
    return setLuminosity(setSaturation(blend, getSaturation(base)), getLuminosity(base));
}

// 13: 彩度 (Saturation)
vec3 blendSaturation(vec3 base, vec3 blend) {
    return setLuminosity(setSaturation(base, getSaturation(blend)), getLuminosity(base));
}

// 14: カラー (Color)
vec3 blendColor(vec3 base, vec3 blend) {
    return setLuminosity(blend, getLuminosity(base));
}

// 15: 輝度 (Luminosity)
vec3 blendLuminosity(vec3 base, vec3 blend) {
    return setLuminosity(base, getLuminosity(blend));
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
    } else if (blendMode == 8) { // 色反転
        blendedColor = blendInvert(bg.rgb, fg.rgb);
    } else if (blendMode == 9) { // ソフトライト
        blendedColor = blendSoftLight(bg.rgb, fg.rgb);
    } else if (blendMode == 10) {// ハードライト
        blendedColor = blendHardLight(bg.rgb, fg.rgb);
    } else if (blendMode == 11) {// 差の絶対値
        blendedColor = blendDifference(bg.rgb, fg.rgb);
    } else if (blendMode == 12) {// 色相
        blendedColor = blendHue(bg.rgb, fg.rgb);
    } else if (blendMode == 13) {// 彩度
        blendedColor = blendSaturation(bg.rgb, fg.rgb);
    } else if (blendMode == 14) {// カラー
        blendedColor = blendColor(bg.rgb, fg.rgb);
    } else if (blendMode == 15) {// 輝度
        blendedColor = blendLuminosity(bg.rgb, fg.rgb);
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
