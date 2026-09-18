pragma Singleton
import QtQuick

QtObject {
    id: theme

    readonly property color bg: "#18181b"
    readonly
    property color bgDark: "#09090b"
    readonly property color card: "#27272a"
    readonly
    property color cardHover: "#3f3f46"
    readonly property color border: "#3f3f46"
    readonly
    property color borderMuted: "#27272a"
    readonly property color text: "#f4f4f5"
    readonly
    property color textMuted: "#a1a1aa"
    readonly property color accent: "#6366f1"
    readonly
    property color accentHover: "#4f46e5"
    readonly property color danger: "#ef4444"
    readonly
    property color dangerHover: "#dc2626"
    readonly property color warning: "#f59e0b"
    readonly
    property color success: "#10b981"
    readonly property color playhead: "#ef4444"

        readonly
    property var clipPalette: [
        "#2a4db8",         "#256e3c",         "#6a2db8",         "#b8862a",         "#2ab8a8",         "#b82a5f"      ]

    function clipColor(kind) {
        if (kind < 0) return "#5a5a5a";
        return clipPalette[kind % clipPalette.length];
    }

    readonly property string fontFamily: "Inter, Noto Sans CJK JP, sans-serif"
    readonly
    property string monoFont: "JetBrains Mono, monospace"

    readonly property int radius: 6
    readonly
    property int radiusSm: 4
}
