#pragma once
#include "../../scripting/lua_host.hpp"
#include <QColor>
#include <QHash>
#include <QObject>
#include <QQmlEngine>
#include <QVariant>
#include <QVariantList>
#include <algorithm>
#include <array>
#include <cmath>
#include <functional>
#include <limits>
#include <map>

namespace AviQtl::UI {

// イージング関数シグネチャ: double function(t, params)
using EasingFunction = std::function<double(double, const std::vector<double> &)>;

class EffectModel : public QObject {
    Q_OBJECT

  private:
    // ─── Static Helpers (メソッドからの参照のため先頭に配置) ───

    static bool isStructuredTrack(const QVariant &raw) {
        const QVariantMap m = raw.toMap();
        return m.contains(QStringLiteral("start")) && m.contains(QStringLiteral("points"));
    }
    // フェーズ3: QVariantボクシングを排除した数値専用トラック評価
    // evaluateTrack と同一ロジックだが戻り値が float (boxing なし)
    // カラー補間・Lua式は対象外 (audio/video の volume, pan 専用)
    static float evaluateTrackFloat(const QVariantList &track, int frame, float fallback) {
        if (track.isEmpty())
            return fallback;
        auto getFrame = [](const QVariant &v) { return v.toMap().value(QStringLiteral("frame")).toInt(); };
        auto getValueD = [](const QVariant &v) -> double { return v.toMap().value(QStringLiteral("value")).toDouble(); };
        auto getInterp = [](const QVariant &v) { return v.toMap().value(QStringLiteral("interp")).toString(); };
        auto getModeParams = [](const QVariant &v) { return v.toMap().value(QStringLiteral("modeParams")).toMap(); };
        auto getBezierParams = [](const QVariant &v) -> std::vector<double> {
            const auto map = v.toMap();
            if (map.contains(QStringLiteral("points"))) {
                QVariantList lst = map.value(QStringLiteral("points")).toList();
                std::vector<double> pts;
                pts.reserve(static_cast<std::size_t>(lst.size()));
                for (const auto &val : std::as_const(lst))
                    pts.push_back(val.toDouble());
                return pts;
            }
            return {map.value(QStringLiteral("bzx1"), 0.33).toDouble(), map.value(QStringLiteral("bzy1"), 0.0).toDouble(), map.value(QStringLiteral("bzx2"), 0.66).toDouble(), map.value(QStringLiteral("bzy2"), 1.0).toDouble(), 1.0, 1.0};
        };

        if (frame <= getFrame(track.front()))
            return static_cast<float>(getValueD(track.front()));
        if (frame >= getFrame(track.back()))
            return static_cast<float>(getValueD(track.back()));

        for (int i = 0; i < track.size() - 1; ++i) {
            const int f0 = getFrame(track[i]), f1 = getFrame(track[i + 1]);
            if (frame < f0 || frame > f1)
                continue;
            const double a = getValueD(track[i]), b = getValueD(track[i + 1]);
            const double tRaw = (frame - f0) / static_cast<double>(f1 - f0);
            QString type = getInterp(track[i]);
            const QVariantMap modeParams = getModeParams(track[i]);

            if (type == QStringLiteral("none"))
                return static_cast<float>((frame < f1) ? a : b);
            if (type == QStringLiteral("random")) {
                const int stepFrames = std::max(1, modeParams.value(QStringLiteral("stepFrames"), 1).toInt());
                const int stepIndex = (frame - f0) / stepFrames;
                const quint32 seed = qHash(f0) ^ qHash(f1) ^ qHash(stepIndex) ^ qHash(static_cast<qint64>(a * 1000)) ^ qHash(static_cast<qint64>(b * 1000));
                return static_cast<float>(std::min(a, b) + (std::max(a, b) - std::min(a, b)) * (double(seed % 1000000u) / 999999.0));
            }
            if (type == QStringLiteral("alternate")) {
                const int stepFrames = std::max(1, modeParams.value(QStringLiteral("stepFrames"), 1).toInt());
                return static_cast<float>(((frame - f0) / stepFrames % 2 == 0) ? a : b);
            }
            std::vector<double> params;
            if (type == QStringLiteral("custom"))
                params = getBezierParams(track[i]);
            if (!easingFunctions().contains(type))
                type = QStringLiteral("linear");
            return static_cast<float>(a + (b - a) * easingFunctions().value(type)(tRaw, params));
        }
        return static_cast<float>(getValueD(track.back()));
    }

    static QVariantMap makePoint(int frame, const QVariant &value, const QString &interp = QStringLiteral("none")) {
        QVariantMap p;
        p[QStringLiteral("frame")] = frame;
        p[QStringLiteral("value")] = value;
        p[QStringLiteral("interp")] = interp;
        return p;
    }

    static QVariantList sortPoints(QVariantList points) {
        std::sort(points.begin(), points.end(), [](const QVariant &a, const QVariant &b) { return a.toMap().value(QStringLiteral("frame")).toInt() < b.toMap().value(QStringLiteral("frame")).toInt(); });
        return points;
    }

