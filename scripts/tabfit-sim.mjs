/* global console, process */
// Self-verification for the tab-bar overflow fit logic.
// Models the layout (tab min width, group labels, chevron, new-tab button) and
// simulates the render/effect loop to check for infinite oscillation (the cause
// of the React #185 crash on tab close/add).

const MIN = 90;
const LABEL = 54; // a group alias chip
const CHEVRON = 40;
const NEWBTN = 40;

// Model: content min-width for `shown` tabs, `labels` group labels among them,
// and whether the chevron is present (shown < count).
function contentMin(shown, labels, hasChevron) {
  return shown * MIN + labels * LABEL + (hasChevron ? CHEVRON : 0) + NEWBTN;
}

// --- OLD logic: shrink on overflow, grow on width-slack (the buggy one) ---
function simulateOld({ barWidth, count, labelsOf }) {
  let capacity = 999;
  const seen = new Map(); // capacity -> pass index, to detect cycles
  for (let pass = 0; pass < 500; pass++) {
    const shown = Math.max(1, Math.min(capacity, count));
    if (seen.has(shown)) {
      return { result: "LOOP", cycleAt: pass, shown };
    }
    seen.set(shown, pass);

    const hasChevron = shown < count;
    const labels = labelsOf(shown, count);
    const min = contentMin(shown, labels, hasChevron);
    const over = min - barWidth;

    if (over > 1) {
      if (shown > 1)
        capacity = Math.max(1, shown - Math.max(1, Math.ceil(over / MIN)));
      else return { result: "STABLE", shown, pass };
    } else if (shown < count) {
      // tabs flex-grow to fill: tabsWidth = barWidth - overhead
      const overhead = labels * LABEL + (hasChevron ? CHEVRON : 0) + NEWBTN;
      const tabsWidth = barWidth - overhead;
      const slack = tabsWidth - shown * MIN;
      const GROW_UNIT = MIN + 50;
      if (slack >= GROW_UNIT) capacity = shown + Math.floor(slack / GROW_UNIT);
      else return { result: "STABLE", shown, pass };
    } else {
      return { result: "STABLE", shown, pass };
    }
  }
  return { result: "NO-CONVERGE(500)" };
}

// --- NEW logic: capacity is a pure function of width only ---
function fitCapacity(barWidth) {
  const RESERVE = 120;
  return Math.max(1, Math.floor((barWidth - RESERVE) / MIN));
}
function simulateNew({ barWidth, count }) {
  // The effect sets capacity = fitCapacity(width). Width does not change when
  // capacity/count/content change, so a "render pass" recomputes the SAME
  // value and React stops. Model that: it's a fixed point in one step.
  const capacity = fitCapacity(barWidth);
  const shown = Math.max(1, Math.min(capacity, count));
  // Second pass would compute the same capacity (width unchanged) => stable.
  const capacity2 = fitCapacity(barWidth);
  const stable = capacity === capacity2;
  return { result: stable ? "STABLE" : "LOOP", shown, capacity };
}

// --- CANDIDATE: width ceiling + shrink-only correction, reset on count/width ---
// capacity resets to the width-based ceiling on a count/width change, then only
// SHRINKS while the real (label-aware) content overflows. Grow never happens,
// so it must be monotonic within a fixed (count,width). We check it terminates.
function simulateShrinkReset({ barWidth, count, labelsOf }) {
  const ceil = fitCapacity(barWidth);
  let capacity = ceil;
  let prevKey = null;
  const seen = new Set();
  for (let pass = 0; pass < 500; pass++) {
    const shown = Math.max(1, Math.min(capacity, count));
    const key = `${capacity}`;
    // reset once per (count,width) — modeled by prevKey against the ceiling
    const ctx = `${count}:${barWidth}`;
    if (prevKey !== ctx) {
      prevKey = ctx;
      if (capacity !== ceil) {
        capacity = ceil;
        continue;
      }
    }
    if (seen.has(key)) return { result: "LOOP", shown };
    seen.add(key);

    const hasChevron = shown < count;
    const labels = labelsOf(shown, count);
    const over = contentMin(shown, labels, hasChevron) - barWidth;
    if (over > 1 && shown > 1) {
      const next = Math.max(1, shown - Math.max(1, Math.ceil(over / MIN)));
      if (next === capacity) return { result: "STABLE", shown, pass };
      capacity = next;
    } else {
      return { result: "STABLE", shown, pass };
    }
  }
  return { result: "NO-CONVERGE(500)" };
}

