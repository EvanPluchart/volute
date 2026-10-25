// The "try it" desk: faders that behave like Volute's. The level is the sound before the
// volume, drawn inside the fill and scaled by it, so it never runs past the thumb. A click on
// an icon mutes. Meters move only while the desk is on screen, and not at all for readers who
// ask for reduced motion.
(function () {
  "use strict";
  var strips = Array.prototype.slice.call(document.querySelectorAll(".strip"));
  if (!strips.length) return;

  var still = window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  strips.forEach(function (strip, i) {
    var input = strip.querySelector("input");
    var out = strip.querySelector("output");
    var fill = strip.querySelector(".fill");
    var level = strip.querySelector(".level");
    var thumb = strip.querySelector(".thumb");
    var mute = strip.querySelector(".mute");
    var label = strip.querySelector(".name span").textContent;
    strip.peak = 0;
    strip.phase = i * 1.7;
    strip.speed = 0.9 + i * 0.35;

    function draw() {
      var v = input.value / 100;
      fill.style.width = v * 100 + "%";
      thumb.style.left = "calc(8px + (100% - 16px) * " + v + ")";
      level.style.width = v * strip.peak * 100 + "%";
      out.textContent = input.value;
    }
    strip.draw = draw;
    input.addEventListener("input", draw);
    mute.addEventListener("click", function () {
      var muted = strip.classList.toggle("muted");
      mute.setAttribute("aria-pressed", muted ? "true" : "false");
      mute.setAttribute("aria-label", (muted ? mute.dataset.unmute : mute.dataset.mute) + " " + label);
    });
    draw();
  });

  if (still) {
    strips.forEach(function (s) { s.peak = 0.6; s.draw(); });
    return;
  }

  var running = false;
  var last = 0;
  function frame(t) {
    if (!running) return;
    // About 30 frames a second, as Volute's own meters.
    if (t - last > 33) {
      last = t;
      strips.forEach(function (s) {
        var x = t / 1000 * s.speed + s.phase;
        var target = 0.45 + 0.3 * Math.sin(x) + 0.25 * Math.abs(Math.sin(x * 3.1));
        // Fast attack, slow release.
        s.peak = target > s.peak ? target : s.peak * 0.86 + target * 0.14;
        s.draw();
      });
    }
    requestAnimationFrame(frame);
  }
  var desk = document.querySelector(".desk");
  if ("IntersectionObserver" in window) {
    new IntersectionObserver(function (entries) {
      var visible = entries[0].isIntersecting;
      if (visible && !running) { running = true; requestAnimationFrame(frame); }
      if (!visible) running = false;
    }).observe(desk);
  } else {
    running = true;
    requestAnimationFrame(frame);
  }
})();
