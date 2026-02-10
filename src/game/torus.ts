import { WORLD_WIDTH, WORLD_HEIGHT } from "./constants";
import type { Vec2 } from "./types";

/**
 * Shortest delta between two points in the toroidal (wrap-around) world.
 * Operates in float pixel space — suitable for the autopilot and rendering layer.
 */
export function shortestDelta(fromX: number, fromY: number, toX: number, toY: number): Vec2 {
  let dx = toX - fromX;
  let dy = toY - fromY;

  if (dx > WORLD_WIDTH / 2) dx -= WORLD_WIDTH;
  if (dx < -WORLD_WIDTH / 2) dx += WORLD_WIDTH;
  if (dy > WORLD_HEIGHT / 2) dy -= WORLD_HEIGHT;
  if (dy < -WORLD_HEIGHT / 2) dy += WORLD_HEIGHT;

  return { x: dx, y: dy };
}
