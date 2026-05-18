#include "interpolation_engine.hpp"
#include <algorithm>
#include <cctype>
#include <cmath>
#include <stdexcept>
#include <vector>

namespace AviQtl::Core {

// ─── ASTノード定義の実装 ───

class NumberNode : public ExpressionNode {
    float m_val;

  public:
    explicit NumberNode(float val) : m_val(val) {}
    float evaluate(float, float) const override { return m_val; }
};

class VariableTNode : public ExpressionNode {
  public:
    float evaluate(float t, float) const override { return t; }
};

class VariableBaseNode : public ExpressionNode {
  public:
    float evaluate(float, float baseVal) const override { return baseVal; }
};

class AddNode : public ExpressionNode {
    std::shared_ptr<ExpressionNode> m_left, m_right;

  public:
    AddNode(std::shared_ptr<ExpressionNode> l, std::shared_ptr<ExpressionNode> r) : m_left(l), m_right(r) {}
    float evaluate(float t, float b) const override { return m_left->evaluate(t, b) + m_right->evaluate(t, b); }
};

class SubNode : public ExpressionNode {
    std::shared_ptr<ExpressionNode> m_left, m_right;

  public:
    SubNode(std::shared_ptr<ExpressionNode> l, std::shared_ptr<ExpressionNode> r) : m_left(l), m_right(r) {}
    float evaluate(float t, float b) const override { return m_left->evaluate(t, b) - m_right->evaluate(t, b); }
};

class MulNode : public ExpressionNode {
    std::shared_ptr<ExpressionNode> m_left, m_right;

  public:
    MulNode(std::shared_ptr<ExpressionNode> l, std::shared_ptr<ExpressionNode> r) : m_left(l), m_right(r) {}
    float evaluate(float t, float b) const override { return m_left->evaluate(t, b) * m_right->evaluate(t, b); }
};

class DivNode : public ExpressionNode {
    std::shared_ptr<ExpressionNode> m_left, m_right;

  public:
    DivNode(std::shared_ptr<ExpressionNode> l, std::shared_ptr<ExpressionNode> r) : m_left(l), m_right(r) {}
    float evaluate(float t, float b) const override {
        float denom = m_right->evaluate(t, b);
        return (denom != 0.0f) ? m_left->evaluate(t, b) / denom : 0.0f;
    }
};

class NegateNode : public ExpressionNode {
    std::shared_ptr<ExpressionNode> m_node;

  public:
    explicit NegateNode(std::shared_ptr<ExpressionNode> n) : m_node(n) {}
    float evaluate(float t, float b) const override { return -m_node->evaluate(t, b); }
};

class PowNode : public ExpressionNode {
    std::shared_ptr<ExpressionNode> m_base, m_exp;

  public:
    PowNode(std::shared_ptr<ExpressionNode> b, std::shared_ptr<ExpressionNode> e) : m_base(b), m_exp(e) {}
    float evaluate(float t, float b) const override { return std::pow(m_base->evaluate(t, b), m_exp->evaluate(t, b)); }
};

class SinNode : public ExpressionNode {
    std::shared_ptr<ExpressionNode> m_node;

  public:
    explicit SinNode(std::shared_ptr<ExpressionNode> n) : m_node(n) {}
    float evaluate(float t, float b) const override { return std::sin(m_node->evaluate(t, b)); }
};

class CosNode : public ExpressionNode {
    std::shared_ptr<ExpressionNode> m_node;

  public:
    explicit CosNode(std::shared_ptr<ExpressionNode> n) : m_node(n) {}
    float evaluate(float t, float b) const override { return std::cos(m_node->evaluate(t, b)); }
};

class SqrtNode : public ExpressionNode {
    std::shared_ptr<ExpressionNode> m_node;

  public:
    explicit SqrtNode(std::shared_ptr<ExpressionNode> n) : m_node(n) {}
    float evaluate(float t, float b) const override {
        float val = m_node->evaluate(t, b);
        return (val >= 0.0f) ? std::sqrt(val) : 0.0f;
    }
};

class AbsNode : public ExpressionNode {
    std::shared_ptr<ExpressionNode> m_node;