    static int inferredDurationForTrack(const QVariant &raw) {
        if (isStructuredTrack(raw)) {
            const QVariantList points = raw.toMap().value(QStringLiteral("points")).toList();
            int maxFrame = 0;
            for (const auto &v : std::as_const(points))
                maxFrame = std::max(maxFrame, v.toMap().value(QStringLiteral("frame")).toInt());
            return std::max(1, maxFrame + 1);
        }
        const QVariantList list = raw.toList();
        if (list.isEmpty())
            return 1;
        int maxFrame = 0;
        for (const auto &v : std::as_const(list))
            maxFrame = std::max(maxFrame, v.toMap().value(QStringLiteral("frame")).toInt());
        return std::max(1, maxFrame + 1);
    }

    static QVariantList flattenStructuredTrack(const QVariantMap &track) {
        QVariantList out;
        out.append(track.value(QStringLiteral("start")));
        QVariantList points = track.value(QStringLiteral("points")).toList();
        points = sortPoints(points);
        for (const auto &v : std::as_const(points))
            out.append(v);
        return out;
    }

    // 3次ベジェ曲線のX座標(時間)からT(パラメータ)を求める - ニュートン法
    static double solveBezierT(double x, double x1, double x2) {
        if (x1 == x2 && x1 == x)
            return x;
        double t = x;
        for (int i = 0; i < 8; ++i) {
            const double one_minus_t = 1.0 - t;
            const double current_x = 3 * one_minus_t * one_minus_t * t * x1 + 3 * one_minus_t * t * t * x2 + t * t * t;
            const double error = current_x - x;
            if (std::abs(error) < 1e-5)
                return t;
            const double dx_dt = 3 * one_minus_t * one_minus_t * x1 + 6 * one_minus_t * t * (x2 - x1) + 3 * t * t * (1.0 - x2);
            if (std::abs(dx_dt) < 1e-6)
                break;
            t -= error / dx_dt;
        }
        return std::clamp(t, 0.0, 1.0);
    }

