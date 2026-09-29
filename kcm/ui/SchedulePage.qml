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
            const start = entry.start_minute === undefined ? entry.minute : entry.start_minute
            scheduleModel.append({
                profileName: entry.name || i18n("Profile %1", i + 1),
                profileEnabled: entry.enabled === undefined ? true : entry.enabled,
                days: entry.days,
                startMinute: start,
                endMinute: entry.end_minute === null || entry.end_minute === undefined ? (start + 60) % 1440 : entry.end_minute,
                limit: entry.limit
            })
        }
        enabledSwitch.checked = kcm.scheduleEnabled
        outsideLimitControl.value = kcm.scheduleOutsideLimit
    }

    function save() {
        const entries = []
        for (let i = 0; i < scheduleModel.count; ++i) {
            const entry = scheduleModel.get(i)
            entries.push({
                name: entry.profileName,
                enabled: entry.profileEnabled,
                days: entry.days,
                start_minute: entry.startMinute,
                end_minute: entry.endMinute,
                limit: entry.limit
            })
        }
        kcm.saveSchedule(enabledSwitch.checked, outsideLimitControl.value, entries)
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
            spacing: Kirigami.Units.smallSpacing

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

            Kirigami.InlineMessage {
                Layout.fillWidth: true
                visible: kcm.scheduleHasLegacyEntries
                type: Kirigami.MessageType.Information
                text: i18n("An older event-based schedule is saved. Saving this page replaces it with the time windows shown below.")
            }

            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                text: i18n("Each enabled profile uses its limit between the selected 24-hour start and end times. An end time before the start time means the next day. Profiles cannot overlap.")
            }

            Controls.Switch {
                id: enabledSwitch
                text: i18n("Enable schedule")
                enabled: !kcm.busy
            }

            RowLayout {
                Layout.fillWidth: true
                Controls.Label { text: i18n("Outside profile hours") }
                Controls.SpinBox {
                    id: outsideLimitControl
                    from: 25; to: 100
                    value: 100
                    enabled: !kcm.busy
                    Accessible.name: i18n("Outside-hours charge limit percentage")
                }
                Controls.Label { text: "%" }
                Item { Layout.fillWidth: true }
            }

            Repeater {
                model: scheduleModel
                delegate: Controls.Frame {
                    id: entryFrame
                    required property int index
                    required property string profileName
                    required property bool profileEnabled
                    required property int days
                    required property int startMinute
                    required property int endMinute
                    required property int limit
                    readonly property bool overnight: endMinute < startMinute
                    Layout.fillWidth: true

                    ColumnLayout {
                        anchors.fill: parent
                        spacing: Kirigami.Units.smallSpacing

                        RowLayout {
                            Layout.fillWidth: true
                            Controls.TextField {
                                text: entryFrame.profileName
                                placeholderText: i18n("Profile name")
                                maximumLength: 48
                                Layout.fillWidth: true
                                enabled: !kcm.busy
                                Accessible.name: i18n("Profile name")
                                onEditingFinished: scheduleModel.setProperty(entryFrame.index, "profileName", text.trim())
                            }
                            Controls.Switch {
                                text: i18n("Enabled")
                                checked: entryFrame.profileEnabled
                                enabled: !kcm.busy
                                onToggled: scheduleModel.setProperty(entryFrame.index, "profileEnabled", checked)
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
                            Controls.Label { text: i18n("Start") }
                            Controls.SpinBox {
                                from: 0; to: 23
                                value: Math.floor(entryFrame.startMinute / 60)
                                textFromValue: function(value) { return String(value).padStart(2, "0") }
                                enabled: !kcm.busy
                                Accessible.name: i18n("Start hour")
                                onValueModified: scheduleModel.setProperty(entryFrame.index, "startMinute", value * 60 + entryFrame.startMinute % 60)
                            }
                            Controls.Label { text: ":" }
                            Controls.SpinBox {
                                from: 0; to: 59
                                value: entryFrame.startMinute % 60
                                textFromValue: function(value) { return String(value).padStart(2, "0") }
                                enabled: !kcm.busy
                                Accessible.name: i18n("Start minute")
                                onValueModified: scheduleModel.setProperty(entryFrame.index, "startMinute", Math.floor(entryFrame.startMinute / 60) * 60 + value)
                            }
                            Item { Layout.fillWidth: true }
                            Controls.Label { text: i18n("End") }
                            Controls.SpinBox {
                                from: 0; to: 23
                                value: Math.floor(entryFrame.endMinute / 60)
                                textFromValue: function(value) { return String(value).padStart(2, "0") }
                                enabled: !kcm.busy
                                Accessible.name: i18n("End hour")
                                onValueModified: scheduleModel.setProperty(entryFrame.index, "endMinute", value * 60 + entryFrame.endMinute % 60)
                            }
                            Controls.Label { text: ":" }
                            Controls.SpinBox {
                                from: 0; to: 59
                                value: entryFrame.endMinute % 60
                                textFromValue: function(value) { return String(value).padStart(2, "0") }
                                enabled: !kcm.busy
                                Accessible.name: i18n("End minute")
                                onValueModified: scheduleModel.setProperty(entryFrame.index, "endMinute", Math.floor(entryFrame.endMinute / 60) * 60 + value)
                            }
                        }

                        Controls.RangeSlider {
                            id: timeRange
                            Layout.fillWidth: true
                            from: 0
                            to: entryFrame.overnight ? 2879 : 1439
                            stepSize: 15
                            snapMode: Controls.RangeSlider.SnapAlways
                            first.value: entryFrame.startMinute
                            second.value: entryFrame.endMinute + (entryFrame.overnight ? 1440 : 0)
                            enabled: !kcm.busy
                            Accessible.name: i18n("Start and end time range")
                            first.onMoved: {
                                const proposed = Math.min(1425, Math.round(first.value / 15) * 15)
                                const latest = entryFrame.overnight ? 1425 : entryFrame.endMinute - 1
                                const earliest = entryFrame.overnight ? entryFrame.endMinute + 1 : 0
                                scheduleModel.setProperty(entryFrame.index, "startMinute", Math.max(earliest, Math.min(latest, proposed)))
                            }
                            second.onMoved: {
                                const proposed = Math.round((second.value - (entryFrame.overnight ? 1440 : 0)) / 15) * 15
                                const latest = entryFrame.overnight ? entryFrame.startMinute - 1 : 1425
                                const earliest = entryFrame.overnight ? 0 : entryFrame.startMinute + 1
                                scheduleModel.setProperty(entryFrame.index, "endMinute", Math.max(earliest, Math.min(latest, proposed)))
                            }
                        }

                        RowLayout {
                            Layout.fillWidth: true
                            Controls.Label {
                                visible: entryFrame.overnight
                                text: i18n("Ends next day")
                                opacity: 0.7
                            }
                            Item { Layout.fillWidth: true }
                            Controls.Label { text: i18n("Charge limit") }
                            Controls.SpinBox {
                                from: 25; to: 100
                                value: entryFrame.limit
                                enabled: !kcm.busy
                                Accessible.name: i18n("Profile charge limit percentage")
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
                    text: i18n("Add profile")
                    icon.name: "list-add"
                    enabled: !kcm.busy && scheduleModel.count < 32
                    onClicked: scheduleModel.append({
                        profileName: i18n("Profile %1", scheduleModel.count + 1),
                        profileEnabled: scheduleModel.count === 0,
                        days: 31, startMinute: 480, endMinute: 1020, limit: 80
                    })
                }
                Item { Layout.fillWidth: true }
                Controls.Button {
                    text: i18n("Save schedule")
                    enabled: !kcm.busy
                    onClicked: root.save()
                }
            }

        }
    }
}
