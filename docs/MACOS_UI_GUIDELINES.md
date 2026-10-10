# macOS adaptation of the shared UI design

## Authority

Read [UI_DESIGN.md](UI_DESIGN.md) first. It contains the required UI philosophy, component
behavior, layout, visual hierarchy and acceptance cases for every platform. This document
contains macOS implementation choices only; it must not become a second copy of the shared
policy. User corrections update the shared contract so Windows and Linux follow the same outcome.

## Native controls and materials

Use SwiftUI and AppKit. Search categories use a native segmented Picker and the query field
uses a capsule shape. Do not substitute custom text tabs, underline selection or hand-drawn
selected pills for the system selector. If a required native control is not exposed by SwiftUI,
use an NSViewRepresentable. On macOS 26 use Liquid Glass; on earlier supported releases use
native material and control appearance. Reduced transparency uses a readable native fallback.
Do not imitate Liquid Glass with an unrelated opaque plate.

The full player sets a dark appearance for its own surface and every top/bottom control capsule.
When controls supply their own glass, hide the additional shared toolbar backing where the SDK
supports it. Preserve native titlebar dragging; a slider placed in that region must own its
interaction rather than moving the window. SF Symbols use aligned visual and hit canvases.

## Layout and interaction

The native NavigationSplitView owns navigation and the detail toolbar. The existing inspector
is a material column inside the detail; do not replace it with a nested split arrangement that
reintroduces constraint loops. One resize can choose a compact layout; it must not continuously
relayout catalog content during animation.

Search uses full-height scrolling content with measured initial clearance inside the content.
Its controls are an overlay without a fixed opaque header band or separator. Suggestions also
float. The player's volume capsule is an overlay without a layout footprint, keeps More
reachable and blocks covered targets. Follow the shared specification for action availability
and narrow layouts; these are not macOS-only preferences. Collection headers fall back to a
stacked cover and metadata arrangement when the inspector leaves too little space. Scrollable
pages measure the player and reserve its clearance inside their content. Track rows retain
selection by identity, support arrow-key navigation and Return playback, and expose selection
and playing state to accessibility. Settings applies the saved System, Light or Dark theme;
the full player keeps its intentional dark appearance.

Use contentTransition with symbolEffect replacement for Play/Pause. Keep the glyph mounted
while a command is pending so busy state does not destroy its transition identity. Control
press/state animation obeys both accessibilityReduceMotion and the app preference. Keyboard
seek and accessible adjustment remain available; the timeline's focus effect is restrained,
clears on Escape or completed pointer seek, and must not stick as a large blue outline.

Synced lyrics retain the Apple Music-like listening treatment: bold active text, softly blurred
and faded context, subtle scale transitions and top/bottom edge fades. Keep the immersive
viewport free of a permanent following switch. Manual exploration reveals sharp context and
an unobtrusive Resume lyrics action. Native lyric buttons remain keyboard-accessible; focused
lines and increased-contrast mode remove the softening. Motion respects the system/app setting.

## Verification

Use the shared acceptance matrix in UI_DESIGN.md. Also inspect native titlebar/toolbar behavior,
light browser versus dark full-player chrome, Liquid Glass and older-material paths, and an open
inspector at the smallest supported window. Do not treat a Swift build as a visual check. When
the user is testing, report the build/test evidence and provide the rebuild instruction without
launching unrelated live sessions for them.
