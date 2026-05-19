#include "audio_plugin_manager.hpp"
#include "../../core/include/settings_manager.hpp"
#include <QCoreApplication>
#include <QDebug>
#include <QDir>
#include <QDirIterator>
#include <QElapsedTimer>
#include <QFile>
#include <QProcess>
#include <QSet>
#include <QStandardPaths>
#include <QString>
#include <QtConcurrent/QtConcurrent>
#include <algorithm>
#include <cstring>
#include <utility>
#include <vector>

#include <CarlaNativePlugin.h>

namespace AviQtl::Engine::Plugin {

namespace {

auto mapFormatToCarlaType(const QString &format, CarlaBackend::PluginType &ptype) -> bool {
    if (format == QStringLiteral("LADSPA")) {
        ptype = CarlaBackend::PLUGIN_LADSPA;
        return true;
    }
    if (format == QStringLiteral("DSSI")) {
        ptype = CarlaBackend::PLUGIN_DSSI;
        return true;
    }
    if (format == QStringLiteral("LV2")) {
        ptype = CarlaBackend::PLUGIN_LV2;
        return true;
    }
    if (format == QStringLiteral("VST2")) {
        ptype = CarlaBackend::PLUGIN_VST2;
        return true;
    }
    if (format == QStringLiteral("VST3")) {
        ptype = CarlaBackend::PLUGIN_VST3;
        return true;
    }
    return false;
}

auto safeQString(const char *s) -> QString { return (s != nullptr) ? QString::fromUtf8(s) : QString(); }

auto clamp01(float v) -> float {
    if (v < 0.0F) {
        return 0.0F;
    }
    if (v > 1.0F) {
        return 1.0F;
    }
    return v;
}

class CarlaHostedPlugin final : public IAudioPlugin {
  public:
    CarlaHostedPlugin(PluginInfo info) : m_info(std::move(info)) {}

    ~CarlaHostedPlugin() override { release(); }

    void ensureBuffers(int frames) {
        if (frames <= 0) {
            return;
        }
        if (std::cmp_less(m_inL.size(), frames)) {
            m_inL.resize(frames, 0.0F);
            m_inR.resize(frames, 0.0F);
            m_outL.resize(frames, 0.0F);
            m_outR.resize(frames, 0.0F);
        }
    }

    void deinterleave(const float *src, int frames) {
        ensureBuffers(frames);
        for (int i = 0; i < frames; ++i) {
            m_inL[i] = src[(i * 2) + 0];
            m_inR[i] = src[(i * 2) + 1];
        }
    }

    void interleave(float *dst, int frames) const {
        for (int i = 0; i < frames; ++i) {
            dst[(i * 2) + 0] = m_outL[i];
            dst[(i * 2) + 1] = m_outR[i];
        }
    }

    // NativeHostDescriptor コールバック (static)
    static auto s_getBufferSize(NativeHostHandle h) -> uint32_t { return static_cast<uint32_t>(static_cast<CarlaHostedPlugin *>(h)->m_maxBlockSize); }
    static auto s_getSampleRate(NativeHostHandle h) -> double { return static_cast<CarlaHostedPlugin *>(h)->m_sampleRate; }
    static auto s_isOffline(NativeHostHandle /*unused*/) -> bool { return false; }
    static auto s_getTimeInfo(NativeHostHandle h) -> const NativeTimeInfo * { return &static_cast<CarlaHostedPlugin *>(h)->m_timeInfo; }
    static auto s_writeMidiEvent(NativeHostHandle /*unused*/, const NativeMidiEvent * /*unused*/) -> bool { return false; }
    static void s_uiParameterChanged(NativeHostHandle /*unused*/, uint32_t /*unused*/, float /*unused*/) {}
    static void s_uiMidiProgramChanged(NativeHostHandle /*unused*/, uint8_t /*unused*/, uint32_t /*unused*/, uint32_t /*unused*/) {}
    static void s_uiCustomDataChanged(NativeHostHandle /*unused*/, const char * /*unused*/, const char * /*unused*/) {}
    static void s_uiClosed(NativeHostHandle /*unused*/) {}
    static auto s_uiOpenFile(NativeHostHandle /*unused*/, bool /*unused*/, const char * /*unused*/, const char * /*unused*/) -> const char * { return nullptr; }
    static auto s_uiSaveFile(NativeHostHandle /*unused*/, bool /*unused*/, const char * /*unused*/, const char * /*unused*/) -> const char * { return nullptr; }
    static auto s_dispatcher(NativeHostHandle /*unused*/, NativeHostDispatcherOpcode /*unused*/, int32_t /*unused*/, intptr_t /*unused*/, void * /*unused*/, float /*unused*/) -> intptr_t { return 0; }

