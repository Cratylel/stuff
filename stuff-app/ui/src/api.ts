import { invoke } from '@tauri-apps/api/core';
import type { PlacementPreview, Point3, Size3, Snapshot } from './types';

export async function getSnapshot(): Promise<Snapshot> {
  return invoke<Snapshot>('snapshot');
}

export function invokeSnapshot<T extends Record<string, unknown>>(command: string, input: T): Promise<Snapshot> {
  return invoke<Snapshot>(command, { input });
}

export function updateNode(input: {
  nodeId: string;
  name?: string;
  placement?: Point3;
  size?: Size3;
}): Promise<Snapshot> {
  return invoke<Snapshot>('update_node', { input });
}

export function previewPlacement(input: {
  parentId: string;
  nodeId?: string;
  proposedPosition: Point3;
  size: Size3;
}): Promise<PlacementPreview> {
  return invoke<PlacementPreview>('preview_placement', { input });
}

export function updateStockPlacement(input: { lotId: string; placement: Point3 }): Promise<Snapshot> {
  return invoke<Snapshot>('update_stock_placement', { input });
}
