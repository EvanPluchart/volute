# 0001 — UI toolkit: Slint

**Status:** accepted, 2026-10-24

## Context

Volute is a tray app with three surfaces: a compact flyout, a main window and an on-screen display shown for a second or two at each key press. It must cost nearly nothing at idle and redraw meters smoothly while open. The author builds Vigie with Slint and Recto and Windows Finder with GPUI.

## Decision

Slint, pinned to an exact version (the one Vigie uses, unless a newer release is needed: then a dedicated commit), with the winit backend and the software renderer:

* **Software renderer:** in Vigie it held the whole app at about 10 MB private against far more for a GPU renderer. Meters are small rectangles; a software renderer redraws them cheaply.
* Vigie's lessons apply from the start:
  * Volute draws its own controls and menus, because the native ones ignore the art direction.
  * Every string goes through `@tr()`, with a test for missing French.
  * Models are patched, never replaced.
  * Volute avoids binding loops on layout properties.
* Volute's code is MIT. Slint is used under its royalty-free license, so the About window shows `AboutSlint`.

## Consequences

* The OSD is a Slint window with winit attributes: no decorations, topmost, skip taskbar, no activation, transparent background. If one of them cannot be set through Slint, `volute-shell` adjusts the native window (styles `WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST`), as Vigie does for its mini view.
* The tray icon is `volute-shell`'s own Win32 code, ported from Vigie (`tray_windows.rs`): it ignores foreign `WM_CLOSE` and re-adds itself when Explorer restarts.