  public:
    explicit AbsNode(std::shared_ptr<ExpressionNode> n) : m_node(n) {}
    float evaluate(float t, float b) const override { return std::abs(m_node->evaluate(t, b)); }
};

class ClampNode : public ExpressionNode {
    std::shared_ptr<ExpressionNode> m_val, m_min, m_max;

  public:
    ClampNode(std::shared_ptr<ExpressionNode> v, std::shared_ptr<ExpressionNode> minN, std::shared_ptr<ExpressionNode> maxN) : m_val(v), m_min(minN), m_max(maxN) {}
    float evaluate(float t, float b) const override {
        float val = m_val->evaluate(t, b);
        float minV = m_min->evaluate(t, b);
        float maxV = m_max->evaluate(t, b);
        return std::clamp(val, minV, maxV);
    }
};

// ─── 簡易トークナイザーと再帰下降パーサ ───

enum class TokenType { Number, VarT, VarBase, Plus, Minus, Mul, Div, LParen, RParen, FunPow, FunSin, FunCos, FunSqrt, FunAbs, FunClamp, Comma, End };

struct Token {
    TokenType type;
    float value = 0.0f;
};

class Parser {
    std::vector<Token> m_tokens;
    size_t m_pos = 0;

    Token peek() const { return m_tokens[m_pos]; }
    Token consume() { return m_tokens[m_pos++]; }
    void expect(TokenType type) {
        if (consume().type != type) {
            throw std::runtime_error("Unexpected token in expression");
        }
    }

    std::shared_ptr<ExpressionNode> primary() {
        Token t = consume();
        switch (t.type) {
        case TokenType::Number:
            return std::make_shared<NumberNode>(t.value);
        case TokenType::VarT:
            return std::make_shared<VariableTNode>();
        case TokenType::VarBase:
            return std::make_shared<VariableBaseNode>();
        case TokenType::LParen: {
            auto node = expr();
            expect(TokenType::RParen);
            return node;
        }
        case TokenType::FunPow: {
            expect(TokenType::LParen);
            auto base = expr();
            expect(TokenType::Comma);
            auto exp = expr();
            expect(TokenType::RParen);
            return std::make_shared<PowNode>(base, exp);
        }
        case TokenType::FunSin: {
            expect(TokenType::LParen);
            auto node = expr();
            expect(TokenType::RParen);
            return std::make_shared<SinNode>(node);
        }
        case TokenType::FunCos: {
            expect(TokenType::LParen);
            auto node = expr();
            expect(TokenType::RParen);
            return std::make_shared<CosNode>(node);
        }
        case TokenType::FunSqrt: {
            expect(TokenType::LParen);
            auto node = expr();
            expect(TokenType::RParen);
            return std::make_shared<SqrtNode>(node);
        }
        case TokenType::FunAbs: {
            expect(TokenType::LParen);
            auto node = expr();
            expect(TokenType::RParen);
            return std::make_shared<AbsNode>(node);
        }
        case TokenType::FunClamp: {
            expect(TokenType::LParen);
            auto val = expr();
            expect(TokenType::Comma);
            auto minN = expr();
            expect(TokenType::Comma);
            auto maxN = expr();
            expect(TokenType::RParen);
            return std::make_shared<ClampNode>(val, minN, maxN);
        }
        default:
            throw std::runtime_error("Unexpected token in primary expression");
        }
    }

    std::shared_ptr<ExpressionNode> unary() {
        if (peek().type == TokenType::Minus) {
            consume();
            return std::make_shared<NegateNode>(unary());
        }
        return primary();
    }

    std::shared_ptr<ExpressionNode> factor() {
        auto node = unary();
        while (peek().type == TokenType::Mul || peek().type == TokenType::Div) {
            Token op = consume();
            if (op.type == TokenType::Mul) {
                node = std::make_shared<MulNode>(node, unary());
            } else {
                node = std::make_shared<DivNode>(node, unary());
            }
        }
        return node;
    }

    std::shared_ptr<ExpressionNode> expr() {
        auto node = factor();
        while (peek().type == TokenType::Plus || peek().type == TokenType::Minus) {
            Token op = consume();
            if (op.type == TokenType::Plus) {
                node = std::make_shared<AddNode>(node, factor());
            } else {
                node = std::make_shared<SubNode>(node, factor());
            }
        }
        return node;
    }

