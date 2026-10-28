import QtQuick

// The pill-shaped unread count from NetNewsWire's UnreadCountView, or just a dot.
Rectangle {
    property int count: 0
    property bool selected: false
    /// A dot instead of the number.
    property bool dot: false

    visible: count > 0
    implicitWidth: dot ? 8 : Math.max(label.implicitWidth + 14, 22)
    implicitHeight: dot ? 8 : label.implicitHeight + 2
    radius: height / 2
    color: selected ? Qt.rgba(1, 1, 1, 0.25) : Theme.badge

    Text {
        id: label
        visible: !parent.dot
        anchors.centerIn: parent
        text: count > 9999 ? "9999+" : count
        font.pixelSize: Theme.smallFontSize
        font.weight: Font.DemiBold
        color: parent.selected ? "white" : Theme.badgeText
    }
}
