import type { Point3, Size3 } from './types';
import { clampPosition } from './projection';

export interface Grip {
  /** Pointer position inside the node, in physical millimetres. */
  x: number;
  y: number;
  z: number;
}

export function gripFromPointer(pointerMm: Point3, nodePosition: Point3): Grip {
  return { x: pointerMm.x - nodePosition.x, y: pointerMm.y - nodePosition.y, z: pointerMm.z - nodePosition.z };
}

/**
 * Converts an absolute Konva top-down node position to a physical position.
 * This intentionally never accumulates deltas, avoiding drift/jumps.
 */
export function topDragPosition(
  absoluteCanvas: { x: number; y: number },
  scale: number,
  current: Point3,
  size: Size3,
  parentSize: Size3,
): Point3 {
  return clampPosition(
    { x: absoluteCanvas.x / scale, y: absoluteCanvas.y / scale, z: current.z },
    size,
    parentSize,
  );
}

/** Elevation is X/Z; the canvas Y coordinate maps directly to Z. */
export function elevationDragPosition(
  absoluteCanvas: { x: number; y: number },
  scale: number,
  current: Point3,
  size: Size3,
  parentSize: Size3,
): Point3 {
  return clampPosition(
    {
      x: absoluteCanvas.x / scale,
      y: current.y,
      // Canvas Y runs down, physical Z runs up. Absolute coordinates keep the
      // pointer grip intact and avoid incremental drag drift.
      z: parentSize.height - absoluteCanvas.y / scale - size.height,
    },
    size,
    parentSize,
  );
}

/** Shelf side view is depth/Z, so canvas X maps to physical Y. */
export function sideDragPosition(
  absoluteCanvas: { x: number; y: number },
  scale: number,
  current: Point3,
  size: Size3,
  parentSize: Size3,
): Point3 {
  return clampPosition(
    {
      x: current.x,
      y: absoluteCanvas.x / scale,
      z: parentSize.height - absoluteCanvas.y / scale - size.height,
    },
    size,
    parentSize,
  );
}
