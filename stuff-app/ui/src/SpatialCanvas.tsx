import { Badge, Box, Divider, Group, Slider, Stack, Text } from '@mantine/core';
import Konva from 'konva';
import { Layer, Line, Rect, Stage, Text as KonvaText, Transformer } from 'react-konva';
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { previewPlacement, updateNode, updateStockPlacement } from './api';
import { elevationDragPosition, topDragPosition } from './drag';
import { childrenOf, elevationProjection, mmToCm, topProjection } from './projection';
import type { Point3, Size3, Snapshot, StockLot, StorageNode } from './types';

interface Props {
  snapshot: Snapshot;
  workspace: StorageNode;
  selectedId?: string;
  onSelect: (id: string) => void;
  onOpenWorkspace: (id: string) => void;
  onNodeContextMenu: (id: string, x: number, y: number) => void;
  onSnapshot: (snapshot: Snapshot) => void;
  onError: (message: string) => void;
}

interface Geometry { position: Point3; size: Size3 }

function useCanvasSize() {
  const ref = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ width: 640, height: 480 });
  useLayoutEffect(() => {
    const element = ref.current;
    if (!element) return;
    const resize = () => setSize({ width: Math.max(300, element.clientWidth), height: Math.max(300, element.clientHeight) });
    resize();
    const observer = new ResizeObserver(resize);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  return { ref, ...size };
}

function useViewport() {
  const [viewport, setViewport] = useState({ x: 0, y: 0, zoom: 1 });
  const onWheel = (event: Konva.KonvaEventObject<WheelEvent>) => {
    event.evt.preventDefault();
    const stage = event.target.getStage();
    const pointer = stage?.getPointerPosition();
    if (!stage || !pointer) return;
    setViewport((current) => {
      const nextZoom = Math.max(0.35, Math.min(3.5, current.zoom * (event.evt.deltaY > 0 ? 0.9 : 1.1)));
      const localX = (pointer.x - current.x) / current.zoom;
      const localY = (pointer.y - current.y) / current.zoom;
      return { zoom: nextZoom, x: pointer.x - localX * nextZoom, y: pointer.y - localY * nextZoom };
    });
  };
  const onDragMove = (event: Konva.KonvaEventObject<DragEvent>) => {
    if (event.target !== event.target.getStage()) return;
    setViewport((current) => ({ ...current, x: event.target.x(), y: event.target.y() }));
  };
  return { viewport, stageProps: { x: viewport.x, y: viewport.y, scaleX: viewport.zoom, scaleY: viewport.zoom, draggable: true, onWheel, onDragMove } };
}

function geometryFor(nodes: StorageNode[]): Record<string, Geometry> {
  return Object.fromEntries(nodes.filter((node) => node.placement && node.size).map((node) => [node.id, { position: node.placement!, size: node.size! }]));
}

function fitScale(parent: Size3, width: number, height: number, top: boolean): number {
  const surfaceHeight = top ? parent.depth : parent.height;
  return Math.max(0.015, Math.min((width - 64) / parent.width, (height - 64) / surfaceHeight, 0.5));
}

export function SpatialCanvas(props: Props) {
  if (!props.workspace.size) return <CanvasEmpty message="This workspace has no dimensions yet." />;
  if (props.workspace.kind === 'room') return <RoomCanvas {...props} />;
  if (props.workspace.kind === 'rack') return <RackCanvas {...props} />;
  if (props.workspace.kind === 'shelf') return <ShelfCanvas {...props} />;
  if (props.workspace.kind === 'drawer') return <DrawerCanvas {...props} />;
  if (props.workspace.kind === 'organizer') return <OrganizerWorkspace {...props} />;
  return <StorageContents {...props} />;
}

function RoomCanvas(props: Props) {
  return <TopDownCanvas {...props} title="Room · top-down" subtitle="Racks, room shelves, and fixtures share the room plane." />;
}

function DrawerCanvas(props: Props) {
  return <TopDownCanvas {...props} title="Drawer · top view" subtitle="Usable drawer boundary; storage is placed in the drawer plane." />;
}

function OrganizerWorkspace({ snapshot, workspace, onSelect }: Props) {
  const { ref, width, height } = useCanvasSize();
  const viewport = useViewport();
  const organizer = snapshot.organizers.find((instance) => instance.nodeId === workspace.id);
  if (!organizer || !workspace.size || !organizer.compartments.length) return <StorageContents snapshot={snapshot} workspace={workspace} onSelect={onSelect} onOpenWorkspace={() => {}} onNodeContextMenu={() => {}} onSnapshot={() => {}} onError={() => {}} />;
  const scale = fitScale(workspace.size, width, height, true);
  const origin = { x: 30, y: 38 };
  const itemById = new Map(snapshot.items.map((item) => [item.id, item]));
  return <div className="canvas-workspace"><CanvasHeader title="Organizer · top view" subtitle="Each compartment shows its current stock. Click a cell to select the organizer." scale={scale} /><div ref={ref} className="konva-host"><Stage width={width} height={height} {...viewport.stageProps}><Layer><Rect x={origin.x} y={origin.y} width={workspace.size.width * scale} height={workspace.size.depth * scale} fill="#24272c" stroke="#508eb8" strokeWidth={1.5} />{organizer.compartments.map((cell) => { const lots = snapshot.stockLots.filter((lot) => lot.organizerId === organizer.id && lot.compartmentKey === cell.key); const label = lots.length ? lots.map((lot) => `${itemById.get(lot.itemId)?.name ?? 'Item'} · ${lot.quantity}`).join('\n') : 'Free'; return <><Rect key={cell.key} x={origin.x + cell.rect.x * scale} y={origin.y + cell.rect.y * scale} width={cell.rect.width * scale} height={cell.rect.depth * scale} fill={lots.length ? '#4d87ae' : '#34383e'} stroke="#d1d4d8" strokeWidth={1} onClick={() => onSelect(workspace.id)} /><KonvaText key={`${cell.key}-text`} x={origin.x + cell.rect.x * scale + 5} y={origin.y + cell.rect.y * scale + 5} width={Math.max(0, cell.rect.width * scale - 10)} text={label} fill="#f2f3f5" fontSize={11} listening={false} /></>; })}</Layer></Stage></div></div>;
}

function StorageContents({ snapshot, workspace }: Props) {
  const children = childrenOf(snapshot, workspace.id);
  const lots = snapshot.stockLots.filter((lot) => lot.nodeId === workspace.id);
  const itemById = new Map(snapshot.items.map((item) => [item.id, item]));
  return <div className="canvas-workspace"><CanvasHeader title={`${workspace.name} · contents`} subtitle="This storage has no spatial compartment layout." scale={1} /><Stack gap={7} p="md"><Text size="xs" fw={700} tt="uppercase" c="dimmed">Contained storage</Text>{children.length ? children.map((child) => <Text key={child.id} size="sm">{child.name} <Text component="span" size="xs" c="dimmed">· {child.kind}</Text></Text>) : <Text size="sm" c="dimmed">No contained storage.</Text>}<Divider /><Text size="xs" fw={700} tt="uppercase" c="dimmed">Stock</Text>{lots.length ? lots.map((lot) => <Text key={lot.id} size="sm">{itemById.get(lot.itemId)?.name ?? 'Unknown item'} · {lot.quantity}</Text>) : <Text size="sm" c="dimmed">No direct stock.</Text>}</Stack></div>;
}

function TopDownCanvas({ snapshot, workspace, selectedId, onSelect, onOpenWorkspace, onNodeContextMenu, onSnapshot, onError, title, subtitle }: Props & { title: string; subtitle: string }) {
  const { ref, width, height } = useCanvasSize();
  const children = useMemo(() => childrenOf(snapshot, workspace.id).filter((node) => node.placement && node.size), [snapshot, workspace.id]);
  const [geometry, setGeometry] = useState<Record<string, Geometry>>(() => geometryFor(children));
  useEffect(() => setGeometry(geometryFor(children)), [snapshot.revision, workspace.id]);
  const parentSize = workspace.size!;
  const lots = spatialLots(snapshot, workspace.id);
  const itemNames = new Map(snapshot.items.map((item) => [item.id, item.name]));
  const scale = fitScale(parentSize, width, height, true);
  const origin = { x: 30, y: 32 };
  const canvasViewport = useViewport();
  const commit = async (node: StorageNode, next: Geometry, snap = true) => {
    try {
      const position = snap
        ? (await previewPlacement({ parentId: workspace.id, nodeId: node.id, proposedPosition: next.position, size: next.size })).position
        : next.position;
      setGeometry((current) => ({ ...current, [node.id]: { ...next, position } }));
      onSnapshot(await updateNode({ nodeId: node.id, placement: position, size: next.size }));
    } catch (reason) { onError(String(reason)); }
  };
  const commitStock = async (lot: StockLot, position: Point3) => {
    try { onSnapshot(await updateStockPlacement({ lotId: lot.id, placement: position })); } catch (reason) { onError(String(reason)); }
  };
  return (
    <div className="canvas-workspace">
      <CanvasHeader title={title} subtitle={subtitle} scale={scale * canvasViewport.viewport.zoom} />
      <div ref={ref} className="konva-host">
        <Stage width={width} height={height} {...canvasViewport.stageProps} onMouseDown={(event) => { if (event.target === event.target.getStage()) onSelect(''); }}>
          <Layer>
            <Grid width={width} height={height} origin={origin} scale={scale} />
            <Rect x={origin.x} y={origin.y} width={parentSize.width * scale} height={parentSize.depth * scale} fill="#24272c" stroke="#9fa7af" strokeWidth={1.5} cornerRadius={2} />
            {children.map((node) => {
              const current = geometry[node.id];
              return current && <TopNode
                key={node.id} node={node} geometry={current} scale={scale} origin={origin}
                selected={selectedId === node.id} onSelect={onSelect} onOpen={onOpenWorkspace} onContextMenu={onNodeContextMenu}
                onChange={(next) => setGeometry((all) => ({ ...all, [node.id]: next }))}
                onCommit={(next, snap) => void commit(node, next, snap)} parentSize={parentSize}
              />;
            })}
            {lots.map((lot) => <TopStock key={lot.id} lot={lot} label={`${itemNames.get(lot.itemId) ?? 'Item'} · ${lot.quantity}`} scale={scale} origin={origin} parentSize={parentSize} onCommit={(position) => void commitStock(lot, position)} />)}
            <KonvaText x={origin.x + 8} y={origin.y + 8} text={`${workspace.name}  ${mmToCm(parentSize.width)} × ${mmToCm(parentSize.depth)} cm`} fill="#b5bac0" fontSize={11} />
          </Layer>
        </Stage>
      </div>
    </div>
  );
}

function RackCanvas({ snapshot, workspace, selectedId, onSelect, onOpenWorkspace, onNodeContextMenu, onSnapshot, onError }: Props) {
  const { ref, width, height } = useCanvasSize();
  const children = useMemo(() => childrenOf(snapshot, workspace.id).filter((node) => node.kind === 'shelf' && node.placement && node.size), [snapshot, workspace.id]);
  const [geometry, setGeometry] = useState<Record<string, Geometry>>(() => geometryFor(children));
  useEffect(() => setGeometry(geometryFor(children)), [snapshot.revision, workspace.id]);
  const parentSize = workspace.size!;
  const lots = spatialLots(snapshot, workspace.id);
  const itemNames = new Map(snapshot.items.map((item) => [item.id, item.name]));
  const scale = fitScale(parentSize, width, height, false);
  const origin = { x: 30, y: 32 };
  const canvasViewport = useViewport();
  const commit = async (node: StorageNode, next: Geometry) => {
    try {
      const position = (await previewPlacement({ parentId: workspace.id, nodeId: node.id, proposedPosition: next.position, size: next.size })).position;
      setGeometry((current) => ({ ...current, [node.id]: { ...next, position } }));
      onSnapshot(await updateNode({ nodeId: node.id, placement: position, size: next.size }));
    } catch (reason) { onError(String(reason)); }
  };
  const commitStock = async (lot: StockLot, position: Point3) => {
    try { onSnapshot(await updateStockPlacement({ lotId: lot.id, placement: position })); } catch (reason) { onError(String(reason)); }
  };
  return (
    <div className="canvas-workspace">
      <CanvasHeader title="Rack · front view" subtitle="Shelf bands and direct stock share the rack workspace." scale={scale * canvasViewport.viewport.zoom} />
      <div ref={ref} className="konva-host">
        <Stage width={width} height={height} {...canvasViewport.stageProps}>
          <Layer>
            <Grid width={width} height={height} origin={origin} scale={scale} />
            <Rect x={origin.x} y={origin.y} width={parentSize.width * scale} height={parentSize.height * scale} fill="#24272c" stroke="#cf8b47" strokeWidth={1.5} />
            {children.map((node) => {
              const current = geometry[node.id];
              return current && <ElevationNode key={node.id} node={node} geometry={current} scale={scale} origin={origin} selected={selectedId === node.id}
                onSelect={onSelect} onOpen={onOpenWorkspace} onContextMenu={onNodeContextMenu} onChange={(next) => setGeometry((all) => ({ ...all, [node.id]: next }))}
                onCommit={(next) => void commit(node, next)} parentSize={parentSize} />;
            })}
            {lots.map((lot) => <ElevationStock key={lot.id} lot={lot} label={`${itemNames.get(lot.itemId) ?? 'Item'} · ${lot.quantity}`} scale={scale} origin={origin} parentSize={parentSize} onCommit={(position) => void commitStock(lot, position)} />)}
            <KonvaText x={origin.x + 8} y={origin.y + 8} text={`${workspace.name} · outer height ${mmToCm(parentSize.height)} cm`} fill="#b5bac0" fontSize={11} />
          </Layer>
        </Stage>
      </div>
    </div>
  );
}

function ShelfCanvas({ snapshot, workspace, selectedId, onSelect, onOpenWorkspace, onNodeContextMenu, onSnapshot, onError }: Props) {
  const [z, setZ] = useState<number>();
  const { ref, width, height } = useCanvasSize();
  const children = useMemo(() => childrenOf(snapshot, workspace.id).filter((node) => node.placement && node.size), [snapshot, workspace.id]);
  const [geometry, setGeometry] = useState<Record<string, Geometry>>(() => geometryFor(children));
  useEffect(() => setGeometry(geometryFor(children)), [snapshot.revision, workspace.id]);
  const parentSize = workspace.size!;
  const lots = spatialLots(snapshot, workspace.id);
  const itemNames = new Map(snapshot.items.map((item) => [item.id, item.name]));
  const scale = fitScale(parentSize, width, Math.floor(height * 0.62), true);
  const origin = { x: 30, y: 32 };
  const topViewport = useViewport();
  const frontViewport = useViewport();
  const commit = async (node: StorageNode, next: Geometry) => {
    try {
      const position = (await previewPlacement({ parentId: workspace.id, nodeId: node.id, proposedPosition: next.position, size: next.size })).position;
      setGeometry((current) => ({ ...current, [node.id]: { ...next, position } }));
      onSnapshot(await updateNode({ nodeId: node.id, placement: position, size: next.size }));
    } catch (reason) { onError(String(reason)); }
  };
  const commitStock = async (lot: StockLot, position: Point3) => {
    try { onSnapshot(await updateStockPlacement({ lotId: lot.id, placement: position })); } catch (reason) { onError(String(reason)); }
  };
  const maxZ = Math.max(1, parentSize.height);
  const activeZ = z ?? maxZ;
  return (
    <div className="canvas-workspace shelf-workspace">
      <CanvasHeader title="Shelf workspace" subtitle="Top view positions objects on the shelf; front view sets their height." scale={scale * topViewport.viewport.zoom} />
      <div ref={ref} className="shelf-canvas-grid">
        <div className="shelf-top-view">
          <Text className="view-label" size="xs" c="dimmed" tt="uppercase">Top view · width and depth</Text><div className="z-control"><Text size="xs" c="dimmed">View height</Text><Slider size="xs" value={activeZ} onChange={setZ} min={0} max={maxZ} step={5} label={(value) => `${mmToCm(value)} cm`} /><Badge size="xs" variant="outline">{mmToCm(activeZ)} cm</Badge></div>
          <Stage width={width} height={Math.floor(height * 0.62)} {...topViewport.stageProps}>
            <Layer>
              <Grid width={width} height={Math.floor(height * 0.62)} origin={origin} scale={scale} />
              <Rect x={origin.x} y={origin.y} width={parentSize.width * scale} height={parentSize.depth * scale} fill="#24272c" stroke="#a08ac7" strokeWidth={1.5} dash={[5, 3]} />
              {children.map((node) => {
                const current = geometry[node.id];
                if (!current) return null;
                return <TopNode key={node.id} node={node} geometry={current} scale={scale} origin={origin} selected={selectedId === node.id} onSelect={onSelect} onOpen={onOpenWorkspace} onContextMenu={onNodeContextMenu}
                  muted={current.position.z > activeZ} onChange={(next) => setGeometry((all) => ({ ...all, [node.id]: next }))} onCommit={(next) => void commit(node, next)} parentSize={parentSize} />;
              })}
              {lots.map((lot) => <TopStock key={lot.id} lot={lot} label={`${itemNames.get(lot.itemId) ?? 'Item'} · ${lot.quantity}`} scale={scale} origin={origin} parentSize={parentSize} muted={(lot.placement?.z ?? 0) > activeZ} onCommit={(position) => void commitStock(lot, position)} />)}
              {children.map((node) => { const current = geometry[node.id]; if (!current) return null; const rect = topProjection(current.position, current.size, scale); return <KonvaText key={`${node.id}-overlay-label`} x={origin.x + rect.x + 5} y={origin.y + rect.y + 5} width={Math.max(0, rect.width - 10)} text={node.name} fontSize={Math.max(9, Math.min(13, rect.width / 8))} fill={current.position.z > activeZ ? '#7e838b' : '#f2f3f5'} listening={false} />; })}
            </Layer>
          </Stage>
        </div>
        <div className="shelf-side-view">
          <Text size="xs" c="dimmed" tt="uppercase">Front view · width and height</Text>
          <Stage width={width} height={Math.max(160, Math.floor(height * 0.38))} {...frontViewport.stageProps}>
            <Layer>
              <Rect x={origin.x} y={22} width={parentSize.width * scale} height={parentSize.height * scale} fill="#202328" stroke="#737a84" />
              <Line points={[origin.x, 22 + (parentSize.height - activeZ) * scale, origin.x + parentSize.width * scale, 22 + (parentSize.height - activeZ) * scale]} stroke="#e1a661" dash={[4, 4]} />
              {children.map((node) => {
                const current = geometry[node.id];
                if (!current) return null;
                return <Rect key={node.id} x={origin.x + current.position.x * scale} y={22 + (parentSize.height - current.position.z - current.size.height) * scale} width={current.size.width * scale} height={current.size.height * scale} fill={colorFor(node.kind, .72)} stroke={selectedId === node.id ? '#f2b96d' : '#d1d4d8'} draggable
                  onClick={() => onSelect(node.id)}
                  onContextMenu={(event) => { event.evt.preventDefault(); event.cancelBubble = true; onSelect(node.id); onNodeContextMenu(node.id, event.evt.clientX, event.evt.clientY); }}
                  onDragMove={(event) => setGeometry((all) => ({ ...all, [node.id]: { ...current, position: elevationDragPosition({ x: event.target.x() - origin.x, y: event.target.y() - 22 }, scale, current.position, current.size, parentSize) } }))}
                  onDragEnd={(event) => void commit(node, { ...current, position: elevationDragPosition({ x: event.target.x() - origin.x, y: event.target.y() - 22 }, scale, current.position, current.size, parentSize) })} />;
              })}
              {lots.map((lot) => <ElevationStock key={lot.id} lot={lot} label={`${itemNames.get(lot.itemId) ?? 'Item'} · ${lot.quantity}`} scale={scale} origin={{ x: origin.x, y: 22 }} parentSize={parentSize} onCommit={(position) => void commitStock(lot, position)} />)}
            </Layer>
          </Stage>
        </div>
      </div>
    </div>
  );
}

function spatialLots(snapshot: Snapshot, nodeId: string): StockLot[] {
  return snapshot.stockLots.filter((lot) => lot.nodeId === nodeId && lot.placement && lot.size);
}

function TopStock({ lot, label, scale, origin, parentSize, onCommit, muted = false }: {
  lot: StockLot; label: string; scale: number; origin: { x: number; y: number }; parentSize: Size3; onCommit: (position: Point3) => void; muted?: boolean;
}) {
  const position = lot.placement!;
  const size = lot.size!;
  const rect = topProjection(position, size, scale);
  return <>
    <Rect x={origin.x + rect.x} y={origin.y + rect.y} width={rect.width} height={rect.height} fill={muted ? '#4b826844' : '#4b8268'} stroke="#b7e1c8" strokeWidth={1.5} cornerRadius={2} draggable
      onDragEnd={(event) => onCommit(topDragPosition({ x: event.target.x() - origin.x, y: event.target.y() - origin.y }, scale, position, size, parentSize))} />
    <KonvaText x={origin.x + rect.x + 4} y={origin.y + rect.y + 4} width={Math.max(0, rect.width - 8)} text={label} fill={muted ? '#8fa69a' : '#f1fff6'} fontSize={10} listening={false} />
  </>;
}

function ElevationStock({ lot, label, scale, origin, parentSize, onCommit }: {
  lot: StockLot; label: string; scale: number; origin: { x: number; y: number }; parentSize: Size3; onCommit: (position: Point3) => void;
}) {
  const position = lot.placement!;
  const size = lot.size!;
  const rect = elevationProjection(position, size, parentSize.height, scale);
  return <>
    <Rect x={origin.x + rect.x} y={origin.y + rect.y} width={rect.width} height={rect.height} fill="#4b8268" stroke="#b7e1c8" strokeWidth={1.5} cornerRadius={2} draggable
      onDragEnd={(event) => onCommit(elevationDragPosition({ x: event.target.x() - origin.x, y: event.target.y() - origin.y }, scale, position, size, parentSize))} />
    <KonvaText x={origin.x + rect.x + 4} y={origin.y + rect.y + 3} width={Math.max(0, rect.width - 8)} text={label} fill="#f1fff6" fontSize={10} listening={false} />
  </>;
}

function TopNode({ node, geometry, scale, origin, selected, onSelect, onOpen, onContextMenu, onChange, onCommit, parentSize, muted = false }: {
  node: StorageNode; geometry: Geometry; scale: number; origin: { x: number; y: number }; selected: boolean; onSelect: (id: string) => void; onOpen: (id: string) => void; onContextMenu: (id: string, x: number, y: number) => void; onChange: (geometry: Geometry) => void; onCommit: (geometry: Geometry, snap?: boolean) => void; parentSize: Size3; muted?: boolean;
}) {
  const rectRef = useRef<Konva.Rect>(null);
  const transformerRef = useRef<Konva.Transformer>(null);
  useEffect(() => { if (selected && rectRef.current && transformerRef.current) transformerRef.current.nodes([rectRef.current]); }, [selected]);
  const rect = topProjection(geometry.position, geometry.size, scale);
  const updateFromNode = (target: Konva.Rect, commit = false) => {
    const sx = target.scaleX(); const sy = target.scaleY();
    const next: Geometry = {
      position: topDragPosition({ x: target.x() - origin.x, y: target.y() - origin.y }, scale, geometry.position, geometry.size, parentSize),
      size: { ...geometry.size, width: Math.max(10, Math.round(target.width() * sx / scale)), depth: Math.max(10, Math.round(target.height() * sy / scale)) },
    };
    target.scaleX(1); target.scaleY(1);
    onChange(next);
    if (commit) onCommit(next, false);
  };
  return <>
    <Rect ref={rectRef} x={origin.x + rect.x} y={origin.y + rect.y} width={rect.width} height={rect.height} fill={colorFor(node.kind, muted ? .24 : 1)} stroke={selected ? '#f2b96d' : '#d1d4d8'} strokeWidth={selected ? 2 : 1} cornerRadius={2} draggable
      onClick={(event) => { event.cancelBubble = true; onSelect(node.id); }} onContextMenu={(event) => { event.evt.preventDefault(); event.cancelBubble = true; onSelect(node.id); onContextMenu(node.id, event.evt.clientX, event.evt.clientY); }} onDblClick={() => ['room', 'rack', 'shelf', 'drawer', 'cell_less_organizer', 'organizer', 'box', 'bin', 'loose', 'custom'].includes(node.kind) && onOpen(node.id)}
      onDragMove={(event) => onChange({ ...geometry, position: topDragPosition({ x: event.target.x() - origin.x, y: event.target.y() - origin.y }, scale, geometry.position, geometry.size, parentSize) })}
      onDragEnd={(event) => onCommit({ ...geometry, position: topDragPosition({ x: event.target.x() - origin.x, y: event.target.y() - origin.y }, scale, geometry.position, geometry.size, parentSize) })}
      onTransform={(event) => updateFromNode(event.target as Konva.Rect)} onTransformEnd={(event) => updateFromNode(event.target as Konva.Rect, true)} />
    <KonvaText x={origin.x + rect.x + 5} y={origin.y + rect.y + 5} text={node.name} fontSize={Math.max(9, Math.min(13, rect.width / 8))} width={Math.max(0, rect.width - 10)} fill={muted ? '#7e838b' : '#f2f3f5'} listening={false} />
    {selected && <Transformer ref={transformerRef} rotateEnabled={false} keepRatio={false} boundBoxFunc={(_, box) => (box.width < 12 || box.height < 12 ? _ : box)} />}
  </>;
}

function ElevationNode({ node, geometry, scale, origin, selected, onSelect, onOpen, onContextMenu, onChange, onCommit, parentSize }: {
  node: StorageNode; geometry: Geometry; scale: number; origin: { x: number; y: number }; selected: boolean; onSelect: (id: string) => void; onOpen: (id: string) => void; onContextMenu: (id: string, x: number, y: number) => void; onChange: (geometry: Geometry) => void; onCommit: (geometry: Geometry) => void; parentSize: Size3;
}) {
  const rect = elevationProjection(geometry.position, geometry.size, parentSize.height, scale);
  return <>
    <Rect x={origin.x + rect.x} y={origin.y + rect.y} width={rect.width} height={rect.height} fill="#6a5b84" stroke={selected ? '#f2b96d' : '#d3d5d9'} strokeWidth={selected ? 2 : 1} draggable
      onClick={(event) => { event.cancelBubble = true; onSelect(node.id); }} onContextMenu={(event) => { event.evt.preventDefault(); event.cancelBubble = true; onSelect(node.id); onContextMenu(node.id, event.evt.clientX, event.evt.clientY); }} onDblClick={() => onOpen(node.id)}
      onDragMove={(event) => onChange({ ...geometry, position: elevationDragPosition({ x: event.target.x() - origin.x, y: event.target.y() - origin.y }, scale, geometry.position, geometry.size, parentSize) })}
      onDragEnd={(event) => onCommit({ ...geometry, position: elevationDragPosition({ x: event.target.x() - origin.x, y: event.target.y() - origin.y }, scale, geometry.position, geometry.size, parentSize) })} />
    <KonvaText x={origin.x + rect.x + 5} y={origin.y + rect.y + 4} text={`${node.name} · base ${mmToCm(geometry.position.z)} cm`} fill="#f2f3f5" fontSize={10} listening={false} />
  </>;
}

function CanvasHeader({ title, subtitle, scale }: { title: string; subtitle: string; scale: number }) {
  return <Group justify="space-between" className="workspace-caption"><Box><Text size="sm" fw={700}>{title}</Text><Text size="xs" c="dimmed">{subtitle}</Text></Box><Badge size="xs" variant="outline">Scale: 1 px = {mmToCm(1 / scale).toFixed(2)} cm</Badge></Group>;
}

function Grid({ width, height, origin, scale }: { width: number; height: number; origin: { x: number; y: number }; scale: number }) {
  const spacing = Math.max(20, Math.round(100 * scale));
  const lines = [] as React.ReactNode[];
  for (let x = origin.x; x < width; x += spacing) lines.push(<Line key={`x${x}`} points={[x, 0, x, height]} stroke="#2c3036" strokeWidth={1} />);
  for (let y = origin.y; y < height; y += spacing) lines.push(<Line key={`y${y}`} points={[0, y, width, y]} stroke="#2c3036" strokeWidth={1} />);
  return <>{lines}</>;
}

function CanvasEmpty({ message }: { message: string }) {
  return <Box className="empty-canvas"><Text c="dimmed">{message}</Text></Box>;
}

function colorFor(kind: string, alpha = 1): string {
  const colors: Record<string, string> = { rack: '#bd7a3d', shelf: '#8071a4', fixture: '#72675d', cell_less_organizer: '#5e91a8', organizer: '#508eb8', drawer: '#705e98', box: '#65717d', bin: '#77856d', loose: '#686b70' };
  const color = colors[kind] ?? '#4f6570';
  return alpha === 1 ? color : `${color}${Math.round(alpha * 255).toString(16).padStart(2, '0')}`;
}
