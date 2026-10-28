import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// The article list, laid out like NetNewsWire's TimelineTableCellView.
Rectangle {
    id: root
    color: Theme.window

    required property var model
    required property var app
    property alias list: list
    property string title: ""
    /// Smart feeds and folders mix feeds, so their rows show feed icons.
    property bool showIcons: true
    property string currentArticleId: ""
    /// The header's leading button: "sidebar" to show the sidebar, "back" to go back
    /// to it in the one-pane layout, or "" for none.
    property string leadingButton: ""

    signal articleSelected(int row, string articleId)
    /// An article was clicked, as opposed to reached with the arrow keys.
    signal articleActivated()
    signal leadingButtonClicked()
    signal statusChanged()
    signal moveFocusLeft()
    signal moveFocusRight()

    function selectRow(row) {
        if (row < 0 || row >= list.count)
            return
        list.currentIndex = row
        list.positionViewAtIndex(row, ListView.Contain)
        const id = model.idAt(row)
        if (id !== currentArticleId) {
            currentArticleId = id
            articleSelected(row, id)
        }
    }

    /// The selected article's row, or -1. The article ID is the selection; ListView's
    /// currentIndex only follows it, because ListView moves currentIndex on its own when
    /// the model resets.
    function currentRow() {
        return currentArticleId.length > 0 ? model.rowForId(currentArticleId) : -1
    }

    /// Re-finds the selected article after the model reloads.
    function restoreSelection() {
        list.currentIndex = currentRow()
    }

    function clearSelection() {
        list.currentIndex = -1
        currentArticleId = ""
        articleSelected(-1, "")
    }

    function selectNextUnread() {
        const row = model.nextUnreadRow(currentRow())
        if (row >= 0) {
            selectRow(row)
            return true
        }
        return false
    }

    function openSearch() {
        searchField.visible = true
        searchField.forceActiveFocus()
        searchField.selectAll()
    }

    function closeSearch() {
        searchField.text = ""
        searchField.visible = false
        list.forceActiveFocus()
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        Item {
            Layout.fillWidth: true
            Layout.preferredHeight: Theme.toolbarHeight

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: root.leadingButton.length > 0 ? 6 : 14
                anchors.rightMargin: 8
                spacing: 2

                IconButton {
                    visible: root.leadingButton.length > 0
                    iconName: root.leadingButton === "back" ? "chevron-left" : "sidebar"
                    tip: root.leadingButton === "back" ? qsTr("Back to Feeds") : qsTr("Show Sidebar (Ctrl+Alt+S)")
                    onClicked: root.leadingButtonClicked()
                }

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 0
                    Label {
                        Layout.fillWidth: true
                        text: root.title
                        elide: Text.ElideRight
                        font.pixelSize: Theme.baseFontSize + 1
                        font.weight: Font.Bold
                        color: Theme.text
                    }
                    Label {
                        Layout.fillWidth: true
                        visible: root.title.length > 0
                        text: root.model.unreadCount > 0 ? qsTr("%n unread", "", root.model.unreadCount) : qsTr("No unread articles")
                        font.pixelSize: Theme.smallFontSize
                        color: Theme.secondaryText
                    }
                }
                IconButton {
                    iconName: "search"
                    tip: qsTr("Search (Ctrl+F)")
                    checkable: true
                    checked: searchField.visible
                    onClicked: searchField.visible ? root.closeSearch() : root.openSearch()
                }
                IconButton {
                    iconName: "filter"
                    tip: root.model.hideRead ? qsTr("Show Read Articles (Ctrl+Shift+H)") : qsTr("Hide Read Articles (Ctrl+Shift+H)")
                    checkable: true
                    checked: root.model.hideRead
                    onClicked: {
                        root.model.hideRead = !root.model.hideRead
                        root.model.reload()
                    }
                }
                IconButton {
                    iconName: "mark-all-read"
                    tip: qsTr("Mark All as Read (K)")
                    enabled: root.model.unreadCount > 0
                    onClicked: if (root.model.markAllRead()) root.statusChanged()
                }
            }
        }

        TextField {
            id: searchField
            Layout.fillWidth: true
            Layout.leftMargin: 10
            Layout.rightMargin: 10
            Layout.bottomMargin: 6
            visible: false
            placeholderText: qsTr("Search articles")
            selectByMouse: true
            onTextChanged: searchTimer.restart()
            Keys.onEscapePressed: root.closeSearch()
            Keys.onDownPressed: {
                list.forceActiveFocus()
                root.selectRow(0)
            }

            Timer {
                id: searchTimer
                interval: 200
                onTriggered: {
                    root.model.searchText = searchField.text
                    root.model.reload()
                }
            }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 1
            color: Theme.separator
        }

        ListView {
            id: list
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: root.model
            currentIndex: -1
            activeFocusOnTab: true
            keyNavigationEnabled: false
            boundsBehavior: Flickable.StopAtBounds
            reuseItems: true
            ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

            Keys.onUpPressed: root.selectRow(Math.max(0, root.currentRow() - 1))
            Keys.onDownPressed: root.selectRow(Math.min(list.count - 1, root.currentRow() + 1))
            Keys.onLeftPressed: root.moveFocusLeft()
            Keys.onRightPressed: root.moveFocusRight()
            Keys.onPressed: function(event) {
                if (event.key === Qt.Key_Home) {
                    root.selectRow(0)
                    event.accepted = true
                } else if (event.key === Qt.Key_End) {
                    root.selectRow(list.count - 1)
                    event.accepted = true
                } else if (event.key === Qt.Key_PageDown || event.key === Qt.Key_PageUp) {
                    const page = Math.max(1, Math.floor(list.height / 90))
                    const delta = event.key === Qt.Key_PageDown ? page : -page
                    root.selectRow(Math.max(0, Math.min(list.count - 1, root.currentRow() + delta)))
                    event.accepted = true
                }
            }

            Label {
                anchors.centerIn: parent
                visible: list.count === 0 && root.title.length > 0
                text: root.model.searchText.length > 0 ? qsTr("No matching articles")
                    : root.model.hideRead ? qsTr("No unread articles") : qsTr("No articles")
                color: Theme.tertiaryText
                font.pixelSize: Theme.baseFontSize + 2
            }

            delegate: ItemDelegate {
                id: row
                required property int index
                required property string articleId
                required property string feedTitle
                required property string iconUrl
                required property string title
                required property string summary
                required property string dateText
                required property bool unread
                required property bool starred

                readonly property bool isSelected: articleId === root.currentArticleId
                readonly property bool focused: isSelected && list.activeFocus
                readonly property color primary: focused ? "white" : Theme.text
                readonly property color secondary: focused ? Qt.rgba(1, 1, 1, 0.85) : Theme.secondaryText

                width: ListView.view.width
                padding: 0
                focusPolicy: Qt.NoFocus
                hoverEnabled: false

                background: Rectangle {
                    color: row.focused ? Theme.selection : row.isSelected ? Theme.inactiveSelection : "transparent"
                    Rectangle {
                        anchors.bottom: parent.bottom
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.leftMargin: 20
                        height: 1
                        color: Theme.rowSeparator
                        visible: !row.isSelected
                    }
                }

                contentItem: RowLayout {
                    spacing: 0

                    // Unread indicator column; a star takes its place for starred articles.
                    Item {
                        Layout.preferredWidth: 20
                        Layout.fillHeight: true
                        Rectangle {
                            visible: row.unread && !row.starred
                            x: 6
                            y: 12
                            width: 8
                            height: 8
                            radius: 4
                            color: row.focused ? "white" : Theme.accent
                        }
                        TintedIcon {
                            visible: row.starred
                            x: 3
                            y: 9
                            iconName: "star-filled"
                            size: 13
                            color: row.focused ? "white" : Theme.star
                        }
                    }

                    FeedIcon {
                        visible: root.showIcons
                        Layout.alignment: Qt.AlignTop
                        Layout.topMargin: 10
                        Layout.rightMargin: 10
                        source: row.iconUrl
                        size: 32
                        radius: 5
                        tint: row.focused ? "white" : Theme.accent
                    }

                    ColumnLayout {
                        Layout.fillWidth: true
                        Layout.topMargin: 8
                        Layout.bottomMargin: 10
                        Layout.rightMargin: 12
                        spacing: 1

                        Label {
                            Layout.fillWidth: true
                            visible: text.length > 0
                            text: row.title
                            textFormat: Text.PlainText
                            wrapMode: Text.Wrap
                            maximumLineCount: 3
                            elide: Text.ElideRight
                            font.pixelSize: Theme.baseFontSize
                            font.weight: Font.DemiBold
                            color: row.primary
                        }
                        Label {
                            Layout.fillWidth: true
                            visible: text.length > 0
                            text: row.summary
                            textFormat: Text.PlainText
                            wrapMode: Text.Wrap
                            maximumLineCount: row.title.length > 0 ? 2 : 4
                            elide: Text.ElideRight
                            font.pixelSize: Theme.baseFontSize
                            color: row.secondary
                        }
                        RowLayout {
                            Layout.fillWidth: true
                            Layout.topMargin: 3
                            spacing: 8
                            Label {
                                Layout.fillWidth: true
                                text: row.feedTitle
                                textFormat: Text.PlainText
                                elide: Text.ElideRight
                                font.pixelSize: Theme.smallFontSize
                                font.weight: Font.Bold
                                color: row.secondary
                            }
                            Label {
                                text: row.dateText
                                font.pixelSize: Theme.smallFontSize
                                font.weight: Font.Bold
                                color: row.secondary
                            }
                        }
                    }
                }

                onClicked: {
                    list.forceActiveFocus()
                    root.selectRow(row.index)
                    root.articleActivated()
                }
                onDoubleClicked: {
                    const url = root.model.urlAt(row.index)
                    if (url.length > 0)
                        root.app.openUrl(url)
                }

                TapHandler {
                    acceptedButtons: Qt.RightButton
                    onTapped: {
                        contextMenu.row = row.index
                        contextMenu.unread = row.unread
                        contextMenu.starred = row.starred
                        contextMenu.popup()
                    }
                }
            }
        }
    }

    Menu {
        id: contextMenu
        property int row: -1
        property bool unread: false
        property bool starred: false

        MenuItem {
            text: contextMenu.unread ? qsTr("Mark as Read") : qsTr("Mark as Unread")
            onTriggered: if (root.model.toggleRead(contextMenu.row)) root.statusChanged()
        }
        MenuItem {
            text: contextMenu.starred ? qsTr("Mark as Unstarred") : qsTr("Mark as Starred")
            onTriggered: if (root.model.toggleStarred(contextMenu.row)) root.statusChanged()
        }
        MenuSeparator {}
        MenuItem {
            text: qsTr("Open in Browser")
            onTriggered: root.app.openUrl(root.model.urlAt(contextMenu.row))
        }
        MenuItem {
            text: qsTr("Copy Article URL")
            onTriggered: clipboardHelper.copyText(root.model.urlAt(contextMenu.row))
        }
    }

    // QML has no clipboard API; a hidden TextEdit can copy for us.
    TextEdit {
        id: clipboardHelper
        visible: false
        function copyText(value) {
            text = value
            selectAll()
            copy()
        }
    }
}
