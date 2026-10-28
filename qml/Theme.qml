pragma Singleton
import QtQuick

// Colors and metrics after NetNewsWire's Mac appearance.
QtObject {
    readonly property bool dark: Qt.styleHints.colorScheme === Qt.ColorScheme.Dark

    readonly property color accent: dark ? "#0a84ff" : "#007aff"
    readonly property color star: dark ? "#ffd60a" : "#f5b400"
    readonly property color today: dark ? "#ff9f0a" : "#ff9500"

    readonly property color window: dark ? "#1e1e1e" : "#ffffff"
    readonly property color sidebar: dark ? "#262628" : "#f2f1f6"
    readonly property color toolbar: dark ? "#2a2a2c" : "#f6f6f6"
    readonly property color separator: dark ? "#3a3a3c" : "#dcdcdc"
    readonly property color rowSeparator: dark ? "#323234" : "#ebebeb"

    readonly property color text: dark ? "#e8e8e8" : "#1d1d1f"
    readonly property color secondaryText: dark ? "#9a9a9e" : "#6e6e73"
    readonly property color tertiaryText: dark ? "#6f6f73" : "#a0a0a5"

    // A focused list's selection is the accent color; an unfocused one is gray, as on macOS.
    readonly property color selection: accent
    readonly property color inactiveSelection: dark ? "#3a3a3c" : "#dcdcdf"
    readonly property color hover: dark ? "#2f2f31" : "#ececef"

    readonly property color badge: dark ? "#3d3d40" : "#dcdce0"
    readonly property color badgeText: dark ? "#c8c8cc" : "#555559"

    readonly property real baseFontSize: 13
    readonly property real smallFontSize: Math.floor(baseFontSize * 0.9)
    readonly property int toolbarHeight: 46
}
