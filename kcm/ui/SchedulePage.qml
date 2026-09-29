import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kcmutils as KCMUtils

KCMUtils.ScrollViewKCM {
    id: root

    property string successText: ""
    readonly property var dayNames: [i18n("Mon"), i18n("Tue"), i18n("Wed"), i18n("Thu"), i18n("Fri"), i18n("Sat"), i18n("Sun")]

    function reload() {
        scheduleModel.clear()
        const entries = kcm.scheduleEntries
        for (let i = 0; i < entries.length; ++i) {
            const entry = entries[i]
            scheduleModel.append({days: entry.days, minute: entry.minute, limit: entry.limit})
        }
        enabledSwitch.checked = kcm.scheduleEnabled
    }

    function save() {
        const entries = []
        for (let i = 0; i < scheduleModel.count; ++i) {
            const entry = scheduleModel.get(i)
            entries.push({days: entry.days, minute: entry.minute, limit: entry.limit})
        }
        kcm.saveSchedule(enabledSwitch.checked, entries)
    }

    Component.onCompleted: reload()

    Connections {
        target: kcm
        function onScheduleLoaded() { root.reload() }
        function onOperationSucceeded(message) {
            root.successText = message
            successTimer.restart()
        }
    }

    Timer {
        id: successTimer
        interval: 4000
        onTriggered: root.successText = ""
    }

    ListModel { id: scheduleModel }

    view: Flickable {
        id: scroller
        contentWidth: width
        contentHeight: content.implicitHeight
        clip: true

        ColumnLayout {
            id: content
            x: Kirigami.Units.largeSpacing
            width: scroller.width - 2 * Kirigami.Units.largeSpacing
            spacing: Kirigami.Units.largeSpacing

            Kirigami.Heading {
                text: i18n("Weekly charge schedule")
                level: 1
                Layout.topMargin: Kirigami.Units.largeSpacing
            }

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

            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                text: i18n("Each entry sets a charge limit at the selected local weekday and time. On startup, the most recent entry takes effect.")
            }

            Controls.Switch {
                id: enabledSwitch
                text: i18n("Enable schedule")
                enabled: !kcm.busy
            }

            Repeater {
                model: scheduleModel
                delegate: Controls.Frame {
                    id: entryFrame
                    required property int index
                    required property int days
                    required property int minute
                    required property int limit
                    Layout.fillWidth: true

                    ColumnLayout {
                        anchors.fill: parent
                        spacing: Kirigami.Units.smallSpacing

                        RowLayout {
                            Layout.fillWidth: true
                            Controls.Label {
                                text: i18n("Entry %1", entryFrame.index + 1)
                                font.bold: true
                                Layout.fillWidth: true
                            }
                            Controls.Button {
                                icon.name: "list-remove"
                                text: i18n("Remove")
                                enabled: !kcm.busy
                                onClicked: scheduleModel.remove(entryFrame.index)
                            }
                        }

                        Flow {
                            Layout.fillWidth: true
                            spacing: Kirigami.Units.smallSpacing
                            Repeater {
                                model: 7
                                delegate: Controls.CheckBox {
                                    required property int index
                                    text: root.dayNames[index]
                                    checked: (entryFrame.days & (1 << index)) !== 0
                                    enabled: !kcm.busy
                                    onClicked: scheduleModel.setProperty(entryFrame.index, "days",
                                        checked ? entryFrame.days | (1 << index) : entryFrame.days & ~(1 << index))
                                }
                            }
                        }

                        RowLayout {
                            Layout.fillWidth: true
                            Controls.Label { text: i18n("Time") }
                            Controls.SpinBox {
                                from: 0; to: 23
                                value: Math.floor(entryFrame.minute / 60)
                                textFromValue: function(value) { return String(value).padStart(2, "0") }
                                enabled: !kcm.busy
                                Accessible.name: i18n("Hour")
                                onValueModified: scheduleModel.setProperty(entryFrame.index, "minute", value * 60 + entryFrame.minute % 60)
                            }
                            Controls.Label { text: ":" }
                            Controls.SpinBox {
                                from: 0; to: 59
                                value: entryFrame.minute % 60
                                textFromValue: function(value) { return String(value).padStart(2, "0") }
                                enabled: !kcm.busy
                                Accessible.name: i18n("Minute")
                                onValueModified: scheduleModel.setProperty(entryFrame.index, "minute", Math.floor(entryFrame.minute / 60) * 60 + value)
                            }
                            Item { Layout.fillWidth: true }
                            Controls.Label { text: i18n("Limit") }
                            Controls.SpinBox {
                                from: 25; to: 100
                                value: entryFrame.limit
                                enabled: !kcm.busy
                                Accessible.name: i18n("Charge limit percentage")
                                onValueModified: scheduleModel.setProperty(entryFrame.index, "limit", value)
                            }
                            Controls.Label { text: "%" }
                        }
                    }
                }
            }

            RowLayout {
                Layout.fillWidth: true
                Controls.Button {
                    text: i18n("Add entry")
                    icon.name: "list-add"
                    enabled: !kcm.busy && scheduleModel.count < 32
                    onClicked: scheduleModel.append({days: 31, minute: 480, limit: 80})
                }
                Item { Layout.fillWidth: true }
                Controls.Button {
                    text: i18n("Save schedule")
                    enabled: !kcm.busy
                    onClicked: root.save()
                }
            }

            Item { Layout.preferredHeight: Kirigami.Units.largeSpacing }
        }
    }
}
