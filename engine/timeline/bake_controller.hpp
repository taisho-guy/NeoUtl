#pragma once
#include <QObject>

namespace AviQtl::Engine::Timeline {

class BakeController : public QObject {
    Q_OBJECT
  public:
    static BakeController &instance();

    // 指定されたシーンおよびフレームに基づいて、ECSへのベイクを実行する
    void bake(int sceneId, int currentFrame);

    // DocumentModelのstructureChangedシグナルハンドラ
    // 構造変化があった場合、直近のシーン・フレームで強制的に再ベイクを行う
    void triggerRebake();

  private slots:
    void onStructureChanged();

  private:
    BakeController();
    ~BakeController() override = default;

    BakeController(const BakeController &) = delete;
    BakeController &operator=(const BakeController &) = delete;

    int m_lastSceneId = -1;
    int m_lastFrame = -1;
};

} // namespace AviQtl::Engine::Timeline
