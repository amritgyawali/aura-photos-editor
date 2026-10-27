import type { BrushPoint } from '../../ipc/nativeRetouch';

/** Remove only redundant straight samples; corners and pressure changes stay authored.
 * Coordinates are normalized, but geometric decisions use source pixels, not viewport size.
 * A null result means the bounded path is full. Jitter returns the existing path.
 */
export function appendBrushPoint(points: BrushPoint[], point: BrushPoint, width: number, height: number): BrushPoint[] | null {
  const last = points[points.length - 1];
  if (last && Math.hypot((point[0] - last[0]) * width, (point[1] - last[1]) * height) < .75
    && Math.abs(point[2] - last[2]) < .03) return points;
  const previous = points[points.length - 2];
  if (previous && last) {
    const dx = (point[0] - previous[0]) * width, dy = (point[1] - previous[1]) * height;
    const px = (last[0] - previous[0]) * width, py = (last[1] - previous[1]) * height;
    const length = dx * dx + dy * dy;
    const t = length > 0 ? (px * dx + py * dy) / length : -1;
    if (t > 0 && t < 1 && Math.abs(px * dy - py * dx) / Math.sqrt(length) < 1e-6
      && Math.abs(last[2] - (previous[2] + t * (point[2] - previous[2]))) < 1e-5) {
      return [...points.slice(0, -1), point];
    }
  }
  return points.length < 1024 ? [...points, point] : null;
}
