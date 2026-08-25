import type { Point3, Rect2, Size3, Snapshot, StorageNode } from './types';

export const MM_PER_CM = 10;

export function mmToCm(mm: number): number {
  return mm / MM_PER_CM;
}

export function cmToMm(cm: number): number {
  return Math.round(cm * MM_PER_CM);
}

export function childrenOf(snapshot: Snapshot, parentId: string): StorageNode[] {
  return snapshot.nodes.filter((node) => node.parentId === parentId);
}

export function parentOf(snapshot: Snapshot, node: StorageNode): StorageNode | undefined {
  return snapshot.nodes.find((candidate) => candidate.id === node.parentId);
}

export function workspaceFor(snapshot: Snapshot, id: string): StorageNode | undefined {
  let current = snapshot.nodes.find((node) => node.id === id);
  while (current) {
    if (['room', 'rack', 'shelf', 'drawer', 'cell_less_organizer', 'organizer', 'box', 'bin', 'loose', 'custom'].includes(current.kind)) return current;
    current = parentOf(snapshot, current);
  }
  return undefined;
}

export function pathFor(snapshot: Snapshot, node: StorageNode): string {
  const parts = [node.name];
  let parent = parentOf(snapshot, node);
  while (parent) {
    parts.unshift(parent.name);
    parent = parentOf(snapshot, parent);
  }
  return parts.join(' / ');
}

export interface CanvasRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** Projects physical mm into a canvas without changing the underlying units. */
export function topProjection(position: Point3, size: Size3, scale: number): CanvasRect {
  return { x: position.x * scale, y: position.y * scale, width: size.width * scale, height: size.depth * scale };
}

export function elevationProjection(position: Point3, size: Size3, parentHeight: number, scale: number): CanvasRect {
  return {
    x: position.x * scale,
    // Canvas Y points down; physical Z points up.
    y: (parentHeight - position.z - size.height) * scale,
    width: size.width * scale,
    height: size.height * scale,
  };
}

export function clampPosition(position: Point3, size: Size3, parentSize: Size3): Point3 {
  const clampAxis = (value: number, objectSize: number, parentAxisSize: number) => Math.round(
    Math.max(0, Math.min(value, Math.max(0, parentAxisSize - objectSize))),
  );
  return {
    x: clampAxis(position.x, size.width, parentSize.width),
    y: clampAxis(position.y, size.depth, parentSize.depth),
    z: clampAxis(position.z, size.height, parentSize.height),
  };
}

export function rectContains(rect: Rect2, point: { x: number; y: number }): boolean {
  return point.x >= rect.x && point.y >= rect.y && point.x <= rect.x + rect.width && point.y <= rect.y + rect.depth;
}

export function depthForTopNode(node: StorageNode): number {
  return node.placement?.z ?? 0;
}
