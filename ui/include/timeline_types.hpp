#pragma once
#include "effect_model.hpp"
#include <QList>
#include <QSet>
#include <QString>
#include <QVariant>
#include <bitset>

namespace AviQtl::UI {

// 最適化: ClipIDを導入
using ClipID = int;

class EffectModel; // 前方宣言

struct Keyframe {
    int frame;
    float value;
    int interpolationType; // 0: 線形
};

struct AudioPluginState {
    QString id;
    bool enabled = true;
    QVariantMap params;
};

struct ClipData {
    int id;
    int sceneId = 0;
    QString type;
    int startFrame;
    int durationFrames;
    int layer;

    // 最適化: レンダリングパスでの文字列比較を避けるためのキャッシュ
    mutable bool isSceneObject = false;
    mutable bool isSceneIdCached = false;

    // ハイブリッド設計: EffectModelは振る舞いを持つためポインタで保持する
    QList<EffectModel *> effects;
    QList<AudioPluginState> audioPlugins;
    QVariantMap params; // 各クリップ固有のパラメータ（ファイルパス等）
};

struct SceneData {
    int id;
    QString name;
    QList<ClipData> clips;

    // レイヤー状態
    QSet<int> lockedLayers;
    QSet<int> hiddenLayers;

    // シーンのコンテキスト（自己完結化）
    int width = 1920;
    int height = 1080;
    double fps = 60.0;
    int totalFrames = 300;

    // ネスト利用のためのメタデータ
    int startFrame = 0;
    int durationFrames = 0;

    // Grid & Snap Settings (Moved from UI/System state to Scene state)
    QString gridMode = QStringLiteral("Auto"); // QStringLiteral("Auto"), QStringLiteral("BPM"), QStringLiteral("Frame")
    double gridBpm = 120.0;
    double gridOffset = 0.0;
    int gridInterval = 10;
    int gridSubdivision = 4;
    bool enableSnap = true;
    int magneticSnapRange = 10;
};
} // namespace AviQtl::UI