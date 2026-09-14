import QtQuick
import QtQuick.Controls
import "./Dialogs"

QtObject {
    id: root

        property LauncherWindow launcherWindow: LauncherWindow {
        id: launcher
        visible: true

        onProjectOpened: {
            launcher.visible = false
            previewWindow.visible = true
            timelineWindow.visible = true
            propertiesWindow.visible = true
        }

        onRequestSystemSettings: systemSettingsDialog.open()
    }

    property PreviewWindow previewWindow: PreviewWindow {
        id: previewWindow
        visible: false

                onOpenProjectSettingsRequested: projectSettingsDialog.open()
        onOpenSceneSettingsRequested: sceneSettingsDialog.open()
        onOpenSystemSettingsRequested: systemSettingsDialog.open()
        onOpenKeybindingsRequested: keybindingsDialog.open()
        onOpenExportRequested: exportDialog.open()
        onOpenTimelineRequested: timelineWindow.visible = true
        onOpenPropertiesRequested: propertiesWindow.visible = true

                onSeekRequested: (frame) => {
            timelineWindow.currentFrame = frame
        }
        onTogglePlayRequested: {
            timelineWindow.isPlaying = !timelineWindow.isPlaying
        }
    }

    property TimelineWindow timelineWindow: TimelineWindow {
        id: timelineWindow
        visible: false

        onSeekRequested: (frame) => {
            previewWindow.currentFrame = frame
        }

        onClipSelected: (clipId) => {
            propertiesWindow.targetObjectId = clipId
        }
    }

    property PropertiesWindow propertiesWindow: PropertiesWindow {
        id: propertiesWindow
        visible: false

        onOpenEffectAddRequested: effectAddDialog.open()
        onOpenEasingEditorRequested: (paramKey) => easingEditorDialog.open()
    }

        property SystemSettingsDialog systemSettingsDialog: SystemSettingsDialog {
        id: systemSettingsDialog
    }

    property ProjectSettingsDialog projectSettingsDialog: ProjectSettingsDialog {
        id: projectSettingsDialog
    }

    property SceneSettingsDialog sceneSettingsDialog: SceneSettingsDialog {
        id: sceneSettingsDialog
    }

    property KeybindingsDialog keybindingsDialog: KeybindingsDialog {
        id: keybindingsDialog
    }

    property ExportDialog exportDialog: ExportDialog {
        id: exportDialog
    }

    property EffectAddDialog effectAddDialog: EffectAddDialog {
        id: effectAddDialog
    }

    property EasingEditorDialog easingEditorDialog: EasingEditorDialog {
        id: easingEditorDialog
    }
}
