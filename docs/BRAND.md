# Brand and art direction

A volute is the spiral at the head of a violin, and the curl of a sound unwinding. Volute says two things: **sound is shaped here, by hand**, and **it is a pleasure to touch**. Warm, precise, tactile. A good mixing desk, not a settings page.

## Principles

1. **Instruments, not forms.** Sliders feel like faders: a weighty thumb, a clear track, the level glowing inside the track. Every control answers the hand at once.
2. **Sound you can see.** Meters move smoothly (attack fast, release slow, a short peak hold) and quiet down to nothing when there is no sound. No meter dances when nothing plays.
3. **Calm at rest.** Idle, the flyout is quiet: names, icons, volumes. Colour comes from sound: a meter, an active output.
4. **One accent.** A single warm accent for what is live (levels, the active device, the primary action). Danger red only for what cuts sound (mute, an error).
5. **Fast motion, never slow.** About 120 ms in, 180 ms out, eased. The OSD slides and fades; nothing bounces.

## Surfaces

* **The OSD** replaces Windows' own volume flyout. Windows draws a flat grey pill with a speaker glyph, a thin bar and a number, in a fixed corner.

  Volute's OSD is:
  * a rounded card on a soft translucent surface;
  * the device's name, with its icon;
  * a large volume figure with a fat fader, and the live level running through it;
  * the mute state as a crossed speaker that animates;
  * the app's icon and name instead when an app is targeted.

  It is legible at a glance on any wallpaper, in light and dark, and stays clear of the game's HUD (position in Settings).
* **The flyout** (from the tray):
  * the master fader at the top, with the output device and a switcher;
  * then one row per app;
  * profiles and mic in a footer.

  It is about 360 px wide, opens in under 100 ms, and closes when focus leaves.
* **The main window:** Mixer, Devices, Profiles, Settings, in a rail as in Vigie.

## Tokens (to set in the design track)

`ui/theme.slint` holds every value: surfaces (base, raised, overlay, translucent OSD), text (primary, muted), accent and its hover, danger, meter gradient (calm, loud, clipping), border, radii (8 controls, 14 cards, 20 OSD), spacing scale (4, 8, 12, 16, 24), type scale (caption 12, body 14, title 18, OSD figure 40), and motion durations.

* **Light and dark from the start**, following the system, with a setting to choose.
* **Contrast:** AA for every text against its surface, in both themes.

## Logo (chosen 2026-10-24)

![Volute](../assets/brand/png/volute-horizontal.png)

**The V of Volute carries a scroll head, as a violin does.** The head is amber, a volute cut into it; muted, it turns red and the volute becomes a bar. The kit, its colours, sizes and rules are in [assets/brand/README.md](../assets/brand/README.md).

* **Palette:** violet `#5B2BFF` (the tile, the brand's ground), cream `#FFF1DE` (the V on violet), amber `#FFB020` (the head: still the one accent), ink `#1B1530` (text), danger `#FF4D5E` (muted only).
* **Style:** heavy, flat, rounded, two or three colours on a coloured tile. The interface's tokens follow this palette in the design track.
* How it was reached: three concepts (a knob volute, the V with a scroll, a volute unwinding into a fader), then the V redrawn bold with a round head and four palettes. All of them are in `assets/brand/concepts/`.

## Logo brief (for the `logo-design` skill)

* **Idea to explore:** a volute (a violin scroll's spiral) that is also a volume sign: a spiral unwinding like sound waves, or a knob whose indicator traces a volute.
* **Style:** bold rounded-square app icon, flat, two or three colours, geometric. It must read at 16 px in the tray and the taskbar, as Vigie's does.
* **Variants:**
  * a tray icon that can show the mute state (a slash, or a quieter form);
  * one-colour versions for the light and dark taskbar.
* **Process:** the skill's process: concepts first, then the full kit for the chosen one.