let failures = 0;
const log = (ok, msg) => {
  if (!ok) failures++;
  console.log(`${ok ? "PASS" : "FAIL"}  ${msg}`);
};

// 1) fitCapacity is monotonic non-decreasing in width, and >= 1.
{
  let mono = true;
  let prev = -1;
  for (let w = 0; w <= 4000; w += 7) {
    const c = fitCapacity(w);
    if (c < 1) mono = false;
    if (c < prev) mono = false;
    prev = c;
  }
  log(mono, "fitCapacity: monotonic non-decreasing and >= 1 across widths");
}

// 2) Reproduce the OLD loop: a group label makes a grow overshoot into an
//    overflow that shrink undoes -> cycle. Pick a width where slack sits in the
//    [GROW_UNIT, MIN+LABEL) gap and the next tab adds a label.
{
  // 5 tabs where showing the 6th introduces a 2nd group label.
  const count = 12;
  const labelsOf = (shown) => (shown >= 6 ? 2 : 1);
  // Tune width so that at shown=5, slack is ~142 (in the [140,144) danger gap).
  // overhead(5) = 1*54 + CHEVRON + NEWBTN = 134; tabsWidth = W-134;
  // slack = W-134-450 = W-584. Want slack≈142 => W≈726.
  const barWidth = 726;
  const old = simulateOld({ barWidth, count, labelsOf });
  log(
    old.result === "LOOP",
    `OLD logic loops on the label gap (got ${old.result} at shown=${old.shown})`,
  );
}

// 3) The NEW logic is stable for the same and many random scenarios.
{
  let allStable = true;
  const cases = [];
  for (let w = 200; w <= 3000; w += 13) {
    for (const count of [1, 2, 5, 12, 40, 200]) {
      const r = simulateNew({ barWidth: w, count });
      if (r.result !== "STABLE") {
        allStable = false;
        cases.push({ w, count, r });
      }
      // shown never exceeds count and is >= 1
      if (r.shown < 1 || r.shown > count) {
        allStable = false;
        cases.push({ w, count, r, bad: "shown out of range" });
      }
    }
  }
  log(
    allStable,
    `NEW logic stable & shown in [1,count] across ${(((3000 - 200) / 13) * 6) | 0} cases`,
  );
  if (!allStable) console.log(cases.slice(0, 5));
}

// 4) NEW logic also never crashes on the exact OLD-loop scenario.
{
  const r = simulateNew({ barWidth: 726, count: 12 });
  log(
    r.result === "STABLE",
    `NEW logic stable on the old-loop scenario (shown=${r.shown}, cap=${r.capacity})`,
  );
}

// 5) shrink-only correction converges (no loop) across many scenarios,
//    including heavy grouping (labels up to 4).
{
  let allStable = true;
  let worstClip = 0;
  const bad = [];
  for (let w = 200; w <= 3000; w += 11) {
    for (const count of [1, 3, 8, 20, 60]) {
      for (const g of [0, 1, 2, 4]) {
        const labelsOf = (shown) => Math.min(g, Math.max(0, shown - 1));
        const r = simulateShrinkReset({ barWidth: w, count, labelsOf });
        if (r.result !== "STABLE") {
          allStable = false;
          bad.push({ w, count, g, r });
        } else {
          // verify the settled layout does NOT overflow (no clip)
          const shown = r.shown;
          const labels = labelsOf(shown, count);
          const over = contentMin(shown, labels, shown < count) - w;
          if (over > 1) worstClip = Math.max(worstClip, over);
        }
      }
    }
  }
  log(allStable, `shrink-only correction converges across grouped scenarios`);
  log(
    worstClip <= 1,
    `shrink-only settled layout never overflows (worst clip ${worstClip}px)`,
  );
  if (bad.length) console.log(bad.slice(0, 5));
}

console.log(
  failures === 0 ? "\nALL CHECKS PASSED" : `\n${failures} CHECK(S) FAILED`,
);
process.exit(failures === 0 ? 0 : 1);