    auto load(const QString &path, int index = 0) -> bool override {
        Q_UNUSED(index)

        CarlaBackend::PluginType ptype = CarlaBackend::PLUGIN_NONE;
        if (!mapFormatToCarlaType(m_info.format, ptype)) {
            qWarning() << "[CarlaHostedPlugin] 未対応フォーマット:" << m_info.format;
            return false;
        }

        const auto &sm = AviQtl::Core::SettingsManager::instance();
        if (m_sampleRate <= 1.0) {
            m_sampleRate = sm.value(QStringLiteral("defaultProjectSampleRate"), 48000).toDouble();
        }
        if (m_maxBlockSize <= 0) {
            m_maxBlockSize = sm.value(QStringLiteral("audioPluginMaxBlockSize"), 512).toInt();
        }

        m_uiNameBuf = m_info.name.toUtf8();

        m_hostDesc.handle = static_cast<NativeHostHandle>(this);
        m_hostDesc.resourceDir = "/usr/share/carla/resources";
        m_hostDesc.uiName = m_uiNameBuf.constData();
        m_hostDesc.uiParentId = 0;
        m_hostDesc.get_buffer_size = s_getBufferSize;
        m_hostDesc.get_sample_rate = s_getSampleRate;
        m_hostDesc.is_offline = s_isOffline;
        m_hostDesc.get_time_info = s_getTimeInfo;
        m_hostDesc.write_midi_event = s_writeMidiEvent;
        m_hostDesc.ui_parameter_changed = s_uiParameterChanged;
        m_hostDesc.ui_midi_program_changed = s_uiMidiProgramChanged;
        m_hostDesc.ui_custom_data_changed = s_uiCustomDataChanged;
        m_hostDesc.ui_closed = s_uiClosed;
        m_hostDesc.ui_open_file = s_uiOpenFile;
        m_hostDesc.ui_save_file = s_uiSaveFile;
        m_hostDesc.dispatcher = s_dispatcher;

        m_descriptor = carla_get_native_rack_plugin();
        if (m_descriptor == nullptr) {
            qWarning() << "[CarlaHostedPlugin] carla_get_native_rack_plugin が nullptr:" << m_info.name;
            return false;
        }

        m_nativeHandle = m_descriptor->instantiate(&m_hostDesc);
        if (m_nativeHandle == nullptr) {
            qWarning() << "[CarlaHostedPlugin] instantiate() が nullptr:" << m_info.name;
            m_descriptor = nullptr;
            return false;
        }

        m_hostHandle = carla_create_native_plugin_host_handle(m_descriptor, m_nativeHandle);
        if (m_hostHandle == nullptr) {
            qWarning() << "[CarlaHostedPlugin] carla_create_native_plugin_host_handle が nullptr:" << m_info.name;
            m_descriptor->cleanup(m_nativeHandle);
            m_nativeHandle = nullptr;
            m_descriptor = nullptr;
            return false;
        }

        // Native Rack はデフォルトで CONTINUOUS_RACK かつステレオ強制のため設定不要
        const QByteArray filename = path.toUtf8();
        const QByteArray name = m_info.name.toUtf8();

        // LV2: carla-discovery は label を "bundle.lv2/http://..." 形式で出力する。
        // carla_add_plugin に渡す label は純粋な URI でなければならないため抽出する。
        QString lv2UriStr = m_info.label;
        if (ptype == CarlaBackend::PLUGIN_LV2) {
            const int dotLv2 = static_cast<int>(lv2UriStr.indexOf(QLatin1String(".lv2/")));
            if (dotLv2 >= 0) {
                lv2UriStr = lv2UriStr.mid(dotLv2 + 5);
            }
        }
        const QByteArray label = lv2UriStr.toUtf8();

        m_loaded = carla_add_plugin(m_hostHandle, CarlaBackend::BINARY_POSIX64, ptype, filename.isEmpty() ? nullptr : filename.constData(), name.isEmpty() ? nullptr : name.constData(), label.isEmpty() ? nullptr : label.constData(), m_info.uniqueId,
                                    nullptr, CarlaBackend::PLUGIN_OPTIONS_NULL);

        if (!m_loaded) {
            qWarning() << "[CarlaHostedPlugin] carla_add_plugin failed:" << m_info.name << m_info.path;
            carla_host_handle_free(m_hostHandle);
            m_hostHandle = nullptr;
            m_descriptor->cleanup(m_nativeHandle);
            m_nativeHandle = nullptr;
            m_descriptor = nullptr;
            return false;
        }

        carla_set_active(m_hostHandle, m_pluginId, true);
        m_descriptor->activate(m_nativeHandle);
        ensureBuffers(m_maxBlockSize);
        qDebug() << "[CarlaHostedPlugin] NativePlugin ロード完了:" << m_info.name;
        return true;
    }

