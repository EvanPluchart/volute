# 0008 — The microphone's level

**Status:** accepted, 2026-10-25

## Context

The flyout and the Devices view show the default microphone with a level meter. On the author's machine the meter stayed empty during M1: Windows measures an input's peak only while some program captures from it. Reading the level at all times would mean Volute opening a capture stream of its own, and Windows 11 then shows its "microphone in use" indicator in the taskbar, naming Volute.

Two ways were open:

* **(a)** open a capture stream while the flyout or the main window is shown, so the meter always moves, and the indicator shows for as long;
* **(b)** open nothing: the meter moves when an app (a call, a recorder) already uses the microphone, and stays empty otherwise.

## Decision

**(b).** Volute never opens a capture stream.

* A mixer that lights the microphone indicator whenever it is looked at teaches the user to ignore that indicator, the one sign that something is listening.
* The meter still answers the question that matters during a call: "is my microphone picking me up?", because the call app captures.
* The Devices view says it under the inputs: "A microphone shows its level while an app is using it."

## Consequences

* An empty meter does not mean a broken microphone. The sentence in the Devices view says so.
* The README's "What Volute reads" states that Volute never records or captures sound.
* If a "test my microphone" button is wanted one day, it opens a capture stream only while it is held, and a new decision says so.