    static const QHash<QString, EasingFunction> &easingFunctions() {
        static auto easeOutBounce = [](double x) -> double {
            constexpr double n1 = 7.5625, d1 = 2.75;
            if (x < 1.0 / d1)
                return n1 * x * x;
            if (x < 2.0 / d1) {
                x -= 1.5 / d1;
                return n1 * x * x + 0.75;
            }
            if (x < 2.5 / d1) {
                x -= 2.25 / d1;
                return n1 * x * x + 0.9375;
            }
            x -= 2.625 / d1;
            return n1 * x * x + 0.984375;
        };

        static const QHash<QString, EasingFunction> funcs = {
            {QStringLiteral("linear"), [](double t, const auto &) { return t; }},
            {QStringLiteral("ease_in_sine"), [](double t, const auto &) { return 1.0 - std::cos(t * M_PI / 2.0); }},
            {QStringLiteral("ease_out_sine"), [](double t, const auto &) { return std::sin(t * M_PI / 2.0); }},
            {QStringLiteral("ease_in_out_sine"), [](double t, const auto &) { return -(std::cos(M_PI * t) - 1.0) / 2.0; }},
            {QStringLiteral("ease_out_in_sine"), [](double t, const auto &) { return t < 0.5 ? std::sin(t * M_PI) / 2.0 : (1.0 - std::cos((t * 2.0 - 1.0) * M_PI / 2.0)) / 2.0 + 0.5; }},
            {QStringLiteral("ease_in_quad"), [](double t, const auto &) { return t * t; }},
            {QStringLiteral("ease_out_quad"), [](double t, const auto &) { return 1.0 - (1.0 - t) * (1.0 - t); }},
            {QStringLiteral("ease_in_out_quad"), [](double t, const auto &) { return t < 0.5 ? 2.0 * t * t : 1.0 - ((-2.0 * t + 2.0) * (-2.0 * t + 2.0)) / 2.0; }},
            {QStringLiteral("ease_out_in_quad"), [](double t, const auto &) { return t < 0.5 ? (1.0 - (1.0 - 2.0 * t) * (1.0 - 2.0 * t)) / 2.0 : (2.0 * t - 1.0) * (2.0 * t - 1.0) / 2.0 + 0.5; }},
            {QStringLiteral("ease_in_cubic"), [](double t, const auto &) { return t * t * t; }},
            {QStringLiteral("ease_out_cubic"), [](double t, const auto &) { return 1.0 - (1.0 - t) * (1.0 - t) * (1.0 - t); }},
            {QStringLiteral("ease_in_out_cubic"), [](double t, const auto &) { return t < 0.5 ? 4.0 * t * t * t : 1.0 - ((-2.0 * t + 2.0) * (-2.0 * t + 2.0) * (-2.0 * t + 2.0)) / 2.0; }},
            {QStringLiteral("ease_out_in_cubic"), [](double t, const auto &) { return t < 0.5 ? (1.0 - std::pow(1.0 - 2.0 * t, 3.0)) / 2.0 : std::pow(2.0 * t - 1.0, 3.0) / 2.0 + 0.5; }},
            {QStringLiteral("ease_in_quart"), [](double t, const auto &) { return t * t * t * t; }},
            {QStringLiteral("ease_out_quart"), [](double t, const auto &) { return 1.0 - (1.0 - t) * (1.0 - t) * (1.0 - t) * (1.0 - t); }},
            {QStringLiteral("ease_in_out_quart"), [](double t, const auto &) { return t < 0.5 ? 8.0 * t * t * t * t : 1.0 - ((-2.0 * t + 2.0) * (-2.0 * t + 2.0) * (-2.0 * t + 2.0) * (-2.0 * t + 2.0)) / 2.0; }},
            {QStringLiteral("ease_out_in_quart"), [](double t, const auto &) { return t < 0.5 ? (1.0 - std::pow(1.0 - 2.0 * t, 4.0)) / 2.0 : std::pow(2.0 * t - 1.0, 4.0) / 2.0 + 0.5; }},
            {QStringLiteral("ease_in_quint"), [](double t, const auto &) { return t * t * t * t * t; }},
            {QStringLiteral("ease_out_quint"), [](double t, const auto &) { return 1.0 - (1.0 - t) * (1.0 - t) * (1.0 - t) * (1.0 - t) * (1.0 - t); }},
            {QStringLiteral("ease_in_out_quint"), [](double t, const auto &) { return t < 0.5 ? 16.0 * t * t * t * t * t : 1.0 - ((-2.0 * t + 2.0) * (-2.0 * t + 2.0) * (-2.0 * t + 2.0) * (-2.0 * t + 2.0) * (-2.0 * t + 2.0)) / 2.0; }},
            {QStringLiteral("ease_out_in_quint"), [](double t, const auto &) { return t < 0.5 ? (1.0 - std::pow(1.0 - 2.0 * t, 5.0)) / 2.0 : std::pow(2.0 * t - 1.0, 5.0) / 2.0 + 0.5; }},
            {QStringLiteral("ease_in_expo"), [](double t, const auto &) { return t == 0.0 ? 0.0 : std::pow(2.0, 10.0 * t - 10.0); }},
            {QStringLiteral("ease_out_expo"), [](double t, const auto &) { return t == 1.0 ? 1.0 : 1.0 - std::pow(2.0, -10.0 * t); }},
            {QStringLiteral("ease_in_out_expo"),
             [](double t, const auto &) {
                 if (t == 0.0)
                     return 0.0;
                 if (t == 1.0)
                     return 1.0;
                 return t < 0.5 ? std::pow(2.0, 20.0 * t - 10.0) / 2.0 : (2.0 - std::pow(2.0, -20.0 * t + 10.0)) / 2.0;
             }},
            {QStringLiteral("ease_out_in_expo"),
             [](double t, const auto &) {
                 if (t == 0.0)
                     return 0.0;
                 if (t == 1.0)
                     return 1.0;
                 return t < 0.5 ? (1.0 - std::pow(2.0, -20.0 * t)) / 2.0 : std::pow(2.0, 20.0 * t - 20.0) / 2.0 + 0.5;
             }},
            {QStringLiteral("ease_in_circ"), [](double t, const auto &) { return 1.0 - std::sqrt(1.0 - t * t); }},
            {QStringLiteral("ease_out_circ"), [](double t, const auto &) { return std::sqrt(1.0 - (t - 1.0) * (t - 1.0)); }},
            {QStringLiteral("ease_in_out_circ"), [](double t, const auto &) { return t < 0.5 ? (1.0 - std::sqrt(1.0 - 4.0 * t * t)) / 2.0 : (std::sqrt(1.0 - (-2.0 * t + 2.0) * (-2.0 * t + 2.0)) + 1.0) / 2.0; }},
            {QStringLiteral("ease_out_in_circ"), [](double t, const auto &) { return t < 0.5 ? std::sqrt(1.0 - (2.0 * t - 1.0) * (2.0 * t - 1.0)) / 2.0 : (1.0 - std::sqrt(1.0 - (2.0 * t - 1.0) * (2.0 * t - 1.0))) / 2.0 + 0.5; }},
            {QStringLiteral("ease_in_back"),
             [](double t, const auto &) {
                 constexpr double c1 = 1.70158, c3 = 1.70158 + 1.0;
                 return c3 * t * t * t - c1 * t * t;
             }},
            {QStringLiteral("ease_out_back"),
             [](double t, const auto &) {
                 constexpr double c1 = 1.70158, c3 = 1.70158 + 1.0;
                 return 1.0 + c3 * (t - 1.0) * (t - 1.0) * (t - 1.0) + c1 * (t - 1.0) * (t - 1.0);
             }},
            {QStringLiteral("ease_in_out_back"),
             [](double t, const auto &) {
                 constexpr double c2 = 1.70158 * 1.525;
                 return t < 0.5 ? ((2.0 * t) * (2.0 * t) * ((c2 + 1.0) * 2.0 * t - c2)) / 2.0 : ((2.0 * t - 2.0) * (2.0 * t - 2.0) * ((c2 + 1.0) * (2.0 * t - 2.0) + c2) + 2.0) / 2.0;
             }},
            {QStringLiteral("ease_out_in_back"),
             [](double t, const auto &) {
                 constexpr double c1 = 1.70158, c3 = c1 + 1.0;
                 auto eout = [&](double u) { return 1.0 + c3 * (u - 1.0) * (u - 1.0) * (u - 1.0) + c1 * (u - 1.0) * (u - 1.0); };
                 auto ein = [&](double u) { return c3 * u * u * u - c1 * u * u; };
                 return t < 0.5 ? eout(2.0 * t) / 2.0 : ein(2.0 * t - 1.0) / 2.0 + 0.5;
             }},
            {QStringLiteral("ease_in_elastic"),
             [](double t, const auto &) {
                 constexpr double c4 = (2.0 * M_PI) / 3.0;
                 if (t == 0.0)
                     return 0.0;
                 if (t == 1.0)
                     return 1.0;
                 return -std::pow(2.0, 10.0 * t - 10.0) * std::sin((t * 10.0 - 10.75) * c4);
             }},
            {QStringLiteral("ease_out_elastic"),
             [](double t, const auto &) {
                 constexpr double c4 = (2.0 * M_PI) / 3.0;
                 if (t == 0.0)
                     return 0.0;
                 if (t == 1.0)
                     return 1.0;
                 return std::pow(2.0, -10.0 * t) * std::sin((t * 10.0 - 0.75) * c4) + 1.0;
             }},
            {QStringLiteral("ease_in_out_elastic"),
             [](double t, const auto &) {
                 constexpr double c5 = (2.0 * M_PI) / 4.5;
                 if (t == 0.0)
                     return 0.0;
                 if (t == 1.0)
                     return 1.0;
                 return t < 0.5 ? -(std::pow(2.0, 20.0 * t - 10.0) * std::sin((20.0 * t - 11.125) * c5)) / 2.0 : (std::pow(2.0, -20.0 * t + 10.0) * std::sin((20.0 * t - 11.125) * c5)) / 2.0 + 1.0;
             }},
            {QStringLiteral("ease_out_in_elastic"),
             [](double t, const auto &) {
                 constexpr double c4 = (2.0 * M_PI) / 3.0;
                 if (t == 0.0)
                     return 0.0;
                 if (t == 1.0)
                     return 1.0;
                 auto eout = [&](double u) { return std::pow(2.0, -10.0 * u) * std::sin((u * 10.0 - 0.75) * c4) + 1.0; };
                 auto ein = [&](double u) {
                     if (u == 0.0)
                         return 0.0;
                     if (u == 1.0)
                         return 1.0;
                     return -std::pow(2.0, 10.0 * u - 10.0) * std::sin((u * 10.0 - 10.75) * c4);
                 };
                 return t < 0.5 ? eout(2.0 * t) / 2.0 : ein(2.0 * t - 1.0) / 2.0 + 0.5;
             }},
            {QStringLiteral("ease_out_bounce"), [](double t, const auto &) { return easeOutBounce(t); }},
            {QStringLiteral("ease_in_bounce"), [](double t, const auto &) { return 1.0 - easeOutBounce(1.0 - t); }},
            {QStringLiteral("ease_in_out_bounce"), [](double t, const auto &) { return t < 0.5 ? (1.0 - easeOutBounce(1.0 - 2.0 * t)) / 2.0 : (1.0 + easeOutBounce(2.0 * t - 1.0)) / 2.0; }},
            {QStringLiteral("ease_out_in_bounce"), [](double t, const auto &) { return t < 0.5 ? easeOutBounce(2.0 * t) / 2.0 : (1.0 - easeOutBounce(1.0 - 2.0 * (t - 0.5))) / 2.0 + 0.5; }},
            {QStringLiteral("custom"), [](double x, const auto &p) {
                 double prevX = 0, prevY = 0;
                 for (size_t i = 0; i < p.size(); i += 6) {
                     double cp1x = p[i], cp1y = p[i + 1], cp2x = p[i + 2], cp2y = p[i + 3], endX = p[i + 4], endY = p[i + 5];
                     if (x <= endX || i + 6 >= p.size()) {
                         double range = endX - prevX;
                         if (range < 1e-6)
                             return endY;
                         double n_cp1x = (cp1x - prevX) / range, n_cp2x = (cp2x - prevX) / range, n_x = (x - prevX) / range;
                         double t = solveBezierT(n_x, n_cp1x, n_cp2x);
                         return (1 - t) * (1 - t) * (1 - t) * prevY + 3 * (1 - t) * (1 - t) * t * cp1y + 3 * (1 - t) * t * t * cp2y + t * t * t * endY;
                     }
                     prevX = endX;
                     prevY = endY;
                 }
                 return x;
             }}};
        return funcs;
    }

