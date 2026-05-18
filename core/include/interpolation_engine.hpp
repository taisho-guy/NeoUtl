#pragma once
#include "document_model.hpp"
#include <functional>
#include <memory>
#include <string>
#include <unordered_map>
#include <vector>

namespace AviQtl::Core {

// ─── AST / 数式パーサ (Custom Expression用) ───
// 再生中の毎フレーム評価コストをゼロにするための簡易ASTプリコンパイラ
class ExpressionNode {
  public:
    virtual ~ExpressionNode() = default;
    virtual float evaluate(float t, float baseVal) const = 0;
};

class InterpolationEngine {
  public:
    static InterpolationEngine &instance();

    // 与えられたキーフレーム群とフレーム番号から補間値を計算する (純粋C++ / スカラー値専用)
    float evaluate(const std::vector<Keyframe> &keyframes, int frame, float fallback) const;

    // カスタム数式 (expression) をASTノードにプリコンパイルする
    std::shared_ptr<ExpressionNode> compileExpression(const std::string &expression) const;

  private:
    InterpolationEngine();
    ~InterpolationEngine() = default;

    InterpolationEngine(const InterpolationEngine &) = delete;
    InterpolationEngine &operator=(const InterpolationEngine &) = delete;

    // 3次ベジェのX座標からTパラメータを求める (ニュートン法)
    static float solveBezierT(float x, float x1, float x2);

    // 各種イージング関数のテーブル
    using EasingFunc = std::function<float(float, const std::vector<float> &)>;
    std::unordered_map<std::string, EasingFunc> m_easings;
};

} // namespace AviQtl::Core
