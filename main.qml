import QtQuick
import QtQuick.Controls
import QtQuick.Controls.Material
import QtQuick.Layouts

ApplicationWindow {
    id: window
    width: 1100
    height: 720
    title: "Z&S-AI"
    visible: true

    // M3 调色板
    M3Colors { id: m3 }

    Material.theme: Material.Light
    Material.accent: m3.primary
    Material.background: m3.background

    ListModel { id: chatModel }

    Component.onCompleted: {
        var h = backend.loadChatHistory()
        for (var i = 0; i < h.length; i++)
            chatModel.append(h[i])
    }

    Connections {
        target: backend
        function onResponseReady(c) {
            chatModel.append({ role: "AI", content: c })
        }
        function onErrorOccurred(t, m) {
            chatModel.append({ role: "Error", content: t + ": " + m })
        }
        function onHistoryCleared() {
            chatModel.clear()
        }
    }

    header: ToolBar {
        Material.elevation: 0
        Material.background: m3.inverseSurface
        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: 12
            TabBar {
                id: tabBar
                Layout.alignment: Qt.AlignLeft
                background: null
                TabButton {
                    text: "\u{1f4cb}  \u{89d2}\u{8272}\u{5361}"
                    Material.accent: m3.inversePrimary
                }
                TabButton {
                    text: "\u{1f4ac}  \u{5bf9}\u{8bdd}"
                    Material.accent: m3.inversePrimary
                }
            }
            Item { Layout.fillWidth: true }
            ToolButton {
                text: "\u{6587}\u{4ef6}"
                onClicked: fileMenu.open()
                Material.foreground: m3.txtInverseSurface
                Menu {
                    id: fileMenu
                    MenuItem { text: "\u{4fdd}\u{5b58}\u{914d}\u{7f6e}"; onTriggered: saveDialog.open() }
                    MenuItem { text: "\u{52a0}\u{8f7d}\u{914d}\u{7f6e}"; onTriggered: loadDialog.open() }
                    MenuSeparator {}
                    MenuItem { text: "\u{9000}\u{51fa}"; onTriggered: Qt.quit() }
                }
            }
            ToolButton {
                text: "\u{8bbe}\u{7f6e}"
                onClicked: settingsDialog.open()
                Material.foreground: m3.txtInverseSurface
            }
            ToolButton {
                text: "\u{5de5}\u{5177}"
                onClicked: toolsMenu.open()
                Material.foreground: m3.txtInverseSurface
                Menu {
                    id: toolsMenu
                    MenuItem { text: "\u{67e5}\u{770b}\u{7cfb}\u{7edf}\u{63d0}\u{793a}\u{8bcd}"; onTriggered: promptDialog.open() }
                    MenuItem { text: "\u{6e05}\u{7a7a}\u{5bf9}\u{8bdd}\u{5386}\u{53f2}"; onTriggered: clearConfirm.open() }
                }
            }
            ToolButton {
                text: "\u{5e2e}\u{52a9}"
                onClicked: aboutDialog.open()
                Material.foreground: m3.txtInverseSurface
            }
        }
    }

    StackLayout {
        anchors.fill: parent
        anchors.margins: 8
        currentIndex: tabBar.currentIndex

        ScrollView {
            clip: true
            ScrollBar.vertical.policy: ScrollBar.AsNeeded
            Flickable {
                contentWidth: parent.width
                contentHeight: charCol.implicitHeight + 20
                boundsBehavior: Flickable.StopAtBounds
                ColumnLayout {
                    id: charCol
                    width: parent.width - 16
                    x: 8
                    spacing: 12

                    Frame {
                        Material.elevation: 1
                        Layout.fillWidth: true
                        padding: 16
                        background: Rectangle {
                            color: m3.surfaceContainerLow
                            radius: m3.shapeLarge
                            border.color: m3.outlineVar
                        }
                        ColumnLayout {
                            spacing: 10
                            Label {
                                text: "\u{89d2}\u{8272}\u{5c5e}\u{6027}"
                                font.bold: true
                                font.pixelSize: 13
                            }
                            RowLayout {
                                Label { text: "\u{540d}\u{79f0}\u{ff1a}"; Layout.preferredWidth: 70 }
                                TextField {
                                    Layout.fillWidth: true
                                    placeholderText: "\u{89d2}\u{8272}\u{540d}\u{79f0}"
                                    text: backend.characterName
                                    onTextEdited: backend.characterName = text
                                }
                            }
                            RowLayout {
                                Label { text: "\u{6027}\u{683c}\u{ff1a}"; Layout.preferredWidth: 70 }
                                TextField {
                                    Layout.fillWidth: true
                                    placeholderText: "\u{5982}\u{ff1a}\u{6d3b}\u{6cfc}\u{3001}\u{6e29}\u{67d4}\u{3001}\u{51b7}\u{9177}"
                                    text: backend.characterPersonality
                                    onTextEdited: backend.characterPersonality = text
                                }
                            }
                            RowLayout {
                                Label { text: "\u{8bf4}\u{8bdd}\u{98ce}\u{683c}\u{ff1a}"; Layout.preferredWidth: 70 }
                                TextField {
                                    Layout.fillWidth: true
                                    placeholderText: "\u{5982}\u{ff1a}\u{559c}\u{6b22}\u{7528}\u{53e4}\u{98ce}\u{53e5}\u{5f0f}"
                                    text: backend.characterSpeakingStyle
                                    onTextEdited: backend.characterSpeakingStyle = text
                                }
                            }
                            RowLayout {
                                Label { text: "\u{76ee}\u{6807}\u{ff1a}"; Layout.preferredWidth: 70 }
                                TextField {
                                    Layout.fillWidth: true
                                    placeholderText: "\u{5982}\u{ff1a}\u{5e2e}\u{52a9}\u{4e3b}\u{89d2}\u{89e3}\u{5f00}\u{8c1c}\u{9898}"
                                    text: backend.characterGoals
                                    onTextEdited: backend.characterGoals = text
                                }
                            }
                        }
                    }

                    Frame {
                        Material.elevation: 1
                        Layout.fillWidth: true
                        padding: 16
                        background: Rectangle {
                            color: m3.surfaceContainerLow
                            radius: m3.shapeLarge
                            border.color: m3.outlineVar
                        }
                        ColumnLayout {
                            spacing: 10
                            Label {
                                text: "\u{80cc}\u{666f}\u{4e0e}\u{573a}\u{666f}"
                                font.bold: true
                                font.pixelSize: 13
                            }
                            ColumnLayout {
                                spacing: 2
                                Layout.fillWidth: true
                                Label { text: "\u{80cc}\u{666f}\u{ff1a}" }
                                ScrollView {
                                    Layout.fillWidth: true
                                    Layout.preferredHeight: 70
                                    TextArea {
                                        placeholderText: "\u{89d2}\u{8272}\u{7684}\u{80cc}\u{666f}\u{6545}\u{4e8b}\u{2026}\u{2026}"
                                        wrapMode: TextArea.Wrap
                                        text: backend.characterBackground
                                        onTextEdited: backend.characterBackground = text
                                    }
                                }
                            }
                            RowLayout {
                                Label { text: "\u{573a}\u{666f}\u{ff1a}"; Layout.preferredWidth: 70 }
                                TextField {
                                    Layout.fillWidth: true
                                    placeholderText: "\u{5982}\u{ff1a}\u{53e4}\u{57ce}\u{5e9f}\u{589f}\u{3001}\u{661f}\u{9645}\u{98de}\u{8239}"
                                    text: backend.scene
                                    onTextEdited: backend.scene = text
                                }
                            }
                            RowLayout {
                                Label { text: "\u{65f6}\u{95f4}\u{ff1a}"; Layout.preferredWidth: 70 }
                                TextField {
                                    Layout.fillWidth: true
                                    placeholderText: "\u{5982}\u{ff1a}\u{9ec4}\u{660f}\u{3001}\u{516c}\u{5143}3024\u{5e74}"
                                    text: backend.time
                                    onTextEdited: backend.time = text
                                }
                            }
                        }
                    }

                    Frame {
                        Material.elevation: 1
                        Layout.fillWidth: true
                        padding: 16
                        background: Rectangle {
                            color: m3.surfaceContainerLow
                            radius: m3.shapeLarge
                            border.color: m3.outlineVar
                        }
                        ColumnLayout {
                            spacing: 10
                            Label {
                                text: "\u{8bb0}\u{5fc6}\u{4e0e}\u{72b6}\u{6001}"
                                font.bold: true
                                font.pixelSize: 13
                            }
                            ColumnLayout {
                                spacing: 2
                                Layout.fillWidth: true
                                Label { text: "\u{957f}\u{671f}\u{8bb0}\u{5fc6}\u{ff1a}" }
                                ScrollView {
                                    Layout.fillWidth: true
                                    Layout.preferredHeight: 70
                                    TextArea {
                                        placeholderText: "\u{89d2}\u{8272}\u{957f}\u{671f}\u{8bb0}\u{5fc6}\u{2026}\u{2026}"
                                        wrapMode: TextArea.Wrap
                                        text: backend.memory
                                        onTextEdited: backend.memory = text
                                    }
                                }
                            }
                            ColumnLayout {
                                spacing: 2
                                Layout.fillWidth: true
                                Label { text: "\u{72b6}\u{6001}\u{8ffd}\u{8e2a}\u{ff1a}" }
                                ScrollView {
                                    Layout.fillWidth: true
                                    Layout.preferredHeight: 70
                                    TextArea {
                                        placeholderText: "\u{597d}\u{611f}\u{5ea6} / \u{4f4d}\u{7f6e} / \u{53d8}\u{91cf}"
                                        wrapMode: TextArea.Wrap
                                        text: backend.frankensteinState
                                        onTextEdited: backend.frankensteinState = text
                                    }
                                }
                            }
                            ColumnLayout {
                                spacing: 2
                                Layout.fillWidth: true
                                Label { text: "\u{989d}\u{5916}\u{6307}\u{4ee4}\u{ff1a}" }
                                ScrollView {
                                    Layout.fillWidth: true
                                    Layout.preferredHeight: 70
                                    TextArea {
                                        placeholderText: "\u{989d}\u{5916}\u{6307}\u{4ee4} / \u{9ad8}\u{9636}\u{53d9}\u{4e8b}\u{89c4}\u{5219}\u{2026}\u{2026}"
                                        wrapMode: TextArea.Wrap
                                        text: backend.extraCommend
                                        onTextEdited: backend.extraCommend = text
                                    }
                                }
                            }
                        }
                    }

                    Frame {
                        Material.elevation: 1
                        Layout.fillWidth: true
                        padding: 16
                        background: Rectangle {
                            color: m3.surfaceContainerLow
                            radius: m3.shapeLarge
                            border.color: m3.outlineVar
                        }
                        ColumnLayout {
                            spacing: 10
                            Label {
                                text: "\u{793a}\u{4f8b}\u{5bf9}\u{8bdd}"
                                font.bold: true
                                font.pixelSize: 13
                            }
                            ColumnLayout {
                                spacing: 2
                                Layout.fillWidth: true
                                Label { text: "\u{793a}\u{4f8b}\u{ff1a}" }
                                ScrollView {
                                    Layout.fillWidth: true
                                    Layout.preferredHeight: 100
                                    TextArea {
                                        placeholderText: "Few-shot \u{793a}\u{4f8b}\u{5bf9}\u{8bdd}"
                                        wrapMode: TextArea.Wrap
                                        text: backend.exampleDialogues
                                        onTextEdited: backend.exampleDialogues = text
                                    }
                                }
                            }
                        }
                    }

                    RowLayout {
                        spacing: 8
                        Button { text: "\u{4fdd}\u{5b58}\u{914d}\u{7f6e}"; onClicked: saveDialog.open() }
                        Button { text: "\u{52a0}\u{8f7d}\u{914d}\u{7f6e}"; onClicked: loadDialog.open() }
                        Button { text: "\u{67e5}\u{770b} Prompt"; onClicked: promptDialog.open() }
                        Item { Layout.fillWidth: true }
                    }
                }
            }
        }

        ColumnLayout {
            spacing: 0
            ListView {
                id: chatView
                Layout.fillWidth: true
                Layout.fillHeight: true
                model: chatModel
                clip: true
                spacing: 6
                topMargin: 6
                bottomMargin: 6
                onCountChanged: positionViewAtEnd()

                delegate: Item {
                    width: ListView.view.width
                    height: bubbleRect.height + 8
                    Rectangle {
                        id: bubbleRect
                        radius: 12
                        width: Math.min(bubbleLabel.implicitWidth + 24, parent.width * 0.75)
                        height: bubbleLabel.implicitHeight + 16
                        color: model.role === "User" ? Material.color(Material.Blue, Material.Shade600)
                             : model.role === "Error" ? "#fff3cd" : "#ffffff"
                        border.color: model.role === "AI" ? "#d0d7de"
                                     : model.role === "Error" ? "#ffe69c" : "transparent"
                        border.width: (model.role === "User" || model.role === "Error") ? 0 : 1
                        anchors {
                            right: model.role === "User" ? parent.right : undefined
                            left: model.role !== "User" ? parent.left : undefined
                            rightMargin: model.role === "User" ? 8 : 0
                            leftMargin: model.role !== "User" ? 8 : 0
                            verticalCenter: parent.verticalCenter
                        }
                        Material.elevation: model.role === "User" ? 4 : 2
                        Label {
                            id: bubbleLabel
                            text: model.content
                            color: model.role === "User" ? "#fff" : model.role === "Error" ? "#664d03" : "#1e1e1e"
                            wrapMode: Text.Wrap
                            anchors.centerIn: parent
                            font.pixelSize: 13
                            textFormat: Text.PlainText
                        }
                    }
                }
            }
            Rectangle {
                Layout.fillWidth: true
                height: 1
                color: m3.outlineVar
            }
            RowLayout {
                Layout.fillWidth: true
                Layout.margins: 8
                spacing: 8
                TextField {
                    id: chatInput
                    Layout.fillWidth: true
                    placeholderText: "\u{8f93}\u{5165}\u{6d88}\u{606f}\u{2026}\u{2026}\u{ff08}Enter \u{53d1}\u{9001}\u{ff09}"
                    enabled: !backend.busy
                    onAccepted: doSend()
                }
                Button {
                    text: backend.busy ? "\u{53d1}\u{9001}\u{4e2d}\u{2026}" : "\u{53d1}\u{9001}"
                    enabled: !backend.busy && backend.hasApiKey && chatInput.text.trim().length > 0
                    highlighted: true
                    Material.background: m3.primary
                    onClicked: doSend()
                }
            }
        }
    }

    footer: ToolBar {
        Material.elevation: 4
        Material.background: m3.inverseSurface
        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: 12
            Label {
                text: backend.hasApiKey ? "\u{1f511} API \u{5df2}\u{5c31}\u{7eea}" : "\u{1f512} API Key \u{672a}\u{914d}\u{7f6e}"
                color: "#cccccc"
                font.pixelSize: 12
            }
            Label {
                text: "  |  \u{1f4e6} " + backend.modelDisplay + "  |  Z&S-AI"
                color: "#cccccc"
                font.pixelSize: 12
            }
        }
    }

    function doSend() {
        var t = chatInput.text.trim()
        if (t === "" || backend.busy) return
        if (!backend.hasApiKey) {
            chatModel.append({ role: "Error", content: "API Key \u{672a}\u{8bbe}\u{7f6e}" })
            return
        }
        chatModel.append({ role: "User", content: t })
        chatInput.text = ""
        backend.sendMessage(t)
    }

    Dialog {
        id: saveDialog
        title: "\u{4fdd}\u{5b58}\u{914d}\u{7f6e}"
        standardButtons: Dialog.Ok | Dialog.Cancel
        anchors.centerIn: parent
        width: 450
        ColumnLayout {
            spacing: 10
            Label { text: "\u{6587}\u{4ef6}\u{540d}\u{ff1a}" }
            TextField {
                id: savePath
                text: "role.conf"
                Layout.fillWidth: true
            }
        }
        onAccepted: {
            resultLabel.text = backend.saveConfig(savePath.text) ? "\u{2705} \u{5df2}\u{4fdd}\u{5b58}" : "\u{274c} \u{5931}\u{8d25}"
            resultDialog.open()
        }
    }

    Dialog {
        id: loadDialog
        title: "\u{52a0}\u{8f7d}\u{914d}\u{7f6e}"
        standardButtons: Dialog.Ok | Dialog.Cancel
        anchors.centerIn: parent
        width: 450
        ColumnLayout {
            spacing: 10
            Label { text: "\u{6587}\u{4ef6}\u{540d}\u{ff1a}" }
            TextField {
                id: loadPath
                text: "role.conf"
                Layout.fillWidth: true
            }
        }
        onAccepted: {
            if (backend.loadConfig(loadPath.text)) {
                chatModel.clear()
                var h = backend.loadChatHistory()
                for (var i = 0; i < h.length; i++)
                    chatModel.append(h[i])
                resultLabel.text = "\u{2705} \u{5df2}\u{52a0}\u{8f7d}"
            } else {
                resultLabel.text = "\u{274c} \u{5931}\u{8d25}"
            }
            resultDialog.open()
        }
    }

    Dialog {
        id: resultDialog
        title: "\u{64cd}\u{4f5c}\u{7ed3}\u{679c}"
        standardButtons: Dialog.Ok
        anchors.centerIn: parent
        Label {
            id: resultLabel
            text: ""
            wrapMode: Text.Wrap
            width: 350
        }
    }

    Dialog {
        id: settingsDialog
        title: "API \u{8bbe}\u{7f6e}"
        standardButtons: Dialog.Ok | Dialog.Cancel
        anchors.centerIn: parent
        width: 480
        ColumnLayout {
            spacing: 10
            Label { text: "API Key\u{ff1a}" }
            TextField {
                id: apiKeyF
                text: backend.apiKey
                echoMode: TextInput.Password
                Layout.fillWidth: true
                placeholderText: "sk-xxx"
            }
            Label { text: "Base URL\u{ff1a}" }
            TextField {
                id: baseUrlF
                text: backend.baseUrl
                Layout.fillWidth: true
                placeholderText: "https://api.deepseek.com"
            }
            Label { text: "\u{6a21}\u{578b}\u{ff1a}" }
            ComboBox {
                id: modelF
                Layout.fillWidth: true
                editable: true
                model: [
                    "deepseek-v4-flash",
                    "deepseek-v4-pro",
                    "deepseek-chat (\u{5df2}\u{5f03}\u{7528})",
                    "deepseek-reasoner (\u{5df2}\u{5f03}\u{7528})"
                ]
                Component.onCompleted: {
                    var found = false
                    for (var i = 0; i < model.length; i++) {
                        if (model[i].indexOf(backend.model) === 0) {
                            currentIndex = i
                            found = true
                            break
                        }
                    }
                    if (!found && backend.model !== "")
                        editText = backend.model
                }
            }
            Label { text: "Reasoning Effort\u{ff1a}" }
            TextField {
                id: reasonF
                text: backend.reasoningEffort
                Layout.fillWidth: true
                placeholderText: "medium"
            }
            Label { text: "Thinking Type\u{ff1a}" }
            TextField {
                id: thinkF
                text: backend.thinkingType
                Layout.fillWidth: true
                placeholderText: "enabled"
            }
        }
        onAccepted: {
            backend.apiKey = apiKeyF.text
            backend.baseUrl = baseUrlF.text
            var m = modelF.currentText
            if (m.indexOf("\u{5df2}\u{5f03}\u{7528}") >= 0) m = m.split(" ")[0]
            backend.model = m
            backend.reasoningEffort = reasonF.text
            backend.thinkingType = thinkF.text
            backend.saveConfig("role.conf")
        }
    }

    Dialog {
        id: promptDialog
        title: "\u{7cfb}\u{7edf}\u{63d0}\u{793a}\u{8bcd}"
        standardButtons: Dialog.Ok
        anchors.centerIn: parent
        width: 700
        height: 500
        ScrollView {
            anchors.fill: parent
            TextArea {
                text: backend.buildSystemPrompt()
                readOnly: true
                font.family: "monospace"
                wrapMode: TextArea.Wrap
            }
        }
    }

    Dialog {
        id: clearConfirm
        title: "\u{6e05}\u{7a7a}\u{5386}\u{53f2}"
        standardButtons: Dialog.Yes | Dialog.No
        anchors.centerIn: parent
        Label {
            text: "\u{786e}\u{5b9a}\u{8981}\u{6e05}\u{7a7a}\u{6240}\u{6709}\u{5bf9}\u{8bdd}\u{5386}\u{53f2}\u{5417}\u{ff1f}\n\u{ff08}\u{89d2}\u{8272}\u{5361}\u{8bbe}\u{7f6e}\u{4e0d}\u{53d7}\u{5f71}\u{54cd}\u{ff09}"
            wrapMode: Text.Wrap
            width: 350
        }
        onAccepted: backend.clearHistory()
    }

    Dialog {
        id: aboutDialog
        title: "\u{5173}\u{4e8e} Z&S-AI"
        standardButtons: Dialog.Ok
        anchors.centerIn: parent
        ColumnLayout {
            spacing: 8
            width: 350
            Label {
                text: "Z&S-AI v2.0"
                font.bold: true
                font.pixelSize: 16
            }
            Label {
                text: "\u{57fa}\u{4e8e} DeepSeek API \u{7684}\u{89d2}\u{8272}\u{626e}\u{6f14}\u{5bf9}\u{8bdd}\u{5de5}\u{5177}"
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
            Label {
                text: "\u{4f7f}\u{7528} Qt6 + QML (Material Design) \u{6784}\u{5efa}"
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
            Label {
                text: "\u{6a21}\u{578b}\u{ff1a}deepseek-v4-flash / deepseek-v4-pro"
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
            Label {
                text: "API\u{ff1a}https://api.deepseek.com"
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
        }
    }
}
