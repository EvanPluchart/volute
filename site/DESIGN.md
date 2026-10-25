# Volute landing page — DESIGN.md

The design system of `site/`. Keep the page and this file in step; the brand is in `docs/BRAND.md`.

## 1. Idea

**The app icon, opened up.** The page is set on the icon's own grounds: violet for Volute speaking (the hero, the download), paper for reading, ink for the figures. One live thing moves on it: **the amber level**, the meter that in the app shows what each app plays.
* Under the headline, a level bar breathes like a meter (still for readers who ask for reduced motion).
* "Try it": three faders that behave like Volute's, with the level inside the fill, scaled by the volume, and a click on the icon to mute.

Light only, on purpose; the screenshots carry the dark theme.

## 2. Colour

| Token | Value | Role |
|---|---|---|
| `--violet` | `#5B2BFF` | Hero and download grounds; never text on paper |
| `--violet-deep` | `#3F17D6` | Links and labels on paper (8.6:1) |
| `--cream` | `#FFF1DE` | Text on violet (6.3:1) and ink, the primary button |
| `--amber` | `#FFB020` | The level, the figures on ink, the focus ring; never text on paper |
| `--ink` | `#1B1530` | Text, the cost field, the footer |
| `--paper` | `#FBF8F3` | Page ground |
| `--surface` | `#F3EDE4` | Alternate sections |
| `--muted` | `#5C546A` | Secondary text (7:1 on paper) |

## 3. Type

Figtree, 400 to 800, self-hosted as a latin variable subset (20 KB). Display `clamp(44px, 7.2vw, 104px)` at 800 with -0.04em tracking; section titles `clamp(30px, 4.2vw, 52px)` at 800; body 17px at 1.55.

## 4. Sections

| Section | Ground | Content |
|---|---|---|
| Hero | Violet | Headline, level, one-paragraph pitch, download, the flyout |
| Try it | Paper | Three working faders on the app's dark surface |
| Features | Surface | Three cards, the main window |
| Cost | Ink | Four measured figures in amber, and how they were measured |
| What it reads | Paper | Six plain statements |
| Next | Surface | The roadmap, in order |
| Questions | Paper | `details` elements |
| Download | Violet | The button again |
| Footer | Ink | Licence, Slint, the other language |

## 5. Rules

* Real figures only, with their source; no adjective where a number fits.
* Screenshots are the real app on its demo backend, framed with a 12 px radius and one soft shadow.
* No emoji, no gradients, no icons from a set: the few glyphs are drawn in the app's own line style.
* Every interactive element works with the keyboard and shows the amber focus ring.
