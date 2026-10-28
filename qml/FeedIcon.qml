import QtQuick
import QtQuick.Effects

// A feed's icon with rounded corners, or a tinted RSS glyph when Feedly has none.
Item {
    id: root
    property string source
    property real size: 16
    property real radius: 3
    property color tint: Theme.accent

    implicitWidth: size
    implicitHeight: size
    width: size
    height: size

    Image {
        id: image
        anchors.fill: parent
        source: root.source
        sourceSize.width: root.size * 2
        sourceSize.height: root.size * 2
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
        visible: false
    }

    Rectangle {
        id: mask
        anchors.fill: parent
        radius: root.radius
        visible: false
        layer.enabled: true
    }

    MultiEffect {
        anchors.fill: parent
        source: image
        visible: image.status === Image.Ready
        maskEnabled: true
        maskSource: mask
    }

    TintedIcon {
        anchors.centerIn: parent
        visible: image.status !== Image.Ready
        iconName: "feed"
        size: root.size
        color: root.tint
    }
}
