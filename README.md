# Landing page

The static page for GitHub Pages, in French (`index.html`) and English (`en/index.html`), sharing `assets/`: the stylesheet, the script, the typeface, the screenshots and the social images. English screenshots are in `assets/en/`. No build step, no framework, no third-party request: Figtree is self-hosted (SIL OFL, licence in `assets/fonts/`).

* **Design:** `DESIGN.md`; the brand is in `../docs/BRAND.md`.
* **Screenshots:** the real app on its demo backend, never the author's own apps:
  * flyouts at 2x: `SLINT_SCALE_FACTOR=2 volute --mock --no-settings --dark --flyout=center --lang=fr|en` (add `--offer-update=0.2.0` for the update line);
  * the main window at 1.25x: `SLINT_SCALE_FACTOR=1.25 volute --mock --no-settings --no-tray --light --open --lang=fr|en`, cropped of its 8 px invisible border;
  * captured with PrintWindow (`docs/ux` has the scripts' description).
* **Social images:** `assets/og.png` and `assets/og-en.png`, from `assets/og-source.html#fr|#en` with headless Chrome at 1200 × 630.
* **Publish** (decision 0011): Pages serves the `gh-pages` branch, the contents of `site/` with a `.nojekyll`, pushed by hand; there is no workflow. The page expects `https://evanpluchart.github.io/volute/`; every asset path is relative.
* **Downloads** point to `releases/latest`, which works for visitors only once the repository is public and a release is published.