    static QVariant evaluateTrack(const QVariantList &track, int frame, const QVariant &fallback) {
        if (track.isEmpty())
            return fallback;
        auto getFrame = [](const QVariant &v) { return v.toMap().value(QStringLiteral("frame")).toInt(); };
        auto getValue = [](const QVariant &v) { return v.toMap().value(QStringLiteral("value")); };
        auto getInterp = [](const QVariant &v) { return v.toMap().value(QStringLiteral("interp")).toString(); };
        auto getModeParams = [](const QVariant &v) { return v.toMap().value(QStringLiteral("modeParams")).toMap(); };
        auto getBezierParams = [](const QVariant &v) -> std::vector<double> {
            const auto map = v.toMap();
            if (map.contains(QStringLiteral("points"))) {
                QVariantList lst = map.value(QStringLiteral("points")).toList();
                std::vector<double> pts;
                for (const auto &val : std::as_const(lst))
                    pts.push_back(val.toDouble());
                return pts;
            }
            return {map.value(QStringLiteral("bzx1"), 0.33).toDouble(), map.value(QStringLiteral("bzy1"), 0.0).toDouble(), map.value(QStringLiteral("bzx2"), 0.66).toDouble(), map.value(QStringLiteral("bzy2"), 1.0).toDouble(), 1.0, 1.0};
        };

        if (frame <= getFrame(track.front()))
            return getValue(track.front());
        if (frame >= getFrame(track.back()))
            return getValue(track.back());

        const bool numeric = fallback.canConvert<double>();
        for (int i = 0; i < track.size() - 1; ++i) {
            const int f0 = getFrame(track[i]), f1 = getFrame(track[i + 1]);
            if (frame < f0 || frame > f1)
                continue;
            const QVariant v0 = getValue(track[i]), v1 = getValue(track[i + 1]);
            const double tRaw = (frame - f0) / double(f1 - f0);
            QString type = getInterp(track[i]);
            const QVariantMap modeParams = getModeParams(track[i]);

            if (type == QStringLiteral("none"))
                return (frame < f1) ? v0 : v1;
            if (v0.typeId() == QMetaType::QString && v1.typeId() == QMetaType::QString) {
                QColor c0(v0.toString()), c1(v1.typeId() == QMetaType::QString ? v1.toString() : v0.toString());
                if (c0.isValid() && c1.isValid()) {
                    std::vector<double> params;
                    if (type == QStringLiteral("custom"))
                        params = getBezierParams(track[i]);
                    if (!easingFunctions().contains(type))
                        type = QStringLiteral("linear");
                    const double t = easingFunctions().value(type)(tRaw, params);
                    return QColor(static_cast<int>(c0.red() + (c1.red() - c0.red()) * t), static_cast<int>(c0.green() + (c1.green() - c0.green()) * t), static_cast<int>(c0.blue() + (c1.blue() - c0.blue()) * t),
                                  static_cast<int>(c0.alpha() + (c1.alpha() - c0.alpha()) * t))
                        .name(QColor::HexArgb);
                }
            }
            if (!numeric || !v0.canConvert<double>() || !v1.canConvert<double>())
                return v0;
            const double a = v0.toDouble(), b = v1.toDouble();
            if (type == QStringLiteral("random")) {
                const int stepFrames = std::max(1, modeParams.value(QStringLiteral("stepFrames"), 1).toInt()), stepIndex = (frame - f0) / stepFrames;
                const quint32 seed = qHash(f0) ^ qHash(f1) ^ qHash(stepIndex) ^ qHash(static_cast<qint64>(a * 1000)) ^ qHash(static_cast<qint64>(b * 1000));
                return std::min(a, b) + (std::max(a, b) - std::min(a, b)) * (double(seed % 1000000u) / 999999.0);
            }
            if (type == QStringLiteral("alternate")) {
                const int stepFrames = std::max(1, modeParams.value(QStringLiteral("stepFrames"), 1).toInt());
                return ((frame - f0) / stepFrames % 2 == 0) ? a : b;
            }
            std::vector<double> params;
            if (type == QStringLiteral("custom"))
                params = getBezierParams(track[i]);
            if (!easingFunctions().contains(type))
                type = QStringLiteral("linear");
            return a + (b - a) * easingFunctions().value(type)(tRaw, params);
        }
        return getValue(track.back());
    }

