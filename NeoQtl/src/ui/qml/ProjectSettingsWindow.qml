import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "common" as Common

Common.AviQtlWindow {
    id: root

    width: 450
    height: 200
    title: qsTr("プロジェクト設定")
        onVisibleChanged: {
        if (visible && Workspace.currentTimeline && Workspace.currentTimeline.project) {
            widthField.text = Workspace.currentTimeline.project.width;
            heightField.text = Workspace.currentTimeline.project.height;
            fpsField.text = Workspace.currentTimeline.project.fps;
            sampleRateField.text = Workspace.currentTimeline.project.sampleRate;
        }
    }

    ColumnLayout {
        anchors.centerIn: parent
        spacing: 15

        GridLayout {
            columns: 2
            rowSpacing: 10
            columnSpacing: 10

            Label {
                text: qsTr("幅:")
            }

            TextField {
                id: widthField

                selectByMouse: true

                validator: IntValidator {
                    bottom: 1
                    top: 8000
                }

            }

            Label {
                text: qsTr("高さ:")
            }

            TextField {
                id: heightField

                selectByMouse: true

                validator: IntValidator {
                    bottom: 1
                    top: 8000
                }

            }

            Label {
                text: qsTr("FPS:")
            }

            TextField {
                id: fpsField

                selectByMouse: true

                validator: DoubleValidator {
                    bottom: 1
                    top: 240
                }

            }

            Label {
                text: qsTr("サンプリングレート:")
            }

            TextField {
                id: sampleRateField

                selectByMouse: true

                validator: IntValidator {
                    bottom: 8000
                    top: 192000
                }

            }

        }

        RowLayout {
            Layout.alignment: Qt.AlignHCenter

            Button {
                text: qsTr("適用")
                highlighted: true
                onClicked: {
                    if (Workspace.currentTimeline && Workspace.currentTimeline.project) {
                        Workspace.currentTimeline.project.width = parseInt(widthField.text);
                        Workspace.currentTimeline.project.height = parseInt(heightField.text);
                        Workspace.currentTimeline.project.fps = parseFloat(fpsField.text);
                        Workspace.currentTimeline.project.sampleRate = parseInt(sampleRateField.text);
                        root.hide();
                    }
                }
            }

            Button {
                text: qsTr("キャンセル")
                onClicked: root.hide()
            }

        }

    }

}
