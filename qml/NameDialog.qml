import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// Asks for a name: New Folder and Rename.
Dialog {
    id: root
    property string prompt
    property string acceptLabel: qsTr("OK")
    property alias text: field.text

    signal submitted(string name)

    modal: true
    anchors.centerIn: parent
    width: Math.min(380, parent.width - 40)

    function show(dialogTitle, label, value, buttonLabel) {
        title = dialogTitle
        prompt = label
        acceptLabel = buttonLabel
        field.text = value
        open()
        field.forceActiveFocus()
        field.selectAll()
    }

    function submit() {
        if (field.text.trim().length === 0)
            return
        close()
        submitted(field.text.trim())
    }

    contentItem: ColumnLayout {
        spacing: 8
        Label { text: root.prompt; color: Theme.text }
        TextField {
            id: field
            Layout.fillWidth: true
            selectByMouse: true
            onAccepted: root.submit()
        }
    }

    footer: DialogButtonBox {
        Button {
            text: qsTr("Cancel")
            DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
        }
        Button {
            text: root.acceptLabel
            highlighted: true
            enabled: field.text.trim().length > 0
            onClicked: root.submit()
        }
    }
}
