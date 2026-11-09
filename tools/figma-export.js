// Exports animation frames from the Figma page "Animations – all edges"
// (page 23:2 of https://www.figma.com/design/FRcEU3FARqVPj2IXguv6wz).
//
// Run through the Figma MCP `use_figma` tool with FRAME_IDS set to the
// frames to export and KNOWN to the hashes of parts already saved; it stops
// before the tool's 48 kB result limit and lists what is left in `remaining`.
// tools/import-figma.py turns the results into assets/.
//
// Every visible layer becomes a node in a flat, paint-ordered list with the
// index of its parent. A layer whose subtree has no keyframes is exported
// whole as one SVG ("leaf"). Layers with keyframes, or with animated layers
// inside, stay containers so their transforms can be animated. Coordinates
// are relative to the animation frame, whose borders are the screen edges.
//
// Figma refuses to export layers that are invisible at rest (opacity 0, e.g.
// faces that only appear later) and crops layers to the frame. So the frame
// is cloned, the clone gets full opacity and no clipping, the SVGs come from
// the clone, and the clone is removed again. Keyframes come from the original.
const FRAME_IDS = __FRAME_IDS__;
const KNOWN = new Set(__KNOWN__);
const LIMIT = 28000;
const page = await figma.getNodeByIdAsync('23:2');
await figma.setCurrentPageAsync(page);

const r = (v) => Math.round(v * 100) / 100;
const svgs = {};
const warnings = [];
let size = 0;

// Gradient ids carry the node id; make them per-file so identical parts
// from different frames dedupe, and drop whitespace between tags.
function normalize(svg) {
  const ids = new Map();
  return svg
    .replace(/(paint|filter|clip|mask)\d+_[a-z_]*\d+_\d+/g, (m) => {
      if (!ids.has(m)) ids.set(m, 'p' + ids.size);
      return ids.get(m);
    })
    .replace(/>\s+</g, '><')
    // Round coordinates to 0.01 so float jitter between copies dedupes.
    .replace(/-?\d*\.\d+(e-?\d+)?/g, (m) => String(Math.round(parseFloat(m) * 100) / 100))
    .trim();
}
function hash(s) {
  let h1 = 0x811c9dc5, h2 = 0x01000193;
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    h1 = Math.imul(h1 ^ c, 16777619) >>> 0;
    h2 = Math.imul(h2 + c, 2246822519) >>> 0;
  }
  return h1.toString(16).padStart(8, '0') + h2.toString(16).padStart(8, '0');
}

function tracksOf(n) {
  const t = n.manualKeyframeTracks;
  return t && Object.keys(t).length ? t : null;
}
function animatedInside(n) {
  return 'children' in n && n.children.some((c) => c.visible && (tracksOf(c) || animatedInside(c)));
}
function easing(e) {
  if (e.type === 'HOLD') return 'h';
  const b = e.easingFunctionCubicBezier;
  if (b) return [r(b.x1), r(b.y1), r(b.x2), r(b.y2)];
  if (e.easingFunctionSpring) return { spring: e.easingFunctionSpring.bounce };
  warnings.push('easing without curve: ' + e.type);
  return [0, 0, 1, 1];
}
const FIELD = { TRANSLATION_X: 'tx', TRANSLATION_Y: 'ty', ROTATION: 'rot', SCALE_XY: 's', SCALE_X: 'sx', SCALE_Y: 'sy', OPACITY: 'o' };
function compactTracks(t) {
  const out = {};
  for (const [field, track] of Object.entries(t)) {
    const key = FIELD[field];
    if (!key) { warnings.push('unknown field ' + field); continue; }
    out[key] = track.keyframes.map((k) => [r(k.timelinePosition), r(typeof k.value.value === "object" && k.value.value !== null ? k.value.value.x : k.value.value), easing(k.easing)]);
  }
  return out;
}
function unhide(n) {
  if ('opacity' in n && n.opacity === 0) n.opacity = 1;
  if ('clipsContent' in n) n.clipsContent = false;
  if ('children' in n) for (const c of n.children) unhide(c);
}

const result = [];
const remaining = [...FRAME_IDS];
while (remaining.length) {
  const id = remaining[0];
  const frame = await figma.getNodeByIdAsync(id);
  const clone = frame.clone();
  try {
    clone.x = frame.x;
    clone.y = frame.y + 100000; // out of sight while it exists
    unhide(clone);
    const fx = clone.absoluteTransform[0][2];
    const fy = clone.absoluteTransform[1][2];
    const nodes = [];
    async function visit(n, c, parent) {
      if (!n.visible) return;
      const bb = c.absoluteBoundingBox;
      const node = { n: n.name, p: parent };
      if (bb) { node.cx = r(bb.x - fx + bb.width / 2); node.cy = r(bb.y - fy + bb.height / 2); }
      const t = tracksOf(n);
      if (t) node.t = compactTracks(t);
      if ((t || animatedInside(n)) && 'children' in n) {
        if (n.opacity !== 1 && !(t && t.OPACITY)) node.o = r(n.opacity);
        if ('rotation' in n && Math.abs(n.rotation) > 0.01) warnings.push(`${frame.name} / ${n.name}: rotated container`);
        const index = nodes.length;
        nodes.push(node);
        for (let i = 0; i < n.children.length; i++) await visit(n.children[i], c.children[i], index);
        return;
      }
      if (n.opacity === 0) warnings.push(`${frame.name} / ${n.name}: leaf hidden at rest`);
      if (n.opacity !== 1 && t && t.OPACITY) warnings.push(`${frame.name} / ${n.name}: leaf opacity ${n.opacity} with opacity keyframes`);
      let svg;
      try {
        svg = await c.exportAsync({ format: 'SVG_STRING', useAbsoluteBounds: false });
      } catch (e) {
        warnings.push(`${frame.name} / ${n.name}: ${e.message}`);
        return;
      }
      const rb = c.absoluteRenderBounds;
      svg = normalize(svg);
      const h = hash(svg);
      if (!KNOWN.has(h) && !svgs[h]) { svgs[h] = svg; size += svg.length; }
      node.svg = h;
      node.x = r(rb.x - fx);
      node.y = r(rb.y - fy);
      nodes.push(node);
    }
    for (let i = 0; i < frame.children.length; i++) await visit(frame.children[i], clone.children[i], -1);
    const entry = { id, name: frame.name, w: frame.width, h: frame.height, duration: frame.children[0].timelines[0].duration, nodes };
    size += JSON.stringify(entry).length;
    if (size > LIMIT && result.length) break; // retry this frame in the next call
    result.push(entry);
    remaining.shift();
  } finally {
    clone.remove();
  }
}
return { warnings, remaining, svgs, frames: result };
