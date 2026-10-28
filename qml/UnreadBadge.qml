import QtQuick

// The pill-shaped unread count from NetNewsWire's UnreadCountView.
Rectangle {
    property int count: 0
    property bool selected: false

    visible: count > 0
    implicitWidth: Math.max(label.implicitWidth + 14, 22)
    implicitHeight: label.implicitHeight + 2
    radius: height / 2
    color: selected ? Qt.rgba(1, 1, 1, 0.25) : Theme.badge

    Text {
        id: label
        anchors.centerIn: parent
        text: count > 9999 ? "9999+" : count
        font.pixelSize: Theme.smallFontSize
        font.weight: Font.DemiBold
        color: parent.selected ? "white" : Theme.badgeText
    }
}
