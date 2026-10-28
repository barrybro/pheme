import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// NetNewsWire's Add Feed sheet: URL, optional name, and folder.
Dialog {
    id: root
    required property var app
    required property var sidebarModel
    property string folderKey: ""
    property string errorText: ""

    signal added(string key)

    title: qsTr("Add Feed")
    modal: true
    anchors.centerIn: parent
    width: Math.min(460, parent.width - 40)
    closePolicy: app.addingFeed ? Popup.NoAutoClose : Popup.CloseOnEscape

    function openFor(folder) {
        folderKey = folder || ""
        errorText = ""
        urlField.text = ""
        nameField.text = ""
        const folders = [{ key: "", title: qsTr("On My Computer (top level)") }].concat(sidebarModel.localFolders())
        folderBox.model = folders
        folderBox.currentIndex = Math.max(0, folders.findIndex(f => f.key === folderKey))
        open()
        urlField.forceActiveFocus()
    }

    function submit() {
        if (urlField.text.trim().length === 0 || app.addingFeed)
            return
        errorText = ""
        app.addFeed(urlField.text, nameField.text, folderBox.model[folderBox.currentIndex].key)
    }

    Connections {
        target: root.app
        function onFeedAdded(ok, message, key) {
            if (!root.opened)
                return
            if (ok) {
                root.close()
                root.added(key)
            } else {
                root.errorText = message
            }
        }
    }

    contentItem: ColumnLayout {
        spacing: 10

        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            color: Theme.secondaryText
            text: qsTr("Enter a feed’s address, or a web site’s and Pheme will find its feed.")
        }
        GridLayout {
            Layout.fillWidth: true
            columns: 2
            columnSpacing: 10
            rowSpacing: 8

            Label { text: qsTr("URL:"); color: Theme.text; Layout.alignment: Qt.AlignRight }
            TextField {
                id: urlField
                Layout.fillWidth: true
                placeholderText: "https://example.com/feed"
                selectByMouse: true
                enabled: !root.app.addingFeed
                onAccepted: root.submit()
            }
            Label { text: qsTr("Name:"); color: Theme.text; Layout.alignment: Qt.AlignRight }
            TextField {
                id: nameField
                Layout.fillWidth: true
                placeholderText: qsTr("Optional")
                selectByMouse: true
                enabled: !root.app.addingFeed
                onAccepted: root.submit()
            }
            Label { text: qsTr("Folder:"); color: Theme.text; Layout.alignment: Qt.AlignRight }
            ComboBox {
                id: folderBox
                Layout.fillWidth: true
                textRole: "title"
                enabled: !root.app.addingFeed
            }
        }
        Label {
            Layout.fillWidth: true
            visible: root.errorText.length > 0
            wrapMode: Text.Wrap
            text: root.errorText
            color: "#e5484d"
        }
    }

    footer: DialogButtonBox {
        BusyIndicator {
            running: root.app.addingFeed
            visible: running
            implicitWidth: 22
            implicitHeight: 22
        }
        Button {
            text: qsTr("Cancel")
            enabled: !root.app.addingFeed
            DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
        }
        Button {
            text: root.app.addingFeed ? qsTr("Adding…") : qsTr("Add")
            highlighted: true
            enabled: urlField.text.trim().length > 0 && !root.app.addingFeed
            onClicked: root.submit()
        }
    }
}
