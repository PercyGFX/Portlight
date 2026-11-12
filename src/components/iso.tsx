// Isometric line-art building blocks for the live map.
//
// Everything is made of boxes. A point in the scene has three coordinates:
// `i` runs down-right, `j` runs down-left and `k` runs straight up. Only the
// top, the front-left and the front-right face of a box are visible.

const COS = 0.866;
const SIN = 0.5;

type Point3 = [i: number, j: number, k: number];

function project(ox: number, oy: number, points: Point3[]): string {
  return points
    .map(([i, j, k]) => `${(ox + (i - j) * COS).toFixed(1)},${(oy + (i + j) * SIN - k).toFixed(1)}`)
    .join(" ");
}

interface BoxProps {
  /** Screen position of the scene's origin. */
  ox: number;
  oy: number;
  /** Where the box starts in the scene. */
  at?: Point3;
  /** Size along i, j and k. */
  size: Point3;
}

function Box({ ox, oy, at = [0, 0, 0], size }: BoxProps) {
  const [i, j, k] = at;
  const [a, b, h] = size;
  return (
    <>
      <polygon className="iso-left" points={project(ox, oy, [[i, j + b, k], [i + a, j + b, k], [i + a, j + b, k + h], [i, j + b, k + h]])} />
      <polygon className="iso-right" points={project(ox, oy, [[i + a, j, k], [i + a, j + b, k], [i + a, j + b, k + h], [i + a, j, k + h]])} />
      <polygon className="iso-top" points={project(ox, oy, [[i, j, k + h], [i + a, j, k + h], [i + a, j + b, k + h], [i, j + b, k + h]])} />
    </>
  );
}

/** Screen origin that puts the middle of an `a` by `b` footprint at (cx, cy). */
function originFor(cx: number, cy: number, a: number, b: number): [number, number] {
  return [cx - ((a - b) / 2) * COS, cy - ((a + b) / 2) * SIN];
}

/** A flat tile: one app. With an icon, the icon stands on the tile. */
export function AppTile({ cx, cy, icon = null }: { cx: number; cy: number; icon?: string | null }) {
  const [ox, oy] = originFor(cx, cy + 5, 34, 34);
  return (
    <g className="iso">
      <Box ox={ox} oy={oy} size={[34, 34, 9]} />
      <polygon className="iso-detail" points={project(ox, oy, [[10, 10, 9], [24, 10, 9], [24, 24, 9], [10, 24, 9]])} />
      {icon && (
        <>
          <ellipse className="iso-icon-shadow" cx={cx} cy={cy - 3} rx="10" ry="4" />
          <image className="iso-icon" href={icon} x={cx - 12} y={cy - 27} width="24" height="24" />
        </>
      )}
    </g>
  );
}

/** An open laptop: this PC. */
export function Laptop({ cx, cy }: { cx: number; cy: number }) {
  const [ox, oy] = originFor(cx, cy + 14, 66, 46);
  return (
    <g className="iso">
      <Box ox={ox} oy={oy} size={[66, 46, 5]} />
      <polygon className="iso-detail" points={project(ox, oy, [[8, 13, 5], [58, 13, 5], [58, 33, 5], [8, 33, 5]])} />
      <Box ox={ox} oy={oy} at={[0, 0, 5]} size={[66, 3, 44]} />
      <polygon className="iso-screen" points={project(ox, oy, [[5, 3, 10], [61, 3, 10], [61, 3, 44], [5, 3, 44]])} />
      <polyline className="iso-glint" points={project(ox, oy, [[12, 3, 36], [34, 3, 36]])} />
      <polyline className="iso-glint" points={project(ox, oy, [[12, 3, 29], [26, 3, 29]])} />
    </g>
  );
}

/** A small rack of three units: a server on the internet. */
export function Server({ cx, cy }: { cx: number; cy: number }) {
  const [ox, oy] = originFor(cx, cy + 18, 38, 38);
  return (
    <g className="iso">
      {[0, 13, 26].map((k) => (
        <g key={k}>
          <Box ox={ox} oy={oy} at={[0, 0, k]} size={[38, 38, 10]} />
          <polyline className="iso-glint" points={project(ox, oy, [[6, 38, k + 5], [17, 38, k + 5]])} />
          <polygon className="iso-led" points={project(ox, oy, [[28, 38, k + 3.5], [32, 38, k + 3.5], [32, 38, k + 6.5], [28, 38, k + 6.5]])} />
        </g>
      ))}
    </g>
  );
}

/** A flat cloud with a globe: the internet. */
export function Cloud({ cx, cy }: { cx: number; cy: number }) {
  return (
    <g className="cloud">
      <path
        className="cloud-shape"
        d={`M${cx - 36} ${cy + 20} a16 16 0 0 1 3 -31.5 a22 22 0 0 1 40 -9 a17 17 0 0 1 27 15.5 a13 13 0 0 1 -3 25 Z`}
      />
      <circle className="cloud-globe" cx={cx} cy={cy + 2} r="10" />
      <ellipse className="cloud-globe" cx={cx} cy={cy + 2} rx="4.4" ry="10" />
      <path className="cloud-globe" d={`M${cx - 10} ${cy + 2}h20`} />
    </g>
  );
}
