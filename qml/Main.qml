import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtCore
import QtQuick.Dialogs
import app.pheme

ApplicationWindow {
    id: window
    width: 1280
    height: 800
    minimumWidth: 320
    minimumHeight: 360
    visible: true
    title: timeline.title.length > 0 ? timeline.title + " — Pheme" : "Pheme"
    color: Theme.window

    onActiveChanged: if (active) detail.repaint()

    palette.window: Theme.window
    palette.base: Theme.window
    palette.text: Theme.text
    palette.windowText: Theme.text
    palette.buttonText: Theme.text
    palette.highlight: Theme.accent
    palette.highlightedText: "white"
    palette.button: Theme.toolbar
    palette.mid: Theme.separator
    palette.placeholderText: Theme.tertiaryText

    // Layout adapts to width, like NetNewsWire across Mac, iPad, and iPhone:
    //   three: sidebar (unless hidden), timeline, and article side by side;
    //   two:   two panes at a time: sidebar + timeline while choosing a feed,
    //          timeline + article while reading;
    //   one:   one pane at a time, with back buttons.
    // `pane` is where the user is in the two- and one-pane layouts.
    readonly property string layoutMode: width >= 980 ? "three" : width >= 560 ? "two" : "one"
    property string pane: "timeline"
    readonly property bool sidebarShown: layoutMode === "three" ? !settings.sidebarCollapsed : pane === "sidebar"
    readonly property bool timelineShown: layoutMode !== "one" || pane === "timeline"
    readonly property bool detailShown: layoutMode === "three" || (layoutMode === "two" ? pane !== "sidebar" : pane === "detail")

    function showPane(which) {
        pane = which
        if (which === "sidebar")
            sidebar.list.forceActiveFocus()
        else if (which === "timeline")
            timeline.list.forceActiveFocus()
        else
            detail.forceActiveFocus()
    }

    /// Gives keyboard focus to a pane that's on screen: the current pane in the narrow
    /// layouts, the sidebar (or the timeline, if it's hidden) in the full one.
    function focusVisiblePane() {
        if (layoutMode !== "three")
            showPane(pane)
        else if (sidebarShown)
            sidebar.list.forceActiveFocus()
        else
            timeline.list.forceActiveFocus()
    }

    // A resize can hide the pane that has focus.
    onLayoutModeChanged: if (started) Qt.callLater(focusVisiblePane)

    function toggleSidebar() {
        if (layoutMode === "three") {
            settings.sidebarCollapsed = !settings.sidebarCollapsed
            if (settings.sidebarCollapsed && sidebar.list.activeFocus)
                timeline.list.forceActiveFocus()
        } else {
            showPane(pane === "sidebar" ? "timeline" : "sidebar")
        }
    }

    /// Articles are marked read when they're actually on screen, which in the narrow
    /// layouts can be after they're selected.
    function markCurrentRead() {
        const row = timeline.currentRow()
        if (row >= 0 && timelineModel.setRead(row, true))
            statusChanged()
        updateDetailStatus(row)
    }

    onDetailShownChanged: if (detailShown) markCurrentRead()

    /// Past the welcome screen: using Pheme locally, with Feedly, or both.
    readonly property bool ready: settings.onboarded || app.signedIn
    property bool showingSignIn: false
    property bool started: false

    AppController {
        id: app
        onDataChanged: window.reloadAll()
        onSignedInChanged: {
            if (signedIn) {
                settings.onboarded = true
                window.showingSignIn = false
            }
            if (window.started)
                window.reloadAll()
            else if (window.ready)
                Qt.callLater(window.start)
        }
    }

    SidebarModel {
        id: sidebarModel
        // Empty when signed out, which leaves the Feedly section out.
        accountTitle: app.signedIn ? (app.accountName.length > 0 ? app.accountName : "Feedly") : ""
    }

    TimelineModel {
        id: timelineModel
        hideRead: settings.hideRead
        onHideReadChanged: settings.hideRead = hideRead
    }

    Settings {
        id: settings
        category: "window"
        property real sidebarWidth: 240
        property real timelineWidth: 380
        property bool sidebarCollapsed: false
        property alias width: window.width
        property alias height: window.height
        property string lastSource: "smart:unread"
        property bool hideRead: false
        property int textSize: 1
        property bool onboarded: false
    }

    function start() {
        if (started)
            return
        started = true
        sidebarModel.reload()
        sidebar.selectKey(settings.lastSource)
        if (sidebar.currentKey.length === 0)
            sidebar.selectKey("smart:unread")
        focusVisiblePane()
        app.refresh()
    }

    function reloadAll() {
        if (!started)
            return
        sidebarModel.reload()
        sidebar.restoreSelection()
        timelineModel.reload()
        timeline.restoreSelection()
        updateDetailStatus(timeline.currentRow())
    }

    function updateDetailStatus(row) {
        const item = row >= 0 ? timeline.list.itemAtIndex(row) : null
        if (item) {
            detail.unread = item.unread
            detail.starred = item.starred
        }
    }

    function statusChanged() {
        sidebarModel.refreshCounts()
        updateDetailStatus(timeline.currentRow())
        sendTimer.restart()
    }

    function goToNextUnread() {
        if (!timeline.selectNextUnread()) {
            // Like NetNewsWire, move on to the next feed with unread articles.
            if (!sidebar.selectNextUnreadFeed())
                return
            timeline.list.forceActiveFocus()
            timeline.selectNextUnread()
        }
        // Going to an article means reading it, in any layout.
        if (!detailShown)
            showPane("detail")
    }

    function toggleRead() {
        if (timelineModel.toggleRead(timeline.currentRow()))
            statusChanged()
    }

    function toggleStar() {
        if (timelineModel.toggleStarred(timeline.currentRow()))
            statusChanged()
    }

    // Read/starred changes go to Feedly a few seconds after the last one.
    Timer {
        id: sendTimer
        interval: 4000
        onTriggered: app.sendPending()
    }

    Timer {
        interval: 30 * 60 * 1000
        repeat: true
        running: window.ready
        onTriggered: app.refresh()
    }

    Component.onCompleted: if (ready) start()

    StackLayout {
        anchors.fill: parent
        currentIndex: window.ready && !window.showingSignIn ? 1 : 0

        SignInView {
            app: app
            onboarding: !window.ready
            onUseLocally: {
                settings.onboarded = true
                window.start()
            }
            onDismissed: window.showingSignIn = false
        }

        SplitView {
            id: split
            orientation: Qt.Horizontal

            handle: Rectangle {
                implicitWidth: 1
                color: Theme.separator
                containmentMask: Item {
                    x: -3
                    width: 7
                    height: split.height
                }
            }

            SidebarView {
                id: sidebar
                visible: window.sidebarShown
                // Fills the window when it's the only pane.
                SplitView.fillWidth: !window.timelineShown
                SplitView.preferredWidth: settings.sidebarWidth
                SplitView.minimumWidth: 160
                SplitView.maximumWidth: window.timelineShown ? 420 : Infinity
                // Remember widths the user drags to in the full layout only.
                onWidthChanged: if (visible && window.layoutMode === "three") settings.sidebarWidth = width
                model: sidebarModel
                app: app
                canHide: window.layoutMode !== "one"
                onHideRequested: window.toggleSidebar()
                onSourceActivated: if (window.layoutMode === "one") window.showPane("timeline")

                onSourceSelected: function(key, title) {
                    settings.lastSource = key
                    timeline.title = title
                    timeline.showIcons = !key.startsWith("feed:")
                    timelineModel.selectionKey = key
                    timelineModel.reload()
                    timeline.clearSelection()
                    timeline.list.positionViewAtBeginning()
                }
                onMoveFocusRight: {
                    if (window.layoutMode === "one")
                        window.showPane("timeline")
                    else
                        timeline.list.forceActiveFocus()
                    if (timeline.currentRow() < 0)
                        timeline.selectRow(0)
                }
                onAddFeedRequested: function(folderKey) { addFeedDialog.openFor(folderKey) }
                onAddFolderRequested: newFolderDialog.show(qsTr("New Folder"), qsTr("Name:"), "", qsTr("Create"))
                onRenameRequested: function(key, title) {
                    renameDialog.key = key
                    renameDialog.show(qsTr("Rename"), qsTr("New name for “%1”:").arg(title), title, qsTr("Rename"))
                }
                onDeleteRequested: function(key, title) {
                    deleteDialog.key = key
                    deleteDialog.itemTitle = title
                    deleteDialog.open()
                }
                onImportRequested: importDialog.open()
                onExportRequested: exportDialog.open()
                onSignInRequested: window.showingSignIn = true
            }

            TimelineView {
                id: timeline
                visible: window.timelineShown
                SplitView.fillWidth: !window.detailShown
                SplitView.preferredWidth: settings.timelineWidth
                SplitView.minimumWidth: 240
                // Always leave the article pane its minimum, so its toolbar never ends up off-screen.
                SplitView.maximumWidth: window.detailShown
                    ? Math.max(240, window.width - detail.SplitView.minimumWidth - (window.sidebarShown ? sidebar.width + 1 : 0) - 1)
                    : Infinity
                onWidthChanged: if (visible && window.layoutMode === "three") settings.timelineWidth = width
                model: timelineModel
                app: app
                leadingButton: window.layoutMode === "one" ? "back" : window.sidebarShown ? "" : "sidebar"
                onLeadingButtonClicked: window.layoutMode === "one" ? window.showPane("sidebar") : window.toggleSidebar()

                onArticleSelected: function(row, articleId) {
                    detail.articleUrl = row >= 0 ? timelineModel.urlAt(row) : ""
                    detail.articleId = articleId
                    if (window.detailShown)
                        window.markCurrentRead()
                }
                onArticleActivated: if (!window.detailShown) window.showPane("detail")
                onStatusChanged: window.statusChanged()
                onMoveFocusLeft: {
                    if (window.layoutMode === "three")
                        window.sidebarShown && sidebar.list.forceActiveFocus()
                    else
                        window.showPane("sidebar")
                }
                onMoveFocusRight: window.detailShown ? detail.forceActiveFocus() : window.showPane("detail")
            }

            DetailView {
                id: detail
                visible: window.detailShown
                SplitView.fillWidth: true
                SplitView.minimumWidth: 280
                app: app
                showBack: window.layoutMode === "one"
                onBackRequested: window.layoutMode === "one" ? window.showPane("timeline") : timeline.list.forceActiveFocus()
                textSize: settings.textSize
                onTextSizeChanged: settings.textSize = textSize
                onToggleReadRequested: window.toggleRead()
                onToggleStarRequested: window.toggleStar()
                onNextUnreadRequested: window.goToNextUnread()
            }
        }
    }

    AddFeedDialog {
        id: addFeedDialog
        app: app
        sidebarModel: sidebarModel
        onAdded: function(key) {
            sidebar.selectKey(key)
            sidebar.list.forceActiveFocus()
        }
    }

    NameDialog {
        id: newFolderDialog
        onSubmitted: function(name) {
            const key = app.addFolder(name)
            if (key.length > 0)
                sidebar.selectKey(key)
        }
    }

    NameDialog {
        id: renameDialog
        property string key
        onSubmitted: function(name) {
            if (app.renameItem(key, name) && key === sidebar.currentKey)
                timeline.title = name
        }
    }

    Dialog {
        id: deleteDialog
        property string key
        property string itemTitle
        anchors.centerIn: parent
        modal: true
        title: key.startsWith("folder:") ? qsTr("Delete folder “%1”?").arg(itemTitle) : qsTr("Delete “%1”?").arg(itemTitle)
        Label {
            text: deleteDialog.key.startsWith("folder:")
                ? qsTr("The folder, its feeds, and their articles will be deleted.")
                : qsTr("The feed and its articles will be deleted.")
            color: Theme.text
        }
        footer: DialogButtonBox {
            Button { text: qsTr("Cancel"); DialogButtonBox.buttonRole: DialogButtonBox.RejectRole }
            Button { text: qsTr("Delete"); DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole }
        }
        onAccepted: {
            const wasSelected = key === sidebar.currentKey
            if (app.deleteItem(key) && wasSelected) {
                sidebar.currentKey = ""
                sidebar.selectKey("smart:unread")
            }
        }
    }

    FileDialog {
        id: importDialog
        title: qsTr("Import Subscriptions")
        nameFilters: [qsTr("OPML files (*.opml *.xml)"), qsTr("All files (*)")]
        onAccepted: app.importOpml(selectedFile.toString())
    }

    FileDialog {
        id: exportDialog
        title: qsTr("Export Subscriptions")
        fileMode: FileDialog.SaveFile
        defaultSuffix: "opml"
        currentFolder: StandardPaths.writableLocation(StandardPaths.DocumentsLocation)
        nameFilters: [qsTr("OPML files (*.opml)")]
        onAccepted: app.exportOpml(selectedFile.toString())
    }

    // A refresh error, shown as a banner over the panes.
    Rectangle {
        visible: window.ready && !window.showingSignIn && app.errorMessage.length > 0
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 16
        width: Math.min(errorRow.implicitWidth + 24, parent.width - 32)
        height: errorRow.implicitHeight + 16
        radius: 8
        color: Theme.dark ? "#3b1f21" : "#fdecec"
        border.color: Theme.dark ? "#6b2b2f" : "#f3b9bb"

        RowLayout {
            id: errorRow
            anchors.centerIn: parent
            width: parent.width - 24
            spacing: 10
            Label {
                Layout.fillWidth: true
                text: app.errorMessage
                wrapMode: Text.Wrap
                color: Theme.text
            }
            Button {
                visible: app.needsSignIn
                text: qsTr("Sign In Again")
                onClicked: app.signOut()
            }
            IconButton {
                iconName: "close"
                tip: qsTr("Dismiss")
                onClicked: app.clearError()
            }
        }
    }

    // Keyboard shortcuts follow NetNewsWire's where they make sense on Linux.
    // Single-letter shortcuts are off while typing in a text field.
    readonly property bool typing: activeFocusItem instanceof TextInput || activeFocusItem instanceof TextEdit
    readonly property bool commandsEnabled: started && !showingSignIn && !typing && !Overlay.overlay.children.some(c => c.visible && c.modal)

    Shortcut { sequences: ["Ctrl+R", "F5"]; enabled: window.started; onActivated: app.refresh() }
    Shortcut { sequence: "Ctrl+N"; enabled: window.started; onActivated: addFeedDialog.openFor(sidebar.currentLocalFolder()) }
    Shortcut { sequence: "Ctrl+Shift+N"; enabled: window.started; onActivated: newFolderDialog.show(qsTr("New Folder"), qsTr("Name:"), "", qsTr("Create")) }
    Shortcut { sequence: "Delete"; enabled: window.commandsEnabled && sidebar.list.activeFocus; onActivated: sidebar.requestDeleteCurrent() }
    Shortcut {
        sequence: "Ctrl+F"
        enabled: window.started
        onActivated: {
            if (!window.timelineShown)
                window.showPane("timeline")
            timeline.openSearch()
        }
    }
    Shortcut { sequence: "Ctrl+Alt+S"; enabled: window.started; onActivated: window.toggleSidebar() }
    Shortcut { sequence: "Ctrl+Shift+H"; enabled: window.started; onActivated: { timelineModel.hideRead = !timelineModel.hideRead; timelineModel.reload() } }
    Shortcut { sequences: ["Ctrl+=", "Ctrl++"]; onActivated: detail.textSize = Math.min(4, detail.textSize + 1) }
    Shortcut { sequence: "Ctrl+-"; onActivated: detail.textSize = Math.max(0, detail.textSize - 1) }
    Shortcut { sequence: "Ctrl+Q"; onActivated: Qt.quit() }
    Shortcut { sequences: ["N", "+", "Space"]; enabled: window.commandsEnabled; onActivated: window.goToNextUnread() }
    Shortcut { sequence: "R"; enabled: window.commandsEnabled; onActivated: window.toggleRead() }
    Shortcut { sequence: "S"; enabled: window.commandsEnabled; onActivated: window.toggleStar() }
    Shortcut { sequence: "K"; enabled: window.commandsEnabled; onActivated: if (timelineModel.markAllRead()) window.statusChanged() }
    Shortcut {
        sequences: ["B", "Return"]
        enabled: window.commandsEnabled && detail.articleUrl.length > 0
        onActivated: app.openUrl(detail.articleUrl)
    }
}
