import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

RowLayout {
    id: root

    property string paramName: qsTr("パラメータ")
    property real minValue: 0
    property real maxValue: 100
    property real startValue: 0
    property real endValue: 0
    property int decimals: 0
    property string interpolationType: "constant"
    property bool isRangeMode: Math.abs(startValue - endValue) > 0.001
    property bool interpolationSelected: interpolationType !== "" && interpolationType !== "constant"
    property bool rightLinked: !interpolationSelected
    property bool rightInteractive: root.enabled && root.isRangeMode && root.interpolationSelected

    signal startValueModified(real value)
    signal endValueModified(real value)
    signal paramButtonClicked()

    function formatValue(val) {
        var num = Number(val);
        if (isNaN(num))
            num = 0;

        return root.decimals === 0 ? num.toFixed(0) : num.toFixed(root.decimals);
    }

    function syncRightDisplay(val) {
        var text = formatValue(val);
        if (rightValueField.text !== text)
            rightValueField.text = text;

    }

    function clearFieldFocus(field) {
        if (field && field.activeFocus) {
            field.ignoreNextEditingFinished = true;
            field.focus = false;
        }
    }

    function ignoreEditingFinished(field) {
        if (!field || !field.ignoreNextEditingFinished)
            return false;

        field.ignoreNextEditingFinished = false;
        return true;
    }

    function pushLeftValue(val) {
        var text = formatValue(val);
        if (leftValueField.text !== text)
            leftValueField.text = text;

        root.startValueModified(val);
        if (root.rightLinked) {
            syncRightDisplay(val);
            root.endValueModified(val);
        }
    }

    spacing: 8
    Component.onCompleted: {
        if (root.rightLinked)
            syncRightDisplay(root.startValue);

    }
    onStartValueChanged: {
        var text = formatValue(root.startValue);
        if (leftValueField.text !== text)
            leftValueField.text = text;

        if (root.rightLinked)
            syncRightDisplay(root.startValue);

    }
    onEndValueChanged: {
        if (root.rightLinked)
            return ;

        var text = formatValue(root.endValue);
        if (rightValueField.text !== text)
            rightValueField.text = text;

    }
    onInterpolationSelectedChanged: {
        if (root.rightLinked) {
            syncRightDisplay(root.startValue);
        } else {
            var text = formatValue(root.endValue);
            if (rightValueField.text !== text)
                rightValueField.text = text;

        }
    }

        Slider {
        id: leftSlider

        Layout.fillWidth: true
        Layout.preferredWidth: 120
        from: root.minValue
        to: root.maxValue
        enabled: root.enabled
        value: {
            var val = parseFloat(leftValueField.text);
            return isNaN(val) ? root.startValue : val;
        }
        onMoved: {
            var val = decimals === 0 ? Math.round(value) : value;
            root.pushLeftValue(val);
        }
    }

        TextField {
        id: leftValueField

        property bool ignoreNextEditingFinished: false

        Layout.preferredWidth: 70
        text: root.formatValue(root.startValue)
        horizontalAlignment: TextInput.AlignHCenter
        selectByMouse: true
        enabled: root.enabled
        onEditingFinished: {
            if (root.ignoreEditingFinished(leftValueField))
                return ;

            var newVal = parseFloat(text);
            if (!isNaN(newVal))
                root.pushLeftValue(newVal);

            root.clearFieldFocus(leftValueField);
        }

        validator: DoubleValidator {
            decimals: root.decimals
            notation: DoubleValidator.StandardNotation
        }

    }

        Button {
        id: paramButton

        Layout.preferredWidth: 100
        text: root.paramName
        enabled: root.enabled
        onClicked: root.paramButtonClicked()
    }

        TextField {
        id: rightValueField

        property bool ignoreNextEditingFinished: false

        Layout.preferredWidth: 70
        text: root.formatValue(root.rightLinked ? root.startValue : root.endValue)
        horizontalAlignment: TextInput.AlignHCenter
        selectByMouse: true
        enabled: root.rightInteractive
        opacity: root.rightInteractive ? 1 : 0.45
        onEditingFinished: {
            if (root.ignoreEditingFinished(rightValueField))
                return ;

            var newVal = parseFloat(text);
            if (!isNaN(newVal))
                root.endValueModified(newVal);

            root.clearFieldFocus(rightValueField);
        }

        validator: DoubleValidator {
            decimals: root.decimals
            notation: DoubleValidator.StandardNotation
        }

    }

        Slider {
        id: rightSlider

        Layout.fillWidth: true
        Layout.preferredWidth: 120
        from: root.minValue
        to: root.maxValue
        enabled: root.rightInteractive
        opacity: root.rightInteractive ? 1 : 0.45
                value: {
            var val = parseFloat(rightValueField.text);
            return isNaN(val) ? (root.rightLinked ? root.startValue : root.endValue) : val;
        }
        onMoved: {
                        var val = decimals === 0 ? Math.round(value) : value;
            rightValueField.text = root.formatValue(val);
            root.endValueModified(val);
        }
    }

}
