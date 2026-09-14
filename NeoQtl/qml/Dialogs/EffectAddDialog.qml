import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Layouts 1.15

Dialog {
    id: root
    title: qsTr("エフェクトの追加")
    modal: true
    focus: true
    standardButtons: Dialog.Ok | Dialog.Cancel
    width: 480
    height: 420
    anchors.centerIn: parent

    background: Rectangle {
        color: "#1e2230"
        border.color: "#32384e"
        radius: 8
    }

    signal effectSelected(string effectId, string effectName)

    property var effectList: [
        { id: "blur", name: qsTr("ぼかし (Blur)"), category: qsTr("フィルター") },
        { id: "color_correction", name: qsTr("色調補正 (Color Correction)"), category: qsTr("カラー") },
        { id: "glow", name: qsTr("発光 (Glow)"), category: qsTr("フィルター") },
        { id: "drop_shadow", name: qsTr("ドロップシャドウ (Drop Shadow)"), category: qsTr("合成") },
        { id: "chroma_key", name: qsTr("クロマキー (Chroma Key)"), category: qsTr("キーイング") },
        { id: "mosaic", name: qsTr("モザイク (Mosaic)"), category: qsTr("フィルター") },
        { id: "directional_blur", name: qsTr("方向ブラー (Directional Blur)"), category: qsTr("フィルター") },
        { id: "displacement_map", name: qsTr("ディスプレイスメントマップ"), category: qsTr("変形") },
        { id: "vignette", name: qsTr("周辺減光 (Vignette)"), category: qsTr("カラー") }
    ]

    property string searchText: ""
    property int selectedIndex: 0

    contentItem: ColumnLayout {
        spacing: 12

        TextField {
            id: searchInput
            Layout.fillWidth: true
            placeholderText: qsTr("エフェクトを検索...")
            color: "white"
            background: Rectangle {
                color: "#141722"
                border.color: searchInput.activeFocus ? "#5e81ff" : "#2b3145"
                radius: 4
            }
            onTextChanged: root.searchText = text.toLowerCase()
        }

        ListView {
            id: effectListView
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: root.effectList.filter(function(item) {
                if (root.searchText === "") return true;
                return item.name.toLowerCase().indexOf(root.searchText) !== -1 ||
                       item.category.toLowerCase().indexOf(root.searchText) !== -1;
            })

            delegate: ItemDelegate {
                width: effectListView.width
                height: 40
                highlighted: root.selectedIndex === index

                background: Rectangle {
                    color: root.selectedIndex === index ? "#2d3752" : (hovered ? "#23293d" : "transparent")
                    radius: 4
                }

                contentItem: RowLayout {
                    spacing: 12
                    Label {
                        text: modelData.name
                        color: "white"
                        font.pixelSize: 13
                        Layout.fillWidth: true
                    }
                    Rectangle {
                        color: "#1b2030"
                        radius: 3
                        implicitWidth: catLabel.implicitWidth + 8
                        implicitHeight: catLabel.implicitHeight + 4
                        Label {
                            id: catLabel
                            anchors.centerIn: parent
                            text: modelData.category
                            color: "#8aa0c4"
                            font.pixelSize: 11
                        }
                    }
                }

                onClicked: root.selectedIndex = index
                onDoubleClicked: {
                    root.selectedIndex = index;
                    root.accept();
                }
            }
        }
    }

    onAccepted: {
        var filtered = effectList.filter(function(item) {
            if (root.searchText === "") return true;
            return item.name.toLowerCase().indexOf(root.searchText) !== -1;
        });
        if (filtered.length > 0 && selectedIndex >= 0 && selectedIndex < filtered.length) {
            var selected = filtered[selectedIndex];
            root.effectSelected(selected.id, selected.name);
        }
    }
}
