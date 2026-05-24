#pragma once
#include <QList>
#include <QObject>
#include <QString>
#include <QUndoStack>
#include <QVariantMap>
#include <map>
#include <memory>
#include <vector>

namespace AviQtl::Core {

// ─── Keyframe ───
struct Keyframe {
    int frame = 0;
    float value = 0.0f;
    QString interpolation = QStringLiteral("linear"); // "linear", "bezier", "custom"

    // ベジェ補間用パラメータ (x1, y1, x2, y2)
    float bzx1 = 0.33f;
    float bzy1 = 0.0f;
    float bzx2 = 0.66f;
    float bzy2 = 1.0f;

    // カスタム数式 (expression)
    QString expression;
};

// ─── Effect ───
struct Effect {
    QString id; // プラグインID (例: "border_blur")
    bool enabled = true;
    QVariantMap params;                                 // 静的パラメータ
    std::map<QString, std::vector<Keyframe>> keyframes; // パラメータ名 -> キーフレーム配列
};

// ─── Clip ───
struct Clip {
    int id = -1;
    int sceneId = 0;
    QString type; // "video" | "image" | "text" | "rect" | "audio" | "scene"
    int layer = 0;
    int startFrame = 0;
    int durationFrames = 0;

    QVariantMap params; // 各クリップ固有の静的パラメータ（ファイルパスなど）
    std::vector<Effect> effects;
};

// ─── SceneSettings ───
struct SceneSettings {
    int id = 0;
    QString name;
    int width = 1920;
    int height = 1080;
    double fps = 60.0;

    bool enableSnap = true;
    QString gridMode = QStringLiteral("Auto");
    std::vector<int> lockedLayers;
    std::vector<int> hiddenLayers;

    std::vector<Clip> clips;
};

// ─── ProjectSettings ───
struct ProjectSettings {
    QString name;
    int defaultSceneWidth = 1920;
    int defaultSceneHeight = 1080;
    double defaultFps = 60.0;
    int audioSampleRate = 48000;
    QString colorSpace = QStringLiteral("BT.709");
};

// ─── DocumentModel ───
// プロジェクト全体の全データを木構造で保持する唯一の正本
class DocumentModel : public QObject {
    Q_OBJECT
  public:
    static DocumentModel &instance();

    void clear();

    // プロジェクト設定
    const ProjectSettings &projectSettings() const { return m_projectSettings; }
    void setProjectSettings(const ProjectSettings &settings);

    // シーン操作
    const std::vector<SceneSettings> &scenes() const { return m_scenes; }
    const SceneSettings *findScene(int sceneId) const;
    void addScene(const SceneSettings &scene);
    void removeScene(int sceneId);

    void updateSceneSettings(const SceneSettings &settings);
    void setClips(int sceneId, std::vector<Clip> &&clips);

    // クリップ操作
    const Clip *findClip(int sceneId, int clipId) const;
    void addClip(int sceneId, const Clip &clip);
    void removeClip(int sceneId, int clipId);

    // Undo / Redo スタックの提供
    QUndoStack *undoStack() { return &m_undoStack; }

  signals:
    // 構造変化が発生し、ECSへのBake（焼き付け）が必要になった時に発火する
    void structureChanged();

  private:
    DocumentModel() = default;
    ~DocumentModel() override = default;

    DocumentModel(const DocumentModel &) = delete;
    DocumentModel &operator=(const DocumentModel &) = delete;

    ProjectSettings m_projectSettings;
    std::vector<SceneSettings> m_scenes;
    QUndoStack m_undoStack;
};

} // namespace AviQtl::Core