    void prepare(double sampleRate, int maxBlockSize) override { // NOLINT(bugprone-easily-swappable-parameters)
        m_sampleRate = sampleRate > 1.0 ? sampleRate : 48000.0;
        m_maxBlockSize = maxBlockSize > 0 ? maxBlockSize : 512;
        ensureBuffers(m_maxBlockSize);
        // サンプルレート/バッファサイズは NativeHostDescriptor コールバックで動的に返す
    }

    void process(float *buf, int frameCount) override {
        if (!m_loaded || m_descriptor == nullptr || m_nativeHandle == nullptr || buf == nullptr || frameCount <= 0) {
            return;
        }

        deinterleave(buf, frameCount);

        std::fill(m_outL.begin(), m_outL.begin() + frameCount, 0.0F);
        std::fill(m_outR.begin(), m_outR.begin() + frameCount, 0.0F);

        float *inBufs[2] = {m_inL.data(), m_inR.data()};
        float *outBufs[2] = {m_outL.data(), m_outR.data()};

        m_descriptor->process(m_nativeHandle, inBufs, outBufs, static_cast<uint32_t>(frameCount), nullptr, 0);

        interleave(buf, frameCount);
    }

    [[nodiscard]] auto active() const -> bool override { return m_loaded; }

    void release() override {
        if (m_descriptor != nullptr && m_nativeHandle != nullptr) {
            m_descriptor->deactivate(m_nativeHandle);
        }

        if (m_hostHandle != nullptr) {
            carla_host_handle_free(m_hostHandle);
            m_hostHandle = nullptr;
        }

        if (m_descriptor != nullptr && m_nativeHandle != nullptr) {
            m_descriptor->cleanup(m_nativeHandle);
            m_nativeHandle = nullptr;
        }

        m_descriptor = nullptr;
        m_loaded = false;
        m_inL.clear();
        m_inR.clear();
        m_outL.clear();
        m_outR.clear();
    }

    [[nodiscard]] auto name() const -> QString override { return m_info.name; }
    [[nodiscard]] auto format() const -> QString override { return m_info.format; }

    [[nodiscard]] auto paramCount() const -> int override {
        if (!m_loaded || m_hostHandle == nullptr) {
            return 0;
        }
        return static_cast<int>(carla_get_parameter_count(m_hostHandle, m_pluginId));
    }

    [[nodiscard]] auto paramName(int i) const -> QString override {
        if (!m_loaded || m_hostHandle == nullptr || i < 0) {
            return {};
        }
        const CarlaParameterInfo *info = carla_get_parameter_info(m_hostHandle, m_pluginId, static_cast<uint32_t>(i));
        return (info != nullptr) ? safeQString(info->name) : QString{};
    }

    [[nodiscard]] auto getParam(int i) const -> float override {
        if (!m_loaded || m_hostHandle == nullptr || i < 0) {
            return 0.0F;
        }
        return carla_get_current_parameter_value(m_hostHandle, m_pluginId, static_cast<uint32_t>(i));
    }

    void setParam(int i, float v) override {
        if (!m_loaded || m_hostHandle == nullptr || i < 0) {
            return;
        }
        carla_set_parameter_value(m_hostHandle, m_pluginId, static_cast<uint32_t>(i), clamp01(v));
    }

