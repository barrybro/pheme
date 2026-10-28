import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// First-run screen: use Pheme on its own, or connect a Feedly account. Also shown later
// from the account menu to add Feedly.
Rectangle {
    id: root
    color: Theme.window

    required property var app
    /// First run, as opposed to adding Feedly later.
    property bool onboarding: true

    signal useLocally()
    signal dismissed()

    ColumnLayout {
        anchors.centerIn: parent
        width: Math.min(440, parent.width - 48)
        spacing: 14

        Image {
            Layout.alignment: Qt.AlignHCenter
            source: "qrc:/qt/qml/app/pheme/resources/icons/pheme.svg"
            sourceSize.width: 96
            sourceSize.height: 96
        }

        Label {
            Layout.alignment: Qt.AlignHCenter
            text: qsTr("Welcome to Pheme")
            font.pixelSize: 24
            font.weight: Font.Bold
            color: Theme.text
        }

        Label {
            Layout.fillWidth: true
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            text: root.onboarding
                ? qsTr("Pheme can follow feeds on its own, right on this computer, or sync with your Feedly account. You can add Feedly any time.")
                : qsTr("Sign in to Feedly to read your Feedly subscriptions alongside your feeds on this computer. Read and starred articles stay in sync with Feedly.")
            color: Theme.secondaryText
            font.pixelSize: Theme.baseFontSize
        }

        Item { Layout.preferredHeight: 6 }

        Button {
            Layout.alignment: Qt.AlignHCenter
            Layout.preferredWidth: 260
            visible: root.onboarding
            highlighted: true
            text: qsTr("Use Without an Account")
            onClicked: root.useLocally()
        }

        RowLayout {
            Layout.fillWidth: true
            visible: root.onboarding
            spacing: 10
            Rectangle { Layout.fillWidth: true; height: 1; color: Theme.separator }
            Label { text: qsTr("or sign in to Feedly"); color: Theme.tertiaryText }
            Rectangle { Layout.fillWidth: true; height: 1; color: Theme.separator }
        }

        Button {
            Layout.alignment: Qt.AlignHCenter
            Layout.preferredWidth: 260
            visible: root.app.oauthAvailable
            enabled: !root.app.signingIn
            highlighted: !root.onboarding
            text: root.app.signingIn ? qsTr("Waiting for your browser…") : qsTr("Sign In with Feedly")
            onClicked: root.app.signInWithBrowser()
        }

        Button {
            Layout.alignment: Qt.AlignHCenter
            visible: root.app.signingIn && root.app.oauthAvailable
            flat: true
            text: qsTr("Cancel")
            onClicked: root.app.cancelSignIn()
        }

        RowLayout {
            Layout.fillWidth: true
            visible: root.app.oauthAvailable
            spacing: 10
            Rectangle { Layout.fillWidth: true; height: 1; color: Theme.separator }
            Label { text: qsTr("or"); color: Theme.tertiaryText }
            Rectangle { Layout.fillWidth: true; height: 1; color: Theme.separator }
        }

        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            textFormat: Text.StyledText
            color: Theme.secondaryText
            font.pixelSize: Theme.smallFontSize + 1
            linkColor: Theme.accent
            text: qsTr("Paste a Feedly developer access token. You can get one at <a href=\"https://feedly.com/v3/auth/dev\">feedly.com/v3/auth/dev</a>.")
            onLinkActivated: function(link) { root.app.openUrl(link) }
            HoverHandler { cursorShape: parent.hoveredLink ? Qt.PointingHandCursor : Qt.ArrowCursor }
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: 8
            TextField {
                id: tokenField
                Layout.fillWidth: true
                placeholderText: qsTr("Access token")
                echoMode: TextInput.Password
                enabled: !root.app.signingIn
                selectByMouse: true
                onAccepted: root.app.signInWithToken(text)
            }
            Button {
                text: root.app.signingIn && !root.app.oauthAvailable ? qsTr("Checking…") : qsTr("Sign In")
                enabled: tokenField.text.trim().length > 0 && !root.app.signingIn
                highlighted: !root.app.oauthAvailable && !root.onboarding
                onClicked: root.app.signInWithToken(tokenField.text)
            }
        }

        Button {
            Layout.alignment: Qt.AlignHCenter
            visible: !root.onboarding
            flat: true
            enabled: !root.app.signingIn
            text: qsTr("Cancel")
            onClicked: root.dismissed()
        }

        Label {
            Layout.fillWidth: true
            visible: root.app.errorMessage.length > 0
            wrapMode: Text.Wrap
            horizontalAlignment: Text.AlignHCenter
            text: root.app.errorMessage
            color: "#e5484d"
        }
    }
}