    static QVariantMap normalizeTrackForDuration(const QVariant &rawTrack, const QVariant &fallback, int durationFrames) {
        if (isStructuredTrack(rawTrack)) {
            QVariantMap raw = rawTrack.toMap();
            QVariantMap start = raw.value(QStringLiteral("start")).toMap();
            QVariantList points = raw.value(QStringLiteral("points")).toList(), nextPoints;
            start[QStringLiteral("frame")] = 0;
            if (!start.contains(QStringLiteral("value")))
                start[QStringLiteral("value")] = fallback;

            const int ceiling = durationFrames;

            for (const auto &v : std::as_const(points)) {
                const int f = v.toMap().value(QStringLiteral("frame")).toInt();
                if (f > 0 && f <= ceiling)
                    nextPoints.append(v);
            }
            QVariantMap out;
            out[QStringLiteral("start")] = start;
            out[QStringLiteral("points")] = sortPoints(nextPoints); // フレーム位置はユーザー設定値のまま
            return out;
        }
        // レガシーリスト形式（end なし）
        QVariantList legacy = sortPoints(rawTrack.toList()), points;
        QVariantMap start = makePoint(0, legacy.isEmpty() ? fallback : evaluateTrack(legacy, 0, fallback), QStringLiteral("linear"));
        for (const auto &v : std::as_const(legacy)) {
            const int f = v.toMap().value(QStringLiteral("frame")).toInt();
            if (f > 0 && f < durationFrames)
                points.append(v);
        }
        QVariantMap out;
        out[QStringLiteral("start")] = start;
        out[QStringLiteral("points")] = sortPoints(points);
        return out;
    }

  public:
    Q_PROPERTY(QString id READ id CONSTANT)
    Q_PROPERTY(QString name READ name CONSTANT)
    Q_PROPERTY(QString kind READ kind CONSTANT)
    Q_PROPERTY(QStringList categories READ categories CONSTANT)
    Q_PROPERTY(bool enabled READ isEnabled WRITE setEnabled NOTIFY enabledChanged)
    Q_PROPERTY(QVariantMap params READ params NOTIFY paramsChanged)
    Q_PROPERTY(QString qmlSource READ qmlSource CONSTANT)
    Q_PROPERTY(QVariantMap keyframeTracks READ keyframeTracks NOTIFY keyframeTracksChanged)
    Q_PROPERTY(QVariantMap uiDefinition READ uiDefinition CONSTANT)