    [[nodiscard]] auto getParamInfo(int i) const -> ParamInfo override {
        ParamInfo out;
        if (!m_loaded || m_hostHandle == nullptr || i < 0) {
            return out;
        }
        const auto pid = static_cast<uint32_t>(i);
        const CarlaParameterInfo *info = carla_get_parameter_info(m_hostHandle, m_pluginId, pid);
        if (info != nullptr) {
            out.name = safeQString(info->name);
        }
        out.defaultValue = carla_get_default_parameter_value(m_hostHandle, m_pluginId, pid);
        out.min = 0.0F;
        out.max = 1.0F;
        return out;
    }

  private:
    const NativePluginDescriptor *m_descriptor = nullptr;
    NativePluginHandle m_nativeHandle = nullptr;
    CarlaHostHandle m_hostHandle = nullptr;
    NativeHostDescriptor m_hostDesc = {};
    NativeTimeInfo m_timeInfo = {};
    QByteArray m_uiNameBuf;
    uint m_pluginId = 0;
    PluginInfo m_info;
    bool m_loaded = false;
    double m_sampleRate = 48000.0;
    int m_maxBlockSize = 512;
    std::vector<float> m_inL;
    std::vector<float> m_inR;
    std::vector<float> m_outL;
    std::vector<float> m_outR;
};

auto discoverySearchPaths() -> const QStringList & {
    static QStringList paths = {
        "/usr/lib/carla/carla-discovery-native", "/usr/local/lib/carla/carla-discovery-native", "/usr/lib64/carla/carla-discovery-native", "/usr/bin/carla-discovery-native", "/usr/local/bin/carla-discovery-native",
    };
#if defined(Q_OS_WIN) || (defined(__APPLE__) && !defined(Q_OS_IOS))
    static bool appended = false;
    if (!appended) {
        const QDir appDir(QCoreApplication::applicationDirPath());
#if defined(Q_OS_WIN)
        const QString bundledTool = appDir.filePath(QStringLiteral("carla-discovery-native.exe"));
#else
        const QString bundledTool = appDir.filePath(QStringLiteral("carla-discovery-native"));
#endif
        if (!paths.contains(bundledTool)) {
            paths.prepend(bundledTool);
        }
        appended = true;
    }
#endif
    return paths;
}

struct FormatConfig {
    QString type;
    QString format;
    QString fileFilter;
    bool bundleDir;
};

auto formats() -> const QList<FormatConfig> & {
    static const QList<FormatConfig> list = {
        {.type = "lv2", .format = "LV2", .fileFilter = "*.lv2", .bundleDir = true},     {.type = "vst2", .format = "VST2", .fileFilter = "*.so", .bundleDir = false}, {.type = "vst3", .format = "VST3", .fileFilter = "*.vst3", .bundleDir = true},
        {.type = "clap", .format = "CLAP", .fileFilter = "*.clap", .bundleDir = false}, {.type = "dssi", .format = "DSSI", .fileFilter = "*.so", .bundleDir = false}, {.type = "sf2", .format = "SF2", .fileFilter = "*.sf2", .bundleDir = false},
        {.type = "sfz", .format = "SFZ", .fileFilter = "*.sfz", .bundleDir = false},
    };
    return list;
}

auto toCategoryStr(int cat) -> QString {
    switch (static_cast<CarlaBackend::PluginCategory>(cat)) {
    case CarlaBackend::PLUGIN_CATEGORY_SYNTH:
        return QStringLiteral("Synth");
    case CarlaBackend::PLUGIN_CATEGORY_DELAY:
        return QStringLiteral("Delay");
    case CarlaBackend::PLUGIN_CATEGORY_EQ:
        return QStringLiteral("EQ");
    case CarlaBackend::PLUGIN_CATEGORY_FILTER:
        return QStringLiteral("Filter");
    case CarlaBackend::PLUGIN_CATEGORY_DISTORTION:
        return QStringLiteral("Distortion");
    case CarlaBackend::PLUGIN_CATEGORY_DYNAMICS:
        return QStringLiteral("Dynamics");
    case CarlaBackend::PLUGIN_CATEGORY_MODULATOR:
        return QStringLiteral("Modulator");
    case CarlaBackend::PLUGIN_CATEGORY_UTILITY:
        return QStringLiteral("Utility");
    case CarlaBackend::PLUGIN_CATEGORY_OTHER: // fall through
    default:
        return QStringLiteral("Other");
    }
}

auto normalizeCategoryTitle(QString category) -> QString {
    category = category.trimmed();
    if (category.isEmpty()) {
        return QStringLiteral("Other");
    }
    const QString lower = category.toLower();
    if (lower == QLatin1String("synth") || lower == QStringLiteral("instrument")) {
        return QStringLiteral("Synth");
    }
    if (lower == QLatin1String("delay") || lower == QStringLiteral("reverb")) {
        return QStringLiteral("Delay");
    }
    if (lower == QStringLiteral("eq")) {
        return QStringLiteral("EQ");
    }
    if (lower == QStringLiteral("filter")) {
        return QStringLiteral("Filter");
    }
    if (lower == QStringLiteral("distortion")) {
        return QStringLiteral("Distortion");
    }
    if (lower == QStringLiteral("dynamics")) {
        return QStringLiteral("Dynamics");
    }
    if (lower == QLatin1String("modulator") || lower == QStringLiteral("modulation")) {
        return QStringLiteral("Modulator");
    }
    if (lower == QLatin1String("utility") || lower == QLatin1String("tools") || lower == QStringLiteral("tool")) {
        return QStringLiteral("Utility");
    } // NOLINT(bugprone-easily-swappable-parameters)
    if (lower == QLatin1String("other") || lower == QLatin1String("unknown") || lower == QLatin1String("misc") || lower == QLatin1String("none") || lower == QStringLiteral("null")) {
        return QStringLiteral("Other");
    }
    return category;
}

auto normalizePluginName(QString name, const QString &label, const QString &filePath) -> QString {
    name = name.trimmed();
    if (!name.isEmpty()) {
        return name;
    }
    const QString l = label.trimmed();
    if (!l.isEmpty()) {
        return l;
    }
    return QFileInfo(filePath).completeBaseName().trimmed();
}

auto normalizePluginLabel(QString label, const QString &name) -> QString {
    label = label.trimmed();
    return label.isEmpty() ? name.trimmed() : label;
}

auto categoryRank(const QString &category) -> int {
    const QString c = normalizeCategoryTitle(category);
    if (c == QStringLiteral("Filter")) {
        return 0;
    }
    if (c == QStringLiteral("EQ")) {
        return 1;
    }
    if (c == QStringLiteral("Dynamics")) {
        return 2;
    }
    if (c == QStringLiteral("Delay")) {
        return 3;
    }
    if (c == QStringLiteral("Distortion")) {
        return 4;
    }
    if (c == QStringLiteral("Modulator")) {
        return 5;
    }
    if (c == QStringLiteral("Utility")) {
        return 6;
    } // NOLINT(bugprone-easily-swappable-parameters)
    if (c == QStringLiteral("Synth")) {
        return 7;
    }
    return 100;
}

auto parseDiscoveryOutput(const QString &output, const QString &format, const QString &filePath) -> QList<PluginInfo> {
    QList<PluginInfo> results;
    PluginInfo current;
    bool inBlock = false;

    for (const QString &rawLine : output.split('\n')) {
        const QString line = rawLine.trimmed();
        if (!line.startsWith(QStringLiteral("carla-discovery::"))) {
            continue;
        }
        const QStringList parts = line.split(QStringLiteral("::"));
        if (parts.size() < 2) {
            continue;
        }
        const QString &key = parts.at(1);
        const QString val = parts.size() >= 3 ? parts.mid(2).join(QStringLiteral("::")) : QLatin1String("");

        if (key == QLatin1String("begin") || key == QStringLiteral("init")) {
            current = PluginInfo{};
            current.format = format;
            current.path = filePath;
            inBlock = true;
        } else if (!inBlock) {
            continue;
        } else if (key == QStringLiteral("name")) {
            current.name = val;
        } else if (key == QStringLiteral("label")) {
            current.label = val;
        } else if (key == QStringLiteral("maker")) {
            current.maker = val;
        } else if (key == QStringLiteral("uniqueId")) {
            current.uniqueId = val.toLongLong();
        } else if (key == QStringLiteral("category")) {
            // 旧APIは整数、新APIは文字列("none","filter"等)で返す
            bool catIsInt = false;
            const int catInt = val.toInt(&catIsInt);
            current.category = (catIsInt && val.trimmed() != QStringLiteral("0")) ? toCategoryStr(catInt) : normalizeCategoryTitle(val);
        } else if (key == QStringLiteral("audio.ins")) {
            current.audioIns = val.toInt();
        } else if (key == QStringLiteral("audio.outs")) {
            current.audioOuts = val.toInt();
        } else if (key == QStringLiteral("end")) {
            current.name = normalizePluginName(current.name, current.label, filePath);
            current.label = normalizePluginLabel(current.label, current.name);
            current.category = normalizeCategoryTitle(current.category);
            if (!current.name.isEmpty()) {
                current.id = QString(QStringLiteral("%1:%2:%3")).arg(current.format, current.label, QString::number(current.uniqueId));
                results.append(current);
            }
            inBlock = false;
        }
    } // NOLINT(bugprone-easily-swappable-parameters)
    return results;
}

// 1ファイルに対してディスカバリを実行
// stdout の逐次読み出しでバッファ詰まりデッドロックを回避
// stdin を /dev/null に向けて子プロセスによる端末状態の汚染を防ぐ
auto runDiscovery(const QString &tool, const QString &type, const QString &format, const QString &target, std::atomic<bool> &stopFlag, int waitStartedMs, int timeoutMs, int waitReadyReadMs, int waitFinishedMs) -> QList<PluginInfo> {
    if (stopFlag) {
        return {};
    }

    QProcess proc;
    proc.setStandardInputFile(QProcess::nullDevice());
    proc.setProcessChannelMode(QProcess::SeparateChannels);
    proc.start(tool, {type, target});

    if (!proc.waitForStarted(waitStartedMs)) {
        qWarning() << "[Discovery] 起動失敗:" << target;
        return {};
    }

    QByteArray output;
    QElapsedTimer timer;
    timer.start();

    while (!stopFlag) {
        proc.waitForReadyRead(waitReadyReadMs);
        output += proc.readAllStandardOutput();
        if (proc.state() == QProcess::NotRunning) {
            break;
        }
        if (timer.elapsed() > timeoutMs) {
            qWarning() << "[Discovery] タイムアウト:" << target;
            proc.kill();
            proc.waitForFinished(waitFinishedMs);
            break;
        }
    }
    output += proc.readAllStandardOutput();

    QByteArray errOutput = proc.readAllStandardError();
    if (output.isEmpty() && errOutput.contains("carla-discovery::")) {
        output = errOutput;
    } else if (!errOutput.isEmpty() && output.isEmpty()) {
        qDebug() << "[Discovery] エラー出力:" << target << errOutput.left(200).trimmed();
    }
    return parseDiscoveryOutput(QString::fromUtf8(output), format, target);
}

auto isDiscoveryTypeSupported(const QString &tool, const QString &type) -> bool {
    QProcess probe;
    probe.setStandardInputFile(QProcess::nullDevice());
    probe.setProcessChannelMode(QProcess::SeparateChannels);
    probe.start(tool, {type, "__probe__"});
    if (!probe.waitForFinished(3000)) {
        probe.kill();
    }
    return !probe.readAllStandardError().contains("invalid string type");
}

auto discoverFormat(const QString &tool, const FormatConfig &cfg, std::atomic<bool> &stopFlag) -> QList<PluginInfo> {
    if (!isDiscoveryTypeSupported(tool, cfg.type)) {
        qWarning() << "[AudioPluginManager]" << cfg.format << "はインストール済みCarlaバージョンで未対応のためスキップ";
        return {};
    }

    // 起動期に不変な設定値を一括読み出し（繰り返し SettingsManager を叩かない）
    const auto &sm = AviQtl::Core::SettingsManager::instance();
    QStringList searchPaths = sm.value(QStringLiteral("pluginPaths") + cfg.format, QStringList()).toStringList();
    const int discoveryThreads = sm.value(QStringLiteral("pluginDiscoveryThreads"), std::max(2, QThread::idealThreadCount() - 1)).toInt();
    const int waitStartedMs = sm.value(QStringLiteral("pluginDiscoveryWaitStartedMs"), 3000).toInt();
    const int timeoutMs = sm.value(QStringLiteral("pluginDiscoveryTimeoutMs"), 5000).toInt();
    const int waitReadyReadMs = sm.value(QStringLiteral("pluginDiscoveryWaitReadyReadMs"), 200).toInt();
    const int waitFinishedMs = sm.value(QStringLiteral("pluginDiscoveryWaitFinishedMs"), 1000).toInt();

    QStringList targets;
    QSet<QString> visited;

    for (const QString &dirPath : std::as_const(searchPaths)) {
        if (stopFlag) {
            break;
        }
        QDir d(dirPath);
        if (!d.exists()) {
            continue;
        }

        const QString canonical = d.canonicalPath();
        if (visited.contains(canonical)) {
            continue;
        }
        visited.insert(canonical);

        if (cfg.bundleDir) {
            const QFileInfoList entries = d.entryInfoList({cfg.fileFilter}, QDir::Dirs | QDir::NoDotAndDotDot);
            for (const QFileInfo &fi : std::as_const(entries)) {
                targets << fi.absoluteFilePath();
            }
        } else {
            QDirIterator it(dirPath, {cfg.fileFilter}, QDir::Files, QDirIterator::Subdirectories);
            while (it.hasNext()) {
                targets << it.next();
            }
        }
    }

    if (cfg.type == QStringLiteral("lv2")) {
        qDebug() << "[AudioPluginManager]" << cfg.format << "バンドル" << targets.size() << "個を検出";
    } else {
        qDebug() << "[AudioPluginManager]" << cfg.format << "ファイル" << targets.size() << "個を検出";
    }

    QList<PluginInfo> all;
    QMutex mutex;

    QThreadPool pool;
    pool.setMaxThreadCount(discoveryThreads);

    QtConcurrent::blockingMap(&pool, targets, [&](const QString &target) -> void {
        if (stopFlag) {
            return;
        }
        QList<PluginInfo> res = runDiscovery(tool, cfg.type, cfg.format, target, stopFlag, waitStartedMs, timeoutMs, waitReadyReadMs, waitFinishedMs);
        if (!res.isEmpty()) {
            QMutexLocker lock(&mutex);
            all += res;
        }
    });

    return all;
}

} // namespace

auto AudioPluginManager::instance() -> AudioPluginManager & {
    static AudioPluginManager inst;
    return inst;
}

AudioPluginManager::AudioPluginManager(QObject *parent) : QObject(parent) {}

AudioPluginManager::~AudioPluginManager() { stopScan(); }

void AudioPluginManager::stopScan() { m_stopRequested = true; }

void AudioPluginManager::initialize() {
    if (m_initialized) {
        return;
    }
    m_initialized = true;

    // スキャンを非同期で開始し、完了後にシグナルを発行する
    // waitForFinished() はメインスレッドをブロックするため削除
    (void)QtConcurrent::run([this] -> void {
        scanPlugins();
        emit pluginsReady(static_cast<int>(m_plugins.size()));
    });
}

void AudioPluginManager::scanPlugins() {
    bool expected = false;
    if (!m_scanning.compare_exchange_strong(expected, true)) {
        qDebug() << "[AudioPluginManager] スキャンは既に実行中";
        return;
    }
    m_stopRequested = false;

    QString tool;
    for (const QString &p : std::as_const(discoverySearchPaths())) {
        if (QFile::exists(p)) {
            tool = p;
            break;
        }
    }
    if (tool.isEmpty()) {
        tool = QStandardPaths::findExecutable(QStringLiteral("carla-discovery-native"));
    }
    if (tool.isEmpty()) {
        qWarning() << "[AudioPluginManager] carla-discovery-native が見つかりません";
        qWarning() << "[AudioPluginManager] 検索パス:" << discoverySearchPaths();
        m_scanning = false;
        return;
    }
    qDebug() << "[AudioPluginManager] ディスカバリツール:" << tool;

    QList<PluginInfo> newPlugins;
    QHash<QString, PluginInfo> newMap;

    for (const FormatConfig &cfg : std::as_const(formats())) {
        if (m_stopRequested) {
            break;
        }
        bool isEnabled = AviQtl::Core::SettingsManager::instance().value(QStringLiteral("pluginEnable") + cfg.format, true).toBool();
        if (!isEnabled) {
            continue;
        }

        qDebug() << "[AudioPluginManager] スキャン中:" << cfg.format;
        const QList<PluginInfo> found = discoverFormat(tool, cfg, m_stopRequested);
        qDebug() << "[AudioPluginManager]" << cfg.format << "→" << found.size() << "個";
        for (const PluginInfo &p : std::as_const(found)) {
            if (!newMap.contains(p.id)) {
                newPlugins.append(p);
                newMap.insert(p.id, p);
            }
        }
    }

    {
        QMutexLocker lock(&m_pluginsMutex);
        m_plugins = std::move(newPlugins);
        m_pluginMap = std::move(newMap);
    }
    qDebug() << "[AudioPluginManager] 検出プラグイン数:" << m_plugins.size();
    m_scanning = false;
}

auto AudioPluginManager::getPluginList() const -> QVariantList {
    QMutexLocker lock(&m_pluginsMutex);
    QVariantList list;
    list.reserve(m_plugins.size());
    for (const auto &info : std::as_const(m_plugins)) {
        QVariantMap map;
        map.insert(QStringLiteral("id"), info.id);
        map.insert(QStringLiteral("name"), info.name);
        map.insert(QStringLiteral("format"), info.format);
        map.insert(QStringLiteral("category"), info.category);
        map.insert(QStringLiteral("maker"), info.maker);
        map.insert(QStringLiteral("audioIns"), info.audioIns);
        map.insert(QStringLiteral("audioOuts"), info.audioOuts);
        list.append(map);
    }
    return list;
}

auto AudioPluginManager::getCategories() const -> QVariantList {
    QMutexLocker lock(&m_pluginsMutex);
    QStringList cats;
    for (const auto &info : std::as_const(m_plugins)) {
        const QString c = normalizeCategoryTitle(info.category);
        if (!cats.contains(c)) {
            cats.append(c);
        }
    }
    std::ranges::sort(cats, [](const QString &a, const QString &b) -> bool {
        const int ra = categoryRank(a);
        const int rb = categoryRank(b);
        return ra != rb ? ra < rb : a.toLower() < b.toLower();
    });
    QVariantList list;
    for (const auto &c : std::as_const(cats)) {
        list.append(c);
    }
    return list;
}

auto AudioPluginManager::getPluginsInCategory(const QString &category) const -> QVariantList {
    QMutexLocker lock(&m_pluginsMutex);
    const QString wanted = normalizeCategoryTitle(category);
    QList<PluginInfo> matched;
    for (const auto &info : std::as_const(m_plugins)) {
        if (normalizeCategoryTitle(info.category) == wanted) {
            matched.append(info);
        }
    }
    std::ranges::sort(matched, [](const PluginInfo &a, const PluginInfo &b) -> bool { return a.name.toLower() < b.name.toLower(); });
    QVariantList list;
    for (const auto &info : std::as_const(matched)) {
        QVariantMap map;
        map.insert(QStringLiteral("id"), info.id);
        map.insert(QStringLiteral("name"), normalizePluginName(info.name, info.label, info.path));
        map.insert(QStringLiteral("format"), info.format);
        map.insert(QStringLiteral("maker"), info.maker);
        map.insert(QStringLiteral("category"), normalizeCategoryTitle(info.category));
        list.append(map);
    }
    return list;
}

auto AudioPluginManager::createPlugin(const QString &id) -> std::unique_ptr<IAudioPlugin> {
    QMutexLocker lock(&m_pluginsMutex);
    auto it = std::ranges::find_if(m_plugins, [&](const PluginInfo &info) -> bool { return info.id == id; });

    if (it == m_plugins.end()) {
        qWarning() << "[AudioPluginManager] Plugin not found:" << id;
        return nullptr;
    }

    auto plugin = std::make_unique<CarlaHostedPlugin>(*it);
    if (!plugin->load(it->path, it->index)) {
        qWarning() << "[AudioPluginManager] Failed to load plugin:" << it->name << it->path;
        return nullptr;
    }

    qDebug() << "[AudioPluginManager] Loaded plugin via independent Carla instance:" << it->name << it->format << it->path;
    return plugin;
}

} // namespace AviQtl::Engine::Plugin
