import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kcmutils as KCMUtils

KCMUtils.ScrollViewKCM {
    id: root

    property string successText: ""

    Connections {
        target: kcm
        function onOperationSucceeded(message) {
            root.successText = message
            successTimer.restart()
        }
        function onStatusChanged() {
            if (kcm.chargeLimit >= 25 && !limitControl.activeFocus)
                limitControl.value = kcm.chargeLimit
        }
    }

    Timer {
        id: successTimer
        interval: 4000
        onTriggered: root.successText = ""
    }

    view: Flickable {
        id: scroller
        contentWidth: width
        contentHeight: content.implicitHeight
        clip: true

        ColumnLayout {
            id: content
            x: Kirigami.Units.largeSpacing
            width: scroller.width - 2 * Kirigami.Units.largeSpacing
            spacing: Kirigami.Units.smallSpacing

            Kirigami.InlineMessage {
                Layout.fillWidth: true
                visible: kcm.lastError.length > 0
                type: Kirigami.MessageType.Error
                text: kcm.lastError
            }

            Kirigami.InlineMessage {
                Layout.fillWidth: true
                visible: root.successText.length > 0
                type: Kirigami.MessageType.Positive
                text: root.successText
            }

            Kirigami.Heading { text: i18n("Battery"); level: 2 }

            RowLayout {
                Layout.fillWidth: true
                Controls.Label {
                    text: kcm.chargePercent < 0 ? i18n("Unavailable") : i18n("%1%", kcm.chargePercent)
                    font.pixelSize: Kirigami.Units.gridUnit * 2
                    font.bold: true
                }
                Controls.Label {
                    text: kcm.batteryState
                    Layout.fillWidth: true
                }
                Controls.Button {
                    text: i18n("Refresh")
                    icon.name: "view-refresh"
                    onClicked: kcm.refresh()
                }
            }

            Kirigami.Separator { Layout.fillWidth: true }
            Kirigami.Heading { text: i18n("Charge limit"); level: 2 }

            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                text: kcm.chargeLimit < 0 ? i18n("Current limit unavailable") : i18n("Current limit: %1%", kcm.chargeLimit)
            }

            RowLayout {
                Layout.fillWidth: true
                Controls.SpinBox {
                    id: limitControl
                    from: 25
                    to: 100
                    value: kcm.chargeLimit >= 25 ? kcm.chargeLimit : 80
                    enabled: kcm.serviceAvailable && !kcm.busy
                    Accessible.name: i18n("Charge limit percentage")
                }
                Controls.Button {
                    text: i18n("Apply")
                    enabled: kcm.serviceAvailable && !kcm.busy
                    onClicked: kcm.setChargeLimit(limitControl.value)
                }
                Item { Layout.fillWidth: true }
            }

            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                opacity: 0.7
                text: i18n("A manual change lasts until the next scheduled event when scheduling is enabled.")
            }

            Controls.Button {
                text: i18n("Charge to 100% once")
                enabled: kcm.serviceAvailable && kcm.overrideAvailable && !kcm.busy
                onClicked: kcm.chargeToFullOnce()
            }

            Controls.Label {
                visible: kcm.serviceAvailable && !kcm.overrideAvailable
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                opacity: 0.7
                text: i18n("One-time full charge is available after the EC command is verified on this laptop.")
            }

            Kirigami.Separator { Layout.fillWidth: true }

            RowLayout {
                Layout.fillWidth: true
                ColumnLayout {
                    Layout.fillWidth: true
                    Kirigami.Heading { text: i18n("Weekly schedule"); level: 2 }
                    Controls.Label {
                        text: kcm.scheduleEnabled ? i18n("Enabled") : i18n("Disabled")
                        opacity: 0.7
                    }
                }
                Controls.Button {
                    text: i18n("Configure")
                    onClicked: kcm.push("SchedulePage.qml")
                }
            }

        }
    }
}
