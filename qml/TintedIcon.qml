import QtQuick
import QtQuick.Controls.impl as Impl

// One of the bundled monochrome SVGs, drawn in a color.
Impl.IconImage {
    property string iconName
    property real size: 16

    source: iconName ? "qrc:/qt/qml/app/pheme/resources/icons/" + iconName + ".svg" : ""
    sourceSize.width: size
    sourceSize.height: size
    width: size
    height: size
}
