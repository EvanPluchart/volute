# Design audit and redesign — 2026-10-25

Board: [redesign/board.png](redesign/board.png), M1 on the left, the redesign on the right, both on the demo backend.

## What was wrong (judged against docs/BRAND.md)

| # | Finding | Principle it broke | Severity |
|---|---|---|---|
| 1 | The master sat on a slab of saturated violet, in both themes: the loudest thing on screen was chrome, not sound. | Calm at rest; one accent | High |
| 2 | Every fader was filled in violet at rest, so nothing told a playing app from a silent one. | Colour comes from sound | High |
| 3 | The live level was a thin amber line hidden inside violet: hard to see. | Sound you can see | Medium |
| 4 | Apps without icons showed a letter on a violet tile; the demo looked like placeholders. | Instruments, not forms | Medium |
| 5 | Names were 12 px above the faders, figures large and bold beside them: the hierarchy was upside down. | — | Medium |
| 6 | The main window had pill tabs, tiny text, and About opened a second window. No Settings at all. | — | High |
| 7 | Buttons, switches and the output picker could not be reached with the keyboard. | AA, keyboard | Medium |
| 8 | A flyout that grew while open left a blank strip at the bottom (in M1 already). | — | High |

## What changed

* **Tokens** (`ui/theme.slint`): neutral warm surfaces in four steps (background, surface, hover, raised), three text levels each AA or better, a neutral fader fill, amber only for levels, violet only for Volute speaking (selection, focus, an update), red only for what cuts sound. Segoe UI Variable, Windows 11's own face.
* **The fader:** a neutral track and fill; the level glows amber *inside* the fill, scaled by the volume, so it never runs past the thumb and silence leaves the fill calm. The thumb is ringed in the surface colour; hover and focus strengthen the fill; focus adds a violet ring.
* **The flyout:** the master on a quiet surface with a large figure and a weightier fader, the output's name as a selector with a check on the default; rows with the name and figure on one line, the fader under them, a hover; the microphone and "Open Volute ↗" in the footer; the update line on top when there is one.
* **The main window:** a rail with the logo, Mixer, Devices, Settings and About; headings; cards for the master, devices and settings; an update reminder in the rail.
* **Settings:** theme (segmented), start with Windows, keep running when the window closes (decision 0009), updates (decision 0005) with the version and one button.
* **About:** the icon, the version, what Volute reads in one sentence, links, the Slint credit.
* **The demo** draws its own app icons, so screenshots look like a real mixer without anyone's apps.
* **Keyboard:** every control takes focus with Tab and works with Space or Enter; Escape closes the flyout from anywhere in it.

## Checked

* Contrast: text and text-muted are 7:1 or better on their surfaces in both themes; text-faint (figures, hints) 4.5:1 or better.
* Budget: cheaper than before (main window 3.0 % on the mock instead of 3.9 %; see `docs/audits/2026-10-25-performance.md`).
* English and French, every string through `@tr()` (the test passes).

## Still open

* Long device names are cut with an ellipsis; Slint 1.18 has no tooltip, and the accessible name carries the full name.
* The OSD (M2) will use the same fader and tokens.
