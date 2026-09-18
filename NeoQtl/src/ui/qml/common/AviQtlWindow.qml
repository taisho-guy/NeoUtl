import QtQuick
import QtQuick.Controls
import QtQuick.Window

ApplicationWindow {
    id: root

        property alias themePalette: systemPalette

        color: systemPalette.window

        SystemPalette {
        id: systemPalette

        colorGroup: SystemPalette.Active
    }

}
