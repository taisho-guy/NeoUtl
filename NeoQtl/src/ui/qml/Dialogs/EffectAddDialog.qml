import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import ".."
import "../Components"

Window {
    id: dialog

    function open() { visible = true }
    function close() { visible = false }
    width: 420
    height: 560
    title: "エフェクト追加"
    color: Theme.bg
    modality: Qt.WindowModal

    signal effectChosen(string effectId)

    property var effects: [
        { id: "border_blur", name: "境界ぼかし", category: "ぼかし" },
        { id: "directional_blur", name: "方向ブラー", category: "ぼかし" },
        { id: "radial_blur", name: "放射ブラー", category: "ぼかし" },
        { id: "color_correction", name: "色調補正", category: "色調整" },
        { id: "chromatic_aberration", name: "色収差", category: "色調整" },
        { id: "drop_shadow", name: "ドロップシャドウ", category: "描画" },
        { id: "diffuse_light", name: "拡散光", category: "描画" },
        { id: "mosaic", name: "モザイク", category: "特殊効果" },
        { id: "pixel_sorter", name: "ピクセルソーター", category: "特殊効果" },
        { id: "clipping", name: "クリッピング", category: "変形" },
        { id: "transform", name: "トランスフォーム", category: "変形" }
    ]
    property string searchText: ""
    property string categoryFilter: "全て"
    readonly
    property var categories: ["全て", "ぼかし", "色調整", "描画", "特殊効果", "変形"]

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 16
        spacing: 12

                NeoTextInput {
            id: searchInput
            Layout.fillWidth: true
            placeholderText: "エフェクト名を検索…"
            onTextChanged: dialog.searchText = text
        }

                ScrollView {
            Layout.fillWidth: true
            height: 32
            clip: true

            RowLayout {
                spacing: 6
                Repeater {
                    model: dialog.categories
                    delegate: NeoButton {
                        text: modelData
                        outline: (dialog.categoryFilter !== modelData)
                        implicitHeight: 26
                        onClicked: dialog.categoryFilter = modelData
                    }
                }
            }
        }

        Rectangle {
            Layout.fillWidth: true
            height: 1
            color: Theme.border
        }

                ScrollView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true

            ListView {
                id: effectListView
                width: parent.width
                spacing: 4
                model: dialog.effects

                delegate: Rectangle {
                    visible: {
                        var matchSearch = (dialog.searchText === "") ||
                                          modelData.name.indexOf(dialog.searchText) !== -1 ||
                                          modelData.id.indexOf(dialog.searchText) !== -1;
                        var matchCat = (dialog.categoryFilter === "全て") || (modelData.category === dialog.categoryFilter);
                        return matchSearch && matchCat;
                    }
                    width: effectListView.width
                    height: visible ? 36 : 0
                    radius: Theme.radiusSm
                    color: mouseArea.pressed ? Theme.cardHover :
                           mouseArea.containsMouse ? Qt.rgba(255, 255, 255, 0.05) : Theme.card
                    border.color: mouseArea.containsMouse ? Theme.textMuted : Theme.border
                    border.width: 1

                    RowLayout {
                        anchors.fill: parent
                        anchors.leftMargin: 12
                        anchors.rightMargin: 12

                        Text {
                            text: modelData.name
                            color: Theme.text
                            font.family: Theme.fontFamily
                            font.pixelSize: 13
                        }

                        Item { Layout.fillWidth: true }

                        Rectangle {
                            width: catText.implicitWidth + 10
                            height: 20
                            radius: 3
                            color: Qt.rgba(99/255, 102/255, 241/255, 0.15)
                            Text {
                                id: catText
                                anchors.centerIn: parent
                                text: modelData.category
                                color: Theme.accent
                                font.family: Theme.fontFamily
                                font.pixelSize: 11
                            }
                        }
                    }

                    MouseArea {
                        id: mouseArea
                        anchors.fill: parent
                        hoverEnabled: true
                        onClicked: {
                            dialog.effectChosen(modelData.id);
                            dialog.close();
                        }
                    }
                }
            }
        }

                RowLayout {
            Layout.fillWidth: true
            Item { Layout.fillWidth: true }
            NeoButton {
                outline: true
                text: "閉じる"
                onClicked: dialog.close()
            }
        }
    }
}
