import QtQuick
import QtQuick.Controls

Menu {
    id: filterMenu

    property int targetClipId: -1

    Instantiator {
        model: Workspace.currentTimeline ? Workspace.currentTimeline.getPluginCategories() : []
        onObjectAdded: (index, object) => {
            return filterMenu.insertMenu(index, object);
        }
        onObjectRemoved: (index, object) => {
            return filterMenu.removeMenu(object);
        }

        delegate: Menu {
            id: categoryMenu

            title: modelData

            Instantiator {
                model: Workspace.currentTimeline.getPluginsByCategory(categoryMenu.title)
                onObjectAdded: (index, subObj) => {
                    return categoryMenu.insertItem(index, subObj);
                }
                onObjectRemoved: (index, subObj) => {
                    return categoryMenu.removeItem(subObj);
                }

                delegate: Common.IconMenuItem {
                    text: modelData.name
                    iconName: "music_line"
                    onTriggered: {
                        if (Workspace.currentTimeline)
                            Workspace.currentTimeline.addAudioPlugin(targetClipId, modelData.id);

                    }
                }

            }

        }

    }

}
