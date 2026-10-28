import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// The source list: smart feeds, then each account's folders and feeds.
Rectangle {
    id: root
    color: Theme.sidebar

    required property var model
    required property var app
    property alias list: list
    property string currentKey: ""
    /// Whether the header shows a button to hide the sidebar.
    property bool canHide: true

    signal sourceSelected(string key, string title)
    /// A source was clicked, as opposed to reached with the arrow keys.
    signal sourceActivated()
    signal hideRequested()
    signal moveFocusRight()
    signal addFeedRequested(string folderKey)
    signal addFolderRequested()
    signal renameRequested(string key, string title)
    signal deleteRequested(string key, string title)
    signal importRequested()
    signal exportRequested()
    signal signInRequested()

    /// The local folder new feeds should go into, given the current selection.
    function currentLocalFolder() {
        const row = currentRow()
        return row >= 0 ? model.localFolderKeyFor(row) : ""
    }

    function requestDeleteCurrent() {
        const item = list.itemAtIndex(currentRow())
        if (item && item.account === "local" && (item.kind === "feed" || item.kind === "folder"))
            deleteRequested(item.itemKey, item.title)
    }

    function selectRow(row) {
        if (row < 0 || row >= list.count)
            return
        list.currentIndex = row
        list.positionViewAtIndex(row, ListView.Contain)
        const key = model.keyAt(row)
        if (key.startsWith("account:"))
            return
        if (key !== currentKey) {
            currentKey = key
            sourceSelected(key, model.titleAt(row))
        }
    }

    function selectKey(key) {
        const row = model.rowForKey(key)
        if (row >= 0)
            selectRow(row)
    }

    /// After a reload, put the selection back where it was without re-selecting.
    function restoreSelection() {
        const row = model.rowForKey(currentKey)
        list.currentIndex = row
    }

    function currentRow() {
        return model.rowForKey(currentKey)
    }

    function step(delta) {
        let row = currentRow()
        do {
            row += delta
        } while (row >= 0 && row < list.count && model.keyAt(row).startsWith("account:"))
        selectRow(row)
    }

    function selectNextUnreadFeed() {
        const row = model.nextUnreadRow(currentRow())
        if (row >= 0) {
            // Feeds inside collapsed folders aren't rows, so this only finds visible ones.
            selectRow(row)
            return true
        }
        return false
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        // Toolbar area above the sidebar.
        Item {
            Layout.fillWidth: true
            Layout.preferredHeight: Theme.toolbarHeight

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 12
                anchors.rightMargin: 8
                spacing: 2

                Label {
                    text: "Pheme"
                    font.pixelSize: Theme.baseFontSize + 2
                    font.weight: Font.Bold
                    color: Theme.text
                    elide: Text.ElideRight
                    Layout.fillWidth: true
                }
                IconButton {
                    iconName: "add"
                    tip: qsTr("New Feed or Folder")
                    onClicked: addMenu.popup()
                    Menu {
                        id: addMenu
                        MenuItem {
                            text: qsTr("New Feed…")
                            onTriggered: root.addFeedRequested(root.currentLocalFolder())
                        }
                        MenuItem {
                            text: qsTr("New Folder…")
                            onTriggered: root.addFolderRequested()
                        }
                    }
                }
                IconButton {
                    iconName: "refresh"
                    tip: qsTr("Refresh (Ctrl+R)")
                    enabled: !root.app.syncing
                    onClicked: root.app.refresh()
                    RotationAnimator on rotation {
                        running: root.app.syncing
                        loops: Animation.Infinite
                        from: 0
                        to: 360
                        duration: 1100
                        onStopped: parent.rotation = 0
                    }
                }
                IconButton {
                    visible: root.canHide
                    iconName: "sidebar"
                    tip: qsTr("Hide Sidebar (Ctrl+Alt+S)")
                    onClicked: root.hideRequested()
                }
            }
        }

        ListView {
            id: list
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: root.model
            currentIndex: -1
            boundsBehavior: Flickable.StopAtBounds
            keyNavigationEnabled: false
            activeFocusOnTab: true
            reuseItems: true
            ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

            Keys.onUpPressed: root.step(-1)
            Keys.onDownPressed: root.step(1)
            Keys.onRightPressed: {
                const item = list.itemAtIndex(root.currentRow())
                if (item && item.isFolder && !item.isExpanded)
                    root.model.setExpanded(root.currentRow(), true)
                else
                    root.moveFocusRight()
            }
            Keys.onLeftPressed: {
                const item = list.itemAtIndex(root.currentRow())
                if (item && item.isFolder && item.isExpanded)
                    root.model.setExpanded(root.currentRow(), false)
            }

            delegate: ItemDelegate {
                id: row
                required property int index
                required property string kind
                required property string itemKey
                required property string title
                required property int unreadCount
                required property int depth
                required property bool expanded
                required property string iconUrl
                required property string account

                readonly property bool isHeader: kind === "header"
                readonly property bool isFolder: kind === "folder"
                readonly property bool isExpanded: expanded
                readonly property bool isSelected: itemKey === root.currentKey && !isHeader
                readonly property bool focused: isSelected && list.activeFocus

                width: ListView.view.width
                height: isHeader ? 32 : 28
                padding: 0
                focusPolicy: Qt.NoFocus
                hoverEnabled: true

                background: Rectangle {
                    anchors.fill: parent
                    anchors.leftMargin: 8
                    anchors.rightMargin: 8
                    anchors.topMargin: 1
                    anchors.bottomMargin: 1
                    radius: 6
                    color: row.focused ? Theme.selection
                         : row.isSelected ? Theme.inactiveSelection
                         : "transparent"
                }

                contentItem: RowLayout {
                    spacing: 6

                    Item { Layout.preferredWidth: 10 + row.depth * 18 }

                    // Disclosure triangle for folders; a spacer elsewhere keeps icons aligned.
                    Item {
                        visible: !row.isHeader
                        Layout.preferredWidth: 12
                        Layout.preferredHeight: 12
                        TintedIcon {
                            anchors.centerIn: parent
                            visible: row.isFolder
                            iconName: "chevron-right"
                            size: 12
                            color: row.focused ? "white" : Theme.secondaryText
                            rotation: row.expanded ? 90 : 0
                            Behavior on rotation { NumberAnimation { duration: 120 } }
                        }
                        MouseArea {
                            anchors.fill: parent
                            anchors.margins: -6
                            enabled: row.isFolder
                            onClicked: root.model.toggleExpanded(row.index)
                        }
                    }

                    TintedIcon {
                        visible: row.kind === "smart" || row.isFolder
                        size: 18
                        iconName: row.isFolder ? "folder"
                            : row.itemKey === "smart:today" ? "today"
                            : row.itemKey === "smart:starred" ? "star-filled"
                            : "unread"
                        color: row.focused ? "white"
                             : row.itemKey === "smart:today" ? Theme.today
                             : row.itemKey === "smart:starred" ? Theme.star
                             : Theme.accent
                    }

                    FeedIcon {
                        visible: row.kind === "feed"
                        source: row.iconUrl
                        size: 18
                        tint: row.focused ? "white" : Theme.accent
                    }

                    Label {
                        Layout.fillWidth: true
                        text: row.title
                        elide: Text.ElideRight
                        font.pixelSize: row.isHeader ? Theme.smallFontSize : Theme.baseFontSize
                        font.weight: row.isHeader ? Font.Bold : Font.Normal
                        color: row.focused ? "white" : row.isHeader ? Theme.tertiaryText : Theme.text
                        topPadding: row.isHeader ? 8 : 0
                    }

                    UnreadBadge {
                        count: row.unreadCount
                        selected: row.focused
                    }

                    Item { Layout.preferredWidth: 12 }
                }

                onClicked: {
                    if (row.isHeader)
                        return
                    list.forceActiveFocus()
                    root.selectRow(row.index)
                    root.sourceActivated()
                }
                onDoubleClicked: if (row.isFolder) root.model.toggleExpanded(row.index)

                TapHandler {
                    acceptedButtons: Qt.RightButton
                    onTapped: {
                        rowMenu.key = row.itemKey
                        rowMenu.itemTitle = row.title
                        rowMenu.kind = row.kind
                        rowMenu.account = row.account
                        rowMenu.folderKey = root.model.localFolderKeyFor(row.index)
                        rowMenu.popup()
                    }
                }
            }
        }

        // Refresh progress, as at the bottom of NetNewsWire's sidebar.
        Item {
            Layout.fillWidth: true
            Layout.preferredHeight: 30

            Rectangle {
                anchors.top: parent.top
                width: parent.width
                height: 1
                color: Theme.separator
                opacity: 0.6
            }

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 14
                anchors.rightMargin: 12
                spacing: 8

                BusyIndicator {
                    running: root.app.syncing
                    visible: running
                    Layout.preferredWidth: 14
                    Layout.preferredHeight: 14
                    padding: 0
                }
                Label {
                    Layout.fillWidth: true
                    elide: Text.ElideRight
                    font.pixelSize: Theme.smallFontSize
                    color: Theme.secondaryText
                    text: root.app.syncing ? root.app.status
                        : [root.app.lastRefreshed.length > 0 ? qsTr("Updated %1").arg(root.app.lastRefreshed) : "",
                           root.app.status].filter(s => s.length > 0).join(" · ")
                }
                IconButton {
                    iconName: "account"
                    tip: qsTr("Accounts and Subscriptions")
                    implicitWidth: 26
                    implicitHeight: 24
                    icon.width: 15
                    icon.height: 15
                    onClicked: accountMenu.popup()

                    Menu {
                        id: accountMenu
                        MenuItem {
                            text: qsTr("Import Subscriptions (OPML)…")
                            onTriggered: root.importRequested()
                        }
                        MenuItem {
                            text: qsTr("Export Subscriptions (OPML)…")
                            onTriggered: root.exportRequested()
                        }
                        MenuSeparator {}
                        MenuItem {
                            visible: root.app.signedIn
                            height: visible ? implicitHeight : 0
                            text: qsTr("Feedly: %1").arg(root.app.accountName.length > 0 ? root.app.accountName : qsTr("signed in"))
                            enabled: false
                        }
                        MenuItem {
                            text: root.app.signedIn ? qsTr("Sign Out of Feedly…") : qsTr("Add Feedly Account…")
                            onTriggered: root.app.signedIn ? signOutDialog.open() : root.signInRequested()
                        }
                    }
                }
            }
        }
    }

    Dialog {
        id: signOutDialog
        parent: Overlay.overlay
        anchors.centerIn: parent
        modal: true
        title: qsTr("Sign out of Feedly?")
        standardButtons: Dialog.Cancel | Dialog.Ok
        Label {
            text: qsTr("Pheme will forget your Feedly articles on this computer.\nYour feeds under On My Computer stay, and nothing changes in your Feedly account.")
            color: Theme.text
        }
        onAccepted: root.app.signOut()
    }

    // Context menu for sidebar rows. Only local items can be changed here; Feedly's are
    // managed on feedly.com for now.
    Menu {
        id: rowMenu
        property string key
        property string itemTitle
        property string kind
        property string account
        property string folderKey
        readonly property bool editable: account === "local" && (kind === "feed" || kind === "folder")

        MenuItem {
            text: qsTr("New Feed…")
            visible: rowMenu.account === "local"
            height: visible ? implicitHeight : 0
            onTriggered: root.addFeedRequested(rowMenu.folderKey)
        }
        MenuItem {
            text: qsTr("New Folder…")
            visible: rowMenu.account === "local" && rowMenu.kind === "header"
            height: visible ? implicitHeight : 0
            onTriggered: root.addFolderRequested()
        }
        MenuSeparator { visible: rowMenu.editable; height: visible ? implicitHeight : 0 }
        MenuItem {
            text: qsTr("Rename…")
            visible: rowMenu.editable
            height: visible ? implicitHeight : 0
            onTriggered: root.renameRequested(rowMenu.key, rowMenu.itemTitle)
        }
        Menu {
            id: moveMenu
            title: qsTr("Move To")
            enabled: rowMenu.editable && rowMenu.kind === "feed"
            Instantiator {
                model: rowMenu.opened ? [{ key: "", title: qsTr("On My Computer (top level)") }].concat(root.model.localFolders()) : []
                delegate: MenuItem {
                    required property var modelData
                    text: modelData.title
                    enabled: modelData.key !== rowMenu.folderKey
                    onTriggered: root.app.moveFeed(rowMenu.key, modelData.key)
                }
                onObjectAdded: function(index, object) { moveMenu.insertItem(index, object) }
                onObjectRemoved: function(index, object) { moveMenu.removeItem(object) }
            }
        }
        MenuItem {
            text: qsTr("Delete…")
            visible: rowMenu.editable
            height: visible ? implicitHeight : 0
            onTriggered: root.deleteRequested(rowMenu.key, rowMenu.itemTitle)
        }
        MenuItem {
            text: qsTr("Managed on feedly.com")
            visible: rowMenu.account === "feedly"
            height: visible ? implicitHeight : 0
            enabled: false
        }
    }
}
