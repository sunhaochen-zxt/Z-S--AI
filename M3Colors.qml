import QtQuick

QtObject {
    // Primary
    readonly property color primary:            "#6750A4"
    readonly property color txtPrimary:         "#FFFFFF"
    readonly property color primaryContainer:   "#EADDFF"
    readonly property color txtPrimaryContainer:"#21005D"

    // Surface
    readonly property color surface:            "#FFFBFE"
    readonly property color surfaceDim:         "#E7E0EC"
    readonly property color surfaceContainerLow:"#F6F2FF"
    readonly property color txtSurface:         "#1C1B1F"
    readonly property color txtSurfaceVariant:  "#49454F"

    // Outline
    readonly property color outlineVar:         "#CAC4D0"

    // Inverse
    readonly property color inverseSurface:     "#313033"
    readonly property color txtInverseSurface:  "#F4EFF4"
    readonly property color inversePrimary:     "#D0BCFF"

    // Radius
    readonly property int shapeLarge: 16
    readonly property int shapeFull:  999
}