    explicit EffectModel(const QString &id, const QString &name, const QString &kind, const QStringList &categories, const QVariantMap &params = {}, const QString &qmlSource = "", const QVariantMap &uiDef = {}, QObject *parent = nullptr)
        : QObject(parent), m_id(id), m_name(name), m_kind(kind), m_categories(categories), m_enabled(true), m_params(params), m_qmlSource(qmlSource), m_uiDefinition(uiDef) {
        for (auto it = m_params.begin(); it != m_params.end(); ++it) {
            QVariantMap track;
            QVariantMap start;
            start[QStringLiteral("frame")] = 0;
            start[QStringLiteral("value")] = it.value();
            start[QStringLiteral("interp")] = QStringLiteral("none");
            track[QStringLiteral("start")] = start;
            // end は設定しない（任意終了点の哲学）
            track[QStringLiteral("points")] = QVariantList();
            m_keyframeTracks[it.key()] = track;
        }
    }

    QString id() const { return m_id; }
    QString name() const { return m_name; }
    QString kind() const { return m_kind; }
    QStringList categories() const { return m_categories; }
    bool isEnabled() const { return m_enabled; }
    QVariantMap params() const { return m_params; }
    QString qmlSource() const { return m_qmlSource; }
    QVariantMap keyframeTracks() const { return m_keyframeTracks; }
    QVariantMap uiDefinition() const { return m_uiDefinition; }

    EffectModel *clone() const {
        auto *copy = new EffectModel(m_id, m_name, m_kind, m_categories, m_params, m_qmlSource, m_uiDefinition);
        copy->m_enabled = m_enabled;
        copy->m_keyframeTracks = m_keyframeTracks;
        copy->m_lastDuration = m_lastDuration;
        return copy;
    }

    Q_INVOKABLE QVariantList keyframeListForUi(const QString &paramName) const {
        invalidateCache(paramName);
        const QVariant raw = m_keyframeTracks.value(paramName);
        if (isStructuredTrack(raw))
            return flattenStructuredTrack(raw.toMap());
        QVariantList list = raw.toList();
        std::sort(list.begin(), list.end(), [](const QVariant &a, const QVariant &b) { return a.toMap().value(QStringLiteral("frame")).toInt() < b.toMap().value(QStringLiteral("frame")).toInt(); });
        return list;
    }

    Q_INVOKABLE bool isEndpointFrame(const QString &paramName, int frame) const {
        const QVariant raw = m_keyframeTracks.value(paramName);
        const int startFrame = isStructuredTrack(raw) ? raw.toMap().value(QStringLiteral("start")).toMap().value(QStringLiteral("frame")).toInt() : 0;
        return frame == startFrame;
    }

    Q_INVOKABLE void syncTrackEndpoints(int durationFrames) {
        m_resolvedCache.clear();
        const int oldDuration = m_lastDuration;
        m_lastDuration = durationFrames;
        // 未初期化トラックを初期化し、旧終端フレームにある中間点を新終端フレームへ追従させる
        for (auto it = m_params.begin(); it != m_params.end(); ++it) {
            const QString &key = it.key();
            if (!m_keyframeTracks.contains(key) || !isStructuredTrack(m_keyframeTracks.value(key))) {
                QVariantMap start;
                start[QStringLiteral("frame")] = 0;
                start[QStringLiteral("value")] = it.value();
                start[QStringLiteral("interp")] = QStringLiteral("none");
                QVariantMap track;
                track[QStringLiteral("start")] = start;
                track[QStringLiteral("points")] = QVariantList();
                m_keyframeTracks[key] = track;
            } else if (oldDuration > 0 && oldDuration != durationFrames) {
                // 旧終端フレームにある中間点を新終端フレームへ追従させる
                QVariantMap track = m_keyframeTracks[key].toMap();
                QVariantList points = track[QStringLiteral("points")].toList();
                bool changed = false;
                for (int i = 0; i < points.size(); ++i) {
                    QVariantMap kf = points[i].toMap();
                    if (kf[QStringLiteral("frame")].toInt() == oldDuration) {
                        kf[QStringLiteral("frame")] = durationFrames;
                        points[i] = kf;
                        changed = true;
                        break;
                    }
                }
                if (changed) {
                    track[QStringLiteral("points")] = sortPoints(points);
                    m_keyframeTracks[key] = track;
                }
            }
        }
        emit keyframeTracksChanged();
    }

