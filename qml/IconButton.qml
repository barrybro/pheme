import QtQuick
import QtQuick.Controls

// A borderless toolbar button showing a tinted SVG, like an NSToolbarItem.
ToolButton {
    id: control
    property string iconName
    property color tint: Theme.secondaryText
    property string tip

    implicitWidth: 32
    implicitHeight: 28
    padding: 4
    focusPolicy: Qt.NoFocus
    display: AbstractButton.IconOnly
    icon.source: iconName ? "qrc:/qt/qml/app/pheme/resources/icons/" + iconName + ".svg" : ""
    icon.width: 18
    icon.height: 18
    icon.color: enabled ? (checked ? Theme.accent : tint) : Theme.tertiaryText

    background: Rectangle {
        radius: 6
        color: control.down ? Theme.inactiveSelection : (control.hovered ? Theme.hover : "transparent")
    }

    ToolTip.visible: hovered && tip.length > 0
    ToolTip.delay: 600
    ToolTip.text: tip
}
