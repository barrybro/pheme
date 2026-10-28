import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// The Settings window, after NetNewsWire's General preferences. Dev builds add a
// Developer tab.
ApplicationWindow {
    id: root
    title: qsTr("Settings")
    width: 560
    height: pages.implicitHeight + (tabs.visible ? tabs.height : 0) + 48
    minimumWidth: 460
    minimumHeight: height
    color: Theme.window
    flags: Qt.Dialog

    required property var app
    /// Main's window settings, which hold the article text size.
    required property var windowSettings
    required property var preferences

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

    /// Refreshed whenever the window comes forward, so themes just added to the themes
    /// folder show up, like NetNewsWire does on becoming active.
    property var themeNames: ["Default"]
    onActiveChanged: if (active) themeNames = app.themeNames()

    readonly property var refreshMinutes: [0, 30, 60, 120, 240, 480]

    Shortcut { sequences: ["Escape", "Ctrl+W"]; onActivated: root.close() }

    component FormLabel: Label {
        Layout.alignment: Qt.AlignRight | Qt.AlignVCenter
        color: Theme.text
    }

    component Note: Label {
        Layout.fillWidth: true
        wrapMode: Text.Wrap
        font.pixelSize: Theme.smallFontSize
        color: Theme.secondaryText
    }

    header: TabBar {
        id: tabs
        visible: root.app.developerBuild
        TabButton { text: qsTr("General") }
        TabButton { text: qsTr("Developer") }
    }

    StackLayout {
        id: pages
        anchors.fill: parent
        anchors.margins: 24
        currentIndex: tabs.currentIndex

        GridLayout {
            columns: 2
            columnSpacing: 10
            rowSpacing: 14
            Layout.alignment: Qt.AlignTop

            FormLabel {
                text: qsTr("Unread Counts:")
                Layout.alignment: Qt.AlignRight | Qt.AlignTop
                topPadding: 6
            }
            ColumnLayout {
                spacing: 0
                Repeater {
                    model: [qsTr("Show"), qsTr("Show dot"), qsTr("Hide")]
                    RadioButton {
                        required property int index
                        required property string modelData
                        text: modelData
                        checked: root.preferences.unreadCountDisplay === index
                        onClicked: root.preferences.unreadCountDisplay = index
                    }
                }
            }

            FormLabel { text: qsTr("Article Text Size:") }
            ComboBox {
                Layout.preferredWidth: 200
                model: [qsTr("Small"), qsTr("Medium"), qsTr("Large"), qsTr("Extra Large"), qsTr("Extra Extra Large")]
                currentIndex: root.windowSettings.textSize
                onActivated: function(index) { root.windowSettings.textSize = index }
            }

            FormLabel { text: qsTr("Article Theme:") }
            RowLayout {
                spacing: 8
                ComboBox {
                    Layout.preferredWidth: 200
                    model: root.themeNames
                    currentIndex: Math.max(0, root.themeNames.indexOf(root.preferences.articleTheme))
                    onActivated: root.preferences.articleTheme = currentText
                }
                Button {
                    text: qsTr("Open Themes Folder")
                    onClicked: root.app.openThemesFolder()
                }
            }

            FormLabel {
                text: qsTr("Article Content:")
                Layout.alignment: Qt.AlignRight | Qt.AlignTop
                topPadding: 6
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 0
                CheckBox {
                    text: qsTr("Enable JavaScript")
                    checked: root.preferences.javascriptEnabled
                    onToggled: root.preferences.javascriptEnabled = checked
                }
                Note {
                    leftPadding: 6
                    text: qsTr("Some content, such as videos and embedded social media posts, may require JavaScript to work correctly.")
                }
            }

            FormLabel { text: qsTr("Refresh Feeds:") }
            ComboBox {
                Layout.preferredWidth: 200
                model: [qsTr("Manually only"), qsTr("Every 30 minutes"), qsTr("Every hour"), qsTr("Every 2 hours"), qsTr("Every 4 hours"), qsTr("Every 8 hours")]
                currentIndex: Math.max(0, root.refreshMinutes.indexOf(root.preferences.refreshInterval))
                onActivated: function(index) { root.preferences.refreshInterval = root.refreshMinutes[index] }
            }
        }

        ColumnLayout {
            spacing: 0
            Layout.alignment: Qt.AlignTop

            CheckBox {
                text: qsTr("Feedly support")
                checked: root.app.feedlyEnabled
                onToggled: root.app.setFeedlyFlag(checked)
            }
            Note {
                leftPadding: 6
                text: qsTr("Feedly sign-in and sync aren’t finished. Turning this off signs out of Feedly. Release builds always leave it off.")
            }
            Item { Layout.fillHeight: true }
        }
    }
}