    Q_INVOKABLE QVariantMap splitTracks(int firstHalfDuration, int originalDuration) {
        m_resolvedCache.clear();
        QVariantMap secondHalfTracks;
        if (originalDuration < 1)
            return secondHalfTracks;

        const int firstEndFrame = std::max(0, firstHalfDuration - 1);
        const int secondHalfDur = std::max(1, originalDuration - firstHalfDuration);
        const int secondEndFrame = std::max(0, secondHalfDur - 1);
        QVariantMap currentTracks = m_keyframeTracks;

        for (auto it = m_params.begin(); it != m_params.end(); ++it) {
            const QString key = it.key();
            const QVariant fallback = it.value();
            QVariantMap track = normalizeTrackForDuration(currentTracks.value(key), fallback, originalDuration);
            QVariantList flat = flattenStructuredTrack(track);
            QVariantMap start = track.value(QStringLiteral("start")).toMap();
            QVariantList points = track.value(QStringLiteral("points")).toList();
            // 前半トラック
            QVariantMap firstTrack;
            QVariantList firstPoints;
            for (const auto &v : std::as_const(points)) {
                const int f = v.toMap().value(QStringLiteral("frame")).toInt();
                if (f > 0 && f < firstEndFrame)
                    firstPoints.append(v.toMap());
            }
            firstTrack[QStringLiteral("start")] = start;
            firstTrack[QStringLiteral("points")] = firstPoints;
            currentTracks[key] = firstTrack;

            // 後半トラック
            QVariantMap secondTrack;
            QVariantMap secondStart;
            secondStart[QStringLiteral("frame")] = 0;
            secondStart[QStringLiteral("value")] = evaluateTrack(flat, firstHalfDuration, fallback);
            secondStart[QStringLiteral("interp")] = start.value(QStringLiteral("interp"), QStringLiteral("none"));
            QVariantList secondPoints;
            for (const auto &v : std::as_const(points)) {
                auto m = v.toMap();
                const int f = m.value(QStringLiteral("frame")).toInt();
                if (f > firstHalfDuration && f < std::max(0, originalDuration - 1)) {
                    m[QStringLiteral("frame")] = f - firstHalfDuration;
                    const int nf = m.value(QStringLiteral("frame")).toInt();
                    if (nf > 0 && nf < secondEndFrame)
                        secondPoints.append(m);
                }
            }
            secondTrack[QStringLiteral("start")] = secondStart;
            secondTrack[QStringLiteral("points")] = secondPoints;
            secondHalfTracks[key] = secondTrack;
        }
        m_keyframeTracks = currentTracks;
        emit keyframeTracksChanged();
        return secondHalfTracks;
    }

    // Must be public to be invokable from QML
    Q_INVOKABLE QStringList availableEasings() const {
        QStringList keys;
        keys << QStringLiteral("none");
        auto funcs = easingFunctions();
        for (auto it = funcs.begin(); it != funcs.end(); ++it)
            keys << it.key();
        keys << QStringLiteral("random") << QStringLiteral("alternate");
        return keys;
    }

    void setEnabled(bool e) {
        m_resolvedCache.clear();
        if (m_enabled != e) {
            m_enabled = e;
            emit enabledChanged();
        }
    }

    Q_INVOKABLE void setParam(const QString &key, const QVariant &val) {
        invalidateCache(key);
        if (m_params[key] != val) {
            m_params[key] = val;

            // アニメーショントラックと同期させ、evaluatedParam() 等が常に最新の静値を返すようにする
            if (m_keyframeTracks.contains(key)) {
                QVariant trackVar = m_keyframeTracks.value(key);
                if (isStructuredTrack(trackVar)) {
                    QVariantMap trackMap = trackVar.toMap();
                    QVariantMap startPoint = trackMap.value(QStringLiteral("start")).toMap();
                    // 開始フレーム(0)の値を更新
                    if (startPoint.value(QStringLiteral("frame")).toInt() == 0) {
                        startPoint[QStringLiteral("value")] = val;
                        trackMap[QStringLiteral("start")] = startPoint;
                        m_keyframeTracks[key] = trackMap;
                        emit keyframeTracksChanged();
                    }
                }
            }

            emit paramsChanged();
            emit paramChanged(key, val);
        }
    }

    Q_INVOKABLE void setKeyframe(const QString &paramName, int frame, const QVariant &value, const QVariantMap &options) {
        invalidateCache(paramName);
        const QVariant fallback = m_params.value(paramName);
        QVariantMap track = normalizeTrackForDuration(m_keyframeTracks.value(paramName), fallback, inferredDurationForTrack(m_keyframeTracks.value(paramName)));

        QVariantMap start = track.value(QStringLiteral("start")).toMap();
        QVariantList points = track.value(QStringLiteral("points")).toList();
        const QString interp = options.value(QStringLiteral("interp"), QStringLiteral("none")).toString();

        const int startFrame = start.value(QStringLiteral("frame")).toInt();

        if (frame <= startFrame) {
            start[QStringLiteral("value")] = value;
            start[QStringLiteral("interp")] = options.value(QStringLiteral("interp"), start.value(QStringLiteral("interp"), QStringLiteral("none")));

            m_params[paramName] = value; // ベース値も同期

            track[QStringLiteral("start")] = start;
            m_keyframeTracks[paramName] = track;
            emit keyframeTracksChanged();
            return;
        }

        QVariantMap kf;
        kf[QStringLiteral("frame")] = frame;
        kf[QStringLiteral("value")] = value;
        kf[QStringLiteral("interp")] = interp;
        if (options.contains(QStringLiteral("points")))
            kf[QStringLiteral("points")] = options.value(QStringLiteral("points"));
        if (options.contains(QStringLiteral("modeParams")))
            kf[QStringLiteral("modeParams")] = options.value(QStringLiteral("modeParams"));

        bool updated = false;
        for (int i = 0; i < points.size(); ++i) {
            if (points[i].toMap().value(QStringLiteral("frame")).toInt() == frame) {
                points[i] = kf;
                updated = true;
                break;
            }
        }
        if (!updated)
            points.append(kf);

        track[QStringLiteral("points")] = sortPoints(points);
        m_keyframeTracks[paramName] = track;
        emit keyframeTracksChanged();
    }