  public:
    explicit Parser(const std::vector<Token> &tokens) : m_tokens(tokens) {}
    std::shared_ptr<ExpressionNode> parse() {
        auto node = expr();
        expect(TokenType::End);
        return node;
    }
};

static std::vector<Token> tokenize(const std::string &src) {
    std::vector<Token> tokens;
    size_t i = 0;
    while (i < src.size()) {
        char c = src[i];
        if (std::isspace(c)) {
            i++;
            continue;
        }
        if (std::isdigit(c) || c == '.') {
            size_t start = i;
            while (i < src.size() && (std::isdigit(src[i]) || src[i] == '.'))
                i++;
            tokens.push_back({TokenType::Number, std::stof(src.substr(start, i - start))});
            continue;
        }
        if (std::isalpha(c)) {
            size_t start = i;
            while (i < src.size() && (std::isalnum(src[i]) || src[i] == '_'))
                i++;
            std::string s = src.substr(start, i - start);
            if (s == "t")
                tokens.push_back({TokenType::VarT});
            else if (s == "base")
                tokens.push_back({TokenType::VarBase});
            else if (s == "pow")
                tokens.push_back({TokenType::FunPow});
            else if (s == "sin")
                tokens.push_back({TokenType::FunSin});
            else if (s == "cos")
                tokens.push_back({TokenType::FunCos});
            else if (s == "sqrt")
                tokens.push_back({TokenType::FunSqrt});
            else if (s == "abs")
                tokens.push_back({TokenType::FunAbs});
            else if (s == "clamp")
                tokens.push_back({TokenType::FunClamp});
            else
                tokens.push_back({TokenType::VarBase}); // 未知のシンボルはbaseとして扱う
            continue;
        }
        if (c == '+') {
            tokens.push_back({TokenType::Plus});
            i++;
        } else if (c == '-') {
            tokens.push_back({TokenType::Minus});
            i++;
        } else if (c == '*') {
            tokens.push_back({TokenType::Mul});
            i++;
        } else if (c == '/') {
            tokens.push_back({TokenType::Div});
            i++;
        } else if (c == '(') {
            tokens.push_back({TokenType::LParen});
            i++;
        } else if (c == ')') {
            tokens.push_back({TokenType::RParen});
            i++;
        } else if (c == ',') {
            tokens.push_back({TokenType::Comma});
            i++;
        } else {
            i++;
        } // 無視
    }
    tokens.push_back({TokenType::End});
    return tokens;
}

// ─── InterpolationEngine の実装 ───

InterpolationEngine::InterpolationEngine() {
    // 優れたC++ネイティブのイージング関数群を登録
    m_easings["linear"] = [](float t, const auto &) { return t; };
    m_easings["ease_in_sine"] = [](float t, const auto &) { return 1.0f - std::cos(t * 3.14159265f / 2.0f); };
    m_easings["ease_out_sine"] = [](float t, const auto &) { return std::sin(t * 3.14159265f / 2.0f); };
    m_easings["ease_in_out_sine"] = [](float t, const auto &) { return -(std::cos(3.14159265f * t) - 1.0f) / 2.0f; };
    m_easings["ease_in_quad"] = [](float t, const auto &) { return t * t; };
    m_easings["ease_out_quad"] = [](float t, const auto &) { return 1.0f - (1.0f - t) * (1.0f - t); };
    m_easings["ease_in_out_quad"] = [](float t, const auto &) { return t < 0.5f ? 2.0f * t * t : 1.0f - ((-2.0f * t + 2.0f) * (-2.0f * t + 2.0f)) / 2.0f; };
    m_easings["ease_in_cubic"] = [](float t, const auto &) { return t * t * t; };
    m_easings["ease_out_cubic"] = [](float t, const auto &) { return 1.0f - (1.0f - t) * (1.0f - t) * (1.0f - t); };
    m_easings["ease_in_out_cubic"] = [](float t, const auto &) { return t < 0.5f ? 4.0f * t * t * t : 1.0f - ((-2.0f * t + 2.0f) * (-2.0f * t + 2.0f) * (-2.0f * t + 2.0f)) / 2.0f; };

    // 3次ベジェカスタム補間
    m_easings["custom"] = [](float x, const std::vector<float> &p) {
        if (p.size() < 6)
            return x;
        float prevX = 0, prevY = 0;
        for (size_t i = 0; i < p.size(); i += 6) {
            float cp1x = p[i], cp1y = p[i + 1], cp2x = p[i + 2], cp2y = p[i + 3], endX = p[i + 4], endY = p[i + 5];
            if (x <= endX || i + 6 >= p.size()) {
                float range = endX - prevX;
                if (range < 1e-6f)
                    return endY;
                float n_cp1x = (cp1x - prevX) / range, n_cp2x = (cp2x - prevX) / range, n_x = (x - prevX) / range;
                float t = solveBezierT(n_x, n_cp1x, n_cp2x);
                return (1.0f - t) * (1.0f - t) * (1.0f - t) * prevY + 3.0f * (1.0f - t) * (1.0f - t) * t * cp1y + 3.0f * (1.0f - t) * t * t * cp2y + t * t * t * endY;
            }
            prevX = endX;
            prevY = endY;
        }
        return x;
    };
}

InterpolationEngine &InterpolationEngine::instance() {
    static InterpolationEngine inst;
    return inst;
}

float InterpolationEngine::solveBezierT(float x, float x1, float x2) {
    if (x1 == x2 && x1 == x)
        return x;
    float t = x;
    for (int i = 0; i < 8; ++i) {
        const float one_minus_t = 1.0f - t;
        const float current_x = 3.0f * one_minus_t * one_minus_t * t * x1 + 3.0f * one_minus_t * t * t * x2 + t * t * t;
        const float error = current_x - x;
        if (std::abs(error) < 1e-5f)
            return t;
        const float dx_dt = 3.0f * one_minus_t * one_minus_t * x1 + 6.0f * one_minus_t * t * (x2 - x1) + 3.0f * t * t * (1.0f - x2);
        if (std::abs(dx_dt) < 1e-6f)
            break;
        t -= error / dx_dt;
    }
    return std::clamp(t, 0.0f, 1.0f);
}

float InterpolationEngine::evaluate(const std::vector<Keyframe> &keyframes, int frame, float fallback) const {
    if (keyframes.empty())
        return fallback;

    if (frame <= keyframes.front().frame)
        return keyframes.front().value;
    if (frame >= keyframes.back().frame)
        return keyframes.back().value;

    for (size_t i = 0; i < keyframes.size() - 1; ++i) {
        const int f0 = keyframes[i].frame;
        const int f1 = keyframes[i + 1].frame;
        if (frame < f0 || frame > f1)
            continue;

        const float a = keyframes[i].value;
        const float b = keyframes[i + 1].value;
        const float tRaw = static_cast<float>(frame - f0) / static_cast<float>(f1 - f0);

        std::string type = keyframes[i].interpolation.toStdString();

        if (type == "none") {
            return (frame < f1) ? a : b;
        }

        if (type == "custom") {
            // カスタム数式 (expression) 評価
            if (!keyframes[i].expression.isEmpty()) {
                try {
                    auto ast = compileExpression(keyframes[i].expression.toStdString());
                    return ast->evaluate(tRaw, a);
                } catch (...) {
                    return a; // フォールバック
                }
            }
            // ベジェカスタム補間
            std::vector<float> params = {keyframes[i].bzx1, keyframes[i].bzy1, keyframes[i].bzx2, keyframes[i].bzy2, 1.0f, 1.0f};
            return a + (b - a) * m_easings.at("custom")(tRaw, params);
        }

        if (m_easings.find(type) == m_easings.end()) {
            type = "linear";
        }

        return a + (b - a) * m_easings.at(type)(tRaw, {});
    }

    return keyframes.back().value;
}

std::shared_ptr<ExpressionNode> InterpolationEngine::compileExpression(const std::string &expression) const {
    auto tokens = tokenize(expression);
    Parser parser(tokens);
    return parser.parse();
}

} // namespace AviQtl::Core
