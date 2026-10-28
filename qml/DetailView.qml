import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtWebEngine

// The article pane: a toolbar of article commands over a web view.
Rectangle {
    id: root
    color: Theme.window

    required property var app
    property string articleId: ""
    property string articleUrl: ""
    property bool unread: false
    property bool starred: false
    property bool hasArticle: articleId.length > 0
    property int textSize: 1
    /// The article theme's name.
    property string theme: "Default"
    property bool javascriptEnabled: false
    /// The one-pane layout's back button, to the timeline.
    property bool showBack: false

    signal toggleReadRequested()
    signal toggleStarRequested()
    signal nextUnreadRequested()
    signal backRequested()

    Keys.onLeftPressed: backRequested()

    function show() {
        web.url = app.articlePage(articleId, qsTr("No Selection"), textSize, Theme.dark, theme, javascriptEnabled)
    }

    /// Makes the web view draw again. While the window is hidden (say, on another
    /// workspace while you read in the browser), WebEngine's Chromium drops the page's
    /// last frame and, until something prompts it, leaves the pane black on return.
    /// Hiding and re-showing the view prompts a new frame without reloading the page.
    function repaint() {
        web.visible = false
        Qt.callLater(() => web.visible = true)
    }

    onArticleIdChanged: show()
    onTextSizeChanged: show()
    onThemeChanged: show()
    onJavascriptEnabledChanged: show()
    Component.onCompleted: show()
    Connections {
        target: Theme
        function onDarkChanged() { root.show() }
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        Item {
            Layout.fillWidth: true
            Layout.preferredHeight: Theme.toolbarHeight

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 8
                anchors.rightMargin: 10
                spacing: 2

                IconButton {
                    visible: root.showBack
                    iconName: "chevron-left"
                    tip: qsTr("Back to Articles")
                    onClicked: root.backRequested()
                }
                IconButton {
                    iconName: "next-unread"
                    tip: qsTr("Go to Next Unread (N)")
                    onClicked: root.nextUnreadRequested()
                }
                IconButton {
                    iconName: root.unread ? "unread" : "read-toggle"
                    tip: root.unread ? qsTr("Mark as Read (R)") : qsTr("Mark as Unread (R)")
                    enabled: root.hasArticle
                    onClicked: root.toggleReadRequested()
                }
                IconButton {
                    iconName: root.starred ? "star-filled" : "star"
                    tint: root.starred ? Theme.star : Theme.secondaryText
                    tip: root.starred ? qsTr("Mark as Unstarred (S)") : qsTr("Mark as Starred (S)")
                    enabled: root.hasArticle
                    onClicked: root.toggleStarRequested()
                }
                Item { Layout.fillWidth: true }
                IconButton {
                    iconName: "text-size"
                    tip: qsTr("Text Size (Ctrl+= / Ctrl+-)")
                    onClicked: sizeMenu.popup()
                    Menu {
                        id: sizeMenu
                        Repeater {
                            model: [qsTr("Small"), qsTr("Medium"), qsTr("Large"), qsTr("Extra Large"), qsTr("Extra Extra Large")]
                            MenuItem {
                                required property int index
                                required property string modelData
                                text: modelData
                                checkable: true
                                checked: root.textSize === index
                                onTriggered: root.textSize = index
                            }
                        }
                    }
                }
                IconButton {
                    iconName: "open-browser"
                    tip: qsTr("Open in Browser (B)")
                    enabled: root.articleUrl.length > 0
                    onClicked: root.app.openUrl(root.articleUrl)
                }
            }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 1
            color: Theme.separator
        }

        WebEngineView {
            id: web
            Layout.fillWidth: true
            Layout.fillHeight: true
            backgroundColor: Theme.window
            settings.javascriptEnabled: root.javascriptEnabled
            settings.pluginsEnabled: false
            // Pages are local files (see render.rs) whose images and media are remote.
            settings.localContentCanAccessRemoteUrls: true
            settings.localContentCanAccessFileUrls: false
            settings.focusOnNavigationEnabled: false

            // The pane only ever shows Pheme's own article pages (in-page anchors included).
            // Clicked links open in the user's browser; anything else, like a refresh or form
            // in an article, is blocked. Embedded frames such as videos load normally.
            onNavigationRequested: function(request) {
                if (!request.isMainFrame)
                    return
                const url = request.url.toString()
                if (url.startsWith("file://") && url.indexOf("/pheme/article-") >= 0)
                    return
                request.reject()
                if (request.navigationType === WebEngineNavigationRequest.LinkClickedNavigation)
                    root.app.openUrl(url)
            }
            onNewWindowRequested: function(request) {
                root.app.openUrl(request.requestedUrl.toString())
            }
            onLoadingChanged: function(request) {
                if (request.status === WebEngineLoadingInfo.LoadFailedStatus)
                    console.warn("Article failed to load:", request.errorCode, request.errorString, request.url)
            }
            onLinkHovered: function(hoveredUrl) {
                linkOverlay.url = hoveredUrl.toString()
            }
            onContextMenuRequested: function(request) {
                request.accepted = true
                if (request.linkUrl.toString().length > 0) {
                    linkMenu.url = request.linkUrl.toString()
                    linkMenu.popup()
                }
            }

            Menu {
                id: linkMenu
                property string url
                MenuItem {
                    text: qsTr("Open Link in Browser")
                    onTriggered: root.app.openUrl(linkMenu.url)
                }
                MenuItem {
                    text: qsTr("Copy Link")
                    onTriggered: web.triggerWebAction(WebEngineView.CopyLinkToClipboard)
                }
            }

            // The hovered link's address, like NetNewsWire's DetailLinkOverlayView.
            Rectangle {
                id: linkOverlay
                property string url: ""
                visible: url.length > 0
                anchors.left: parent.left
                anchors.bottom: parent.bottom
                anchors.margins: 6
                width: Math.min(overlayText.implicitWidth + 16, parent.width - 12)
                height: overlayText.implicitHeight + 8
                radius: 5
                color: Theme.toolbar
                border.color: Theme.separator
                Label {
                    id: overlayText
                    anchors.centerIn: parent
                    width: parent.width - 16
                    text: linkOverlay.url
                    elide: Text.ElideMiddle
                    font.pixelSize: Theme.smallFontSize
                    color: Theme.secondaryText
                }
            }
        }
    }
}