    Q_INVOKABLE void removeKeyframe(const QString &paramName, int frame) {
        invalidateCache(paramName);
        const QVariant fallback = m_params.value(paramName);
        QVariantMap track = normalizeTrackForDuration(m_keyframeTracks.value(paramName), fallback, inferredDurationForTrack(m_keyframeTracks.value(paramName)));

        const int startFrame = track.value(QStringLiteral("start")).toMap().value(QStringLiteral("frame")).toInt();
        // 開始点は削除不可
        if (frame <= startFrame)
            return;
        QVariantList points = track.value(QStringLiteral("points")).toList(), next;
        for (const auto &v : std::as_const(points))
            if (v.toMap().value(QStringLiteral("frame")).toInt() != frame)
                next.append(v);
        track[QStringLiteral("points")] = next;
        m_keyframeTracks[paramName] = track;
        emit keyframeTracksChanged();
    }

    Q_INVOKABLE QVariantMap evaluatedParams(int frame, double fps = 60.0) const {
        QVariantMap out;
        // 全てのキーを網羅するために m_params から開始
        auto keys = m_params.keys();
        for (const QString &paramName : std::as_const(keys)) {
            out[paramName] = evaluatedParam(paramName, frame, fps);
        }
        return out;
    }

    Q_INVOKABLE QVariant evaluatedParam(const QString &paramName, int frame, double fps = 60.0) const {
        const QVariant fallback = m_params.value(paramName);
        if (!m_keyframeTracks.contains(paramName))
            return fallback;

        // 1. キャッシュされた解決済みトラックを使用して評価（ヒープ割り当てを回避）
        if (!m_resolvedCache.contains(paramName)) {
            const QVariant raw = m_keyframeTracks.value(paramName);
            if (isStructuredTrack(raw)) {
                int d = (m_lastDuration > 0) ? m_lastDuration : inferredDurationForTrack(raw);
                QVariantMap normalized = normalizeTrackForDuration(raw, fallback, d);
                m_resolvedCache.insert(paramName, flattenStructuredTrack(normalized));
            } else {
                m_resolvedCache.insert(paramName, sortPoints(raw.toList()));
            }
        }
        QVariant baseValue = evaluateTrack(m_resolvedCache.value(paramName), frame, fallback);

        // 2. パラメータ自体がエクスプレッション定義かチェック（簡易実装）
        QString strVal = m_params.value(paramName).toString();
        if (strVal.startsWith(QStringLiteral("="))) {
            // "=time*100" -> "time*100"
            std::string expr = strVal.mid(1).toStdString();
            double time = (fps > 0.0) ? frame / fps : 0.0;
            return AviQtl::Scripting::LuaHost::instance().evaluate(expr, time, 0, baseValue.toDouble());
        }

        return baseValue;
    }

    // フェーズ3: 数値パラメータ用 QVariant フリー評価 API (public)
    // C++側 ECS 同期パスから呼ぶ。m_resolvedCache を evaluatedParam と共有する
    float evaluatedParamFloat(const QString &paramName, int frame, float fallback = 0.0f, double fps = 60.0) const {
        Q_UNUSED(fps)
        const QVariant fb = m_params.value(paramName);
        const float fbF = fb.isValid() ? static_cast<float>(fb.toDouble()) : fallback;
        if (!m_keyframeTracks.contains(paramName))
            return fbF;
        if (!m_resolvedCache.contains(paramName)) {
            const QVariant raw = m_keyframeTracks.value(paramName);
            if (isStructuredTrack(raw)) {
                const int d = (m_lastDuration > 0) ? m_lastDuration : inferredDurationForTrack(raw);
                m_resolvedCache.insert(paramName, flattenStructuredTrack(normalizeTrackForDuration(raw, fb, d)));
            } else {
                m_resolvedCache.insert(paramName, sortPoints(raw.toList()));
            }
        }
        return evaluateTrackFloat(m_resolvedCache.value(paramName), frame, fbF);
    }

    // bool パラメータ用: float 評価 → 0.5 閾値で bool に変換
    bool evaluatedParamBool(const QString &paramName, int frame, bool fallback = false, double fps = 60.0) const {
        const QVariant fb = m_params.value(paramName);
        if (!m_keyframeTracks.contains(paramName))
            return fb.isValid() ? fb.toBool() : fallback;
        return evaluatedParamFloat(paramName, frame, fallback ? 1.0f : 0.0f, fps) > 0.5f;
    }

    void setKeyframeTracks(const QVariantMap &tracks) {
        m_keyframeTracks = tracks;
        emit keyframeTracksChanged();
    }

    void invalidateCache(const QString &paramName) const {
        if (!paramName.isEmpty()) {
            m_resolvedCache.remove(paramName);
        }
    }

  signals:
    void enabledChanged();
    void paramsChanged();
    void paramChanged(const QString &key, const QVariant &val);
    void keyframeTracksChanged();

  private:
    QString m_id;
    QString m_name;
    QString m_kind;
    QStringList m_categories;
    bool m_enabled;
    QVariantMap m_params;
    QString m_qmlSource;
    QVariantMap m_uiDefinition;
    QVariantMap m_keyframeTracks; // パラメータ名 -> QVariantList[{frame,value,interp}]

    mutable int m_lastDuration = -1;
    mutable QHash<QString, QVariantList> m_resolvedCache;
};
} // namespace AviQtl::UI