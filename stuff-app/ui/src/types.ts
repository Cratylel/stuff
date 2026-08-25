export type BackendMode = 'live' | 'demo';

export interface Point3 {
  x: number;
  y: number;
  z: number;
}

export interface Size3 {
  width: number;
  depth: number;
  height: number;
}

export interface Rect2 {
  x: number;
  y: number;
  width: number;
  depth: number;
}

export interface Backend {
  mode: BackendMode;
  connectionNote?: string;
}

export interface StorageNode {
  id: string;
  name: string;
  path: string;
  kind: string;
  parentId?: string;
  placement?: Point3;
  size?: Size3;
  dimensionSemantics: 'roomBounds' | 'outer' | 'usable';
  acceptsInventory: boolean;
  spatialStock: boolean;
  allowedChildKinds: string[];
}

export interface OrganizerCell {
  row: number;
  column: number;
  rect: Rect2;
}

export interface OrganizerTemplate {
  id: string;
  name: string;
  outerSize: Size3;
  cells: OrganizerCell[];
  instanceCount: number;
}

export interface OrganizerInstance {
  id: string;
  nodeId: string;
  templateId: string;
  compartments: { key: string; rect: Rect2 }[];
}

export interface RackShelf {
  name: string;
  altitude: number;
  usableHeight: number;
}

export interface RackTemplate {
  id: string;
  name: string;
  outerSize: Size3;
  shelves: RackShelf[];
}

export interface InventoryLocation {
  lotId: string;
  nodeId: string;
  organizerId?: string;
  compartmentKey?: string;
  quantity: number;
  placement?: Point3;
  path: string;
}

export interface InventoryItem {
  id: string;
  name: string;
  manufacturer?: string;
  manufacturerPartNumber?: string;
  category?: string;
  notes?: string;
  tags: string[];
  size?: Size3;
  totalQuantity: number;
  locations: InventoryLocation[];
}

export interface StockLot extends InventoryLocation {
  id: string;
  itemId: string;
  size?: Size3;
}

export interface Snapshot {
  revision: number;
  backend: Backend;
  nodes: StorageNode[];
  organizerTemplates: OrganizerTemplate[];
  organizers: OrganizerInstance[];
  rackTemplates: RackTemplate[];
  items: InventoryItem[];
  stockLots: StockLot[];
}

export interface PlacementPreview {
  position: Point3;
  snappedAxes: [boolean, boolean, boolean];
  snapped: boolean;
}

export type MainTab = 'inventory' | 'layout' | 'types';
export type LayoutWorkspace = 'room' | 'rack' | 'shelf' | 'drawer';
