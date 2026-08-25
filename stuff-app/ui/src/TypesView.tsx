import { ActionIcon, Badge, Box, Button, Divider, Group, NumberInput, Paper, ScrollArea, Select, Stack, Tabs, Text, TextInput } from '@mantine/core';
import { useForm } from '@mantine/form';
import Konva from 'konva';
import { Layer, Rect, Stage, Transformer } from 'react-konva';
import { Copy, Factory, Plus, Save, Trash2 } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { invokeSnapshot } from './api';
import { cmToMm, mmToCm } from './projection';
import type { OrganizerCell, OrganizerTemplate, RackShelf, RackTemplate, Size3, Snapshot } from './types';

interface Props { snapshot: Snapshot; activeTab: 'organizers' | 'racks'; onTabChange: (tab: 'organizers' | 'racks') => void; onSnapshot: (snapshot: Snapshot) => void; onOpenLayout: (nodeId: string) => void; }
type OrganizerDraft = { id?: string; name: string; outerSize: Size3; cells: OrganizerCell[] };
type RackDraft = { id?: string; name: string; outerSize: Size3; shelves: RackShelf[] };

export function TypesView({ snapshot, activeTab, onTabChange, onSnapshot, onOpenLayout }: Props) {
  return <div className="types-page"><Tabs value={activeTab} onChange={(value) => value && onTabChange(value as 'organizers' | 'racks')} className="type-tabs"><Tabs.List><Tabs.Tab value="organizers">Organizer Types</Tabs.Tab><Tabs.Tab value="racks">Rack Types</Tabs.Tab></Tabs.List><Tabs.Panel value="organizers"><OrganizerTypes snapshot={snapshot} activeTab={activeTab} onTabChange={onTabChange} onSnapshot={onSnapshot} onOpenLayout={onOpenLayout} /></Tabs.Panel><Tabs.Panel value="racks"><RackTypes snapshot={snapshot} activeTab={activeTab} onTabChange={onTabChange} onSnapshot={onSnapshot} onOpenLayout={onOpenLayout} /></Tabs.Panel></Tabs></div>;
}

function OrganizerTypes({ snapshot, onSnapshot, onOpenLayout }: Props) {
  const [selectedId, setSelectedId] = useState<string>();
  const [draft, setDraft] = useState<OrganizerDraft>();
  const [error, setError] = useState<string>();
  const active = snapshot.organizerTemplates.find((template) => template.id === selectedId);
  useEffect(() => { if (active) setDraft(toOrganizerDraft(active)); }, [active?.id, snapshot.revision]);
  const create = () => { setSelectedId(undefined); setDraft(defaultOrganizerDraft()); };
  const apply = async (command: string, input: Record<string, unknown>) => { try { const next = await invokeSnapshot(command, input); onSnapshot(next); setError(undefined); return next; } catch (reason) { setError(String(reason)); } };
  return <div className="type-library-layout"><TypeList title="Organizer types" createLabel="New organizer type" count={snapshot.organizerTemplates.length} items={snapshot.organizerTemplates.map((template) => ({ id: template.id, name: template.name, detail: `${mmToCm(template.outerSize.width)} × ${mmToCm(template.outerSize.depth)} × ${mmToCm(template.outerSize.height)} cm · ${template.cells.length} cells`, badge: template.instanceCount }))} selectedId={selectedId} onSelect={setSelectedId} onCreate={create} />
    <main className="type-editor"><div className="type-editor-heading"><Box><Text fw={700}>{draft?.id ? 'Edit organizer type' : 'New organizer type'}</Text><Text size="xs" c="dimmed">Set the outside size, then draw the compartments people can put parts into.</Text></Box>{draft?.id && <Badge variant="outline">{active?.instanceCount ?? 0} instances</Badge>}</div>{error && <div className="inline-error">{error}</div>}{draft ? <OrganizerEditor draft={draft} onChange={setDraft} onSave={() => void apply('save_organizer_template', organizerInput(draft))} onClone={() => draft.id && void apply('clone_organizer_template', { sourceTemplateId: draft.id, name: `${draft.name} copy`, outerSize: draft.outerSize })} /> : <EmptyTypeEditor onCreate={create} />}</main>
    <aside className="type-actions"><InstantiateOrganizer snapshot={snapshot} templates={snapshot.organizerTemplates} onSnapshot={onSnapshot} onError={setError} onOpenLayout={onOpenLayout} /><SavePhysicalOrganizer organizers={snapshot.nodes.filter((node) => node.kind === 'organizer')} onSnapshot={onSnapshot} onError={setError} /></aside>
  </div>;
}

function OrganizerEditor({ draft, onChange, onSave, onClone }: { draft: OrganizerDraft; onChange: (draft: OrganizerDraft) => void; onSave: () => void; onClone: () => void }) {
  const [selectedCell, setSelectedCell] = useState<string>();
  const selected = draft.cells.find((cell) => cellKey(cell) === selectedCell);
  const setSize = (key: keyof Size3, value: string | number) => onChange({ ...draft, outerSize: { ...draft.outerSize, [key]: cmToMm(Number(value) || 0) } });
  const setCell = (next: OrganizerCell) => onChange({ ...draft, cells: draft.cells.map((cell) => cellKey(cell) === cellKey(next) ? next : cell) });
  const addCell = () => {
    const row = Math.max(-1, ...draft.cells.map((cell) => cell.row)) + 1;
    const next = { row, column: 0, rect: { x: 0, y: Math.max(0, draft.outerSize.depth - 100), width: Math.max(20, Math.min(100, draft.outerSize.width)), depth: 80 } };
    onChange({ ...draft, cells: [...draft.cells, next] }); setSelectedCell(cellKey(next));
  };
  const updateSelectedNumeric = (key: keyof OrganizerCell['rect'], value: string | number) => selected && setCell({ ...selected, rect: { ...selected.rect, [key]: cmToMm(Number(value) || 0) } });
  return <Stack gap="sm" mt="md"><div className="type-name-grid"><TextInput label="Organizer type name" size="xs" value={draft.name} onChange={(event) => onChange({ ...draft, name: event.currentTarget.value })} /><NumberInput label="Outer width (cm)" size="xs" min={0.5} step={0.5} decimalScale={1} value={mmToCm(draft.outerSize.width)} onChange={(value) => setSize('width', value)} /><NumberInput label="Outer depth (cm)" size="xs" min={0.5} step={0.5} decimalScale={1} value={mmToCm(draft.outerSize.depth)} onChange={(value) => setSize('depth', value)} /><NumberInput label="Outer height (cm)" size="xs" min={0.5} step={0.5} decimalScale={1} value={mmToCm(draft.outerSize.height)} onChange={(value) => setSize('height', value)} /></div>
    <div className="organizer-editor-grid"><Paper withBorder className="organizer-canvas"><OrganizerCanvas draft={draft} selectedCell={selectedCell} onSelect={setSelectedCell} onChangeCell={setCell} /></Paper><Paper withBorder p="sm"><Text size="xs" fw={700} tt="uppercase" c="dimmed">Selected cell (cm)</Text>{selected ? <div className="cell-number-grid"><NumberInput label="Left" size="xs" step={0.5} decimalScale={1} value={mmToCm(selected.rect.x)} onChange={(value) => updateSelectedNumeric('x', value)} /><NumberInput label="From front" size="xs" step={0.5} decimalScale={1} value={mmToCm(selected.rect.y)} onChange={(value) => updateSelectedNumeric('y', value)} /><NumberInput label="Width" size="xs" min={0.5} step={0.5} decimalScale={1} value={mmToCm(selected.rect.width)} onChange={(value) => updateSelectedNumeric('width', value)} /><NumberInput label="Depth" size="xs" min={0.5} step={0.5} decimalScale={1} value={mmToCm(selected.rect.depth)} onChange={(value) => updateSelectedNumeric('depth', value)} /><Button color="red" variant="subtle" size="xs" leftSection={<Trash2 size={13} />} onClick={() => { onChange({ ...draft, cells: draft.cells.filter((cell) => cellKey(cell) !== selectedCell) }); setSelectedCell(undefined); }}>Remove cell</Button></div> : <Text size="xs" c="dimmed" mt="sm">Select a cell, then enter its position and size here.</Text>}<Button size="xs" variant="default" mt="md" leftSection={<Plus size={14} />} onClick={addCell}>Add cell</Button></Paper></div>
    <Group><Button size="xs" leftSection={<Save size={14} />} onClick={onSave}>Save organizer type</Button>{draft.id && <Button size="xs" variant="default" leftSection={<Copy size={14} />} onClick={onClone}>Clone type</Button>}<Text size="xs" c="dimmed">Cells must stay inside the organizer and cannot overlap.</Text></Group>
  </Stack>;
}

function OrganizerCanvas({ draft, selectedCell, onSelect, onChangeCell }: { draft: OrganizerDraft; selectedCell?: string; onSelect: (key: string) => void; onChangeCell: (cell: OrganizerCell) => void }) {
  const width = 520; const height = 340; const scale = Math.min((width - 36) / Math.max(1, draft.outerSize.width), (height - 36) / Math.max(1, draft.outerSize.depth)); const origin = { x: 18, y: 18 };
  return <Stage width={width} height={height}><Layer><Rect x={origin.x} y={origin.y} width={draft.outerSize.width * scale} height={draft.outerSize.depth * scale} fill="#152229" stroke="#6dcbd6" strokeWidth={1.5} />{draft.cells.map((cell) => <EditableCell key={cellKey(cell)} cell={cell} scale={scale} origin={origin} selected={selectedCell === cellKey(cell)} onSelect={() => onSelect(cellKey(cell))} onChange={onChangeCell} />)}</Layer></Stage>;
}

function EditableCell({ cell, scale, origin, selected, onSelect, onChange }: { cell: OrganizerCell; scale: number; origin: { x: number; y: number }; selected: boolean; onSelect: () => void; onChange: (cell: OrganizerCell) => void }) {
  const rectRef = useRef<Konva.Rect>(null); const trRef = useRef<Konva.Transformer>(null);
  useEffect(() => { if (selected && rectRef.current && trRef.current) trRef.current.nodes([rectRef.current]); }, [selected]);
  const nextFrom = (target: Konva.Rect, persistScale = false) => { const sx = target.scaleX(); const sy = target.scaleY(); const next = { ...cell, rect: { x: Math.round((target.x() - origin.x) / scale), y: Math.round((target.y() - origin.y) / scale), width: Math.max(1, Math.round(target.width() * sx / scale)), depth: Math.max(1, Math.round(target.height() * sy / scale)) } }; if (persistScale) { target.scaleX(1); target.scaleY(1); } onChange(next); };
  return <><Rect ref={rectRef} x={origin.x + cell.rect.x * scale} y={origin.y + cell.rect.y * scale} width={cell.rect.width * scale} height={cell.rect.depth * scale} fill="#4d87ae" stroke={selected ? '#f2b96d' : '#d1d4d8'} draggable onClick={(event) => { event.cancelBubble = true; onSelect(); }} onDragMove={(event) => nextFrom(event.target as Konva.Rect)} onDragEnd={(event) => nextFrom(event.target as Konva.Rect)} onTransform={(event) => nextFrom(event.target as Konva.Rect, true)} onTransformEnd={(event) => nextFrom(event.target as Konva.Rect, true)} />{selected && <Transformer ref={trRef} rotateEnabled={false} keepRatio={false} />}</>;
}

function RackTypes({ snapshot, onSnapshot, onOpenLayout }: Props) {
  const [selectedId, setSelectedId] = useState<string>(); const [draft, setDraft] = useState<RackDraft>(); const [error, setError] = useState<string>();
  const active = snapshot.rackTemplates.find((template) => template.id === selectedId);
  useEffect(() => { if (active) setDraft(toRackDraft(active)); }, [active?.id, snapshot.revision]);
  const create = () => { setSelectedId(undefined); setDraft(defaultRackDraft()); };
  const apply = async (command: string, input: Record<string, unknown>) => { try { const next = await invokeSnapshot(command, input); onSnapshot(next); setError(undefined); } catch (reason) { setError(String(reason)); } };
  const physicalRacks = snapshot.nodes.filter((node) => node.kind === 'rack');
  return <div className="type-library-layout"><TypeList title="Rack types" createLabel="New rack type" count={snapshot.rackTemplates.length} items={snapshot.rackTemplates.map((template) => ({ id: template.id, name: template.name, detail: `${mmToCm(template.outerSize.width)} × ${mmToCm(template.outerSize.depth)} × ${mmToCm(template.outerSize.height)} cm · ${template.shelves.length} shelves` }))} selectedId={selectedId} onSelect={setSelectedId} onCreate={create} /><main className="type-editor"><div className="type-editor-heading"><Box><Text fw={700}>{draft?.id ? 'Edit rack type' : 'New rack type'}</Text><Text size="xs" c="dimmed">Set the rack’s outside size, then add each shelf’s base height and clear height.</Text></Box></div>{error && <div className="inline-error">{error}</div>}{draft ? <RackEditor draft={draft} onChange={setDraft} onSave={() => void apply('save_rack_template', rackInput(draft))} onClone={() => draft.id && void apply('clone_rack_template', { sourceTemplateId: draft.id, name: `${draft.name} copy`, outerSize: draft.outerSize })} /> : <EmptyTypeEditor onCreate={create} />}</main><aside className="type-actions"><InstantiateRack snapshot={snapshot} templates={snapshot.rackTemplates} onSnapshot={onSnapshot} onError={setError} onOpenLayout={onOpenLayout} /><SavePhysicalRack physicalRacks={physicalRacks} onSnapshot={onSnapshot} onError={setError} /></aside></div>;
}

function RackEditor({ draft, onChange, onSave, onClone }: { draft: RackDraft; onChange: (draft: RackDraft) => void; onSave: () => void; onClone: () => void }) {
  const setSize = (key: keyof Size3, value: string | number) => onChange({ ...draft, outerSize: { ...draft.outerSize, [key]: cmToMm(Number(value) || 0) } });
  const setShelf = (index: number, patch: Partial<RackShelf>) => onChange({ ...draft, shelves: draft.shelves.map((shelf, i) => i === index ? { ...shelf, ...patch } : shelf) });
  return <Stack mt="md"><div className="type-name-grid"><TextInput label="Rack type name" size="xs" value={draft.name} onChange={(event) => onChange({ ...draft, name: event.currentTarget.value })} /><NumberInput label="Outer width (cm)" size="xs" min={1.5} step={0.5} decimalScale={1} value={mmToCm(draft.outerSize.width)} onChange={(value) => setSize('width', value)} /><NumberInput label="Outer depth (cm)" size="xs" min={1.5} step={0.5} decimalScale={1} value={mmToCm(draft.outerSize.depth)} onChange={(value) => setSize('depth', value)} /><NumberInput label="Outer height (cm)" size="xs" min={0.5} step={0.5} decimalScale={1} value={mmToCm(draft.outerSize.height)} onChange={(value) => setSize('height', value)} /></div><Paper withBorder p="sm"><Group justify="space-between"><Box><Text size="xs" fw={700} tt="uppercase" c="dimmed">Shelf levels</Text><Text size="xs" c="dimmed">Each shelf starts at a base height and has a usable clear height.</Text></Box><Button size="xs" variant="default" leftSection={<Plus size={13} />} onClick={() => onChange({ ...draft, shelves: [...draft.shelves, { name: `Shelf ${draft.shelves.length + 1}`, altitude: 0, usableHeight: 100 }] })}>Add shelf</Button></Group><div className="rack-shelf-headings"><Text size="xs" c="dimmed">Shelf name</Text><Text size="xs" c="dimmed">Base height (cm)</Text><Text size="xs" c="dimmed">Clear height (cm)</Text></div><Stack gap={6} mt={4}>{draft.shelves.map((shelf, index) => <ShelfLevelRow key={index} shelf={shelf} index={index} onChange={(patch) => setShelf(index, patch)} onRemove={() => onChange({ ...draft, shelves: draft.shelves.filter((_, i) => i !== index) })} />)}{!draft.shelves.length && <Text size="xs" c="dimmed">No shelves yet.</Text>}</Stack></Paper><Group><Button size="xs" leftSection={<Save size={14} />} onClick={onSave}>Save rack type</Button>{draft.id && <Button size="xs" variant="default" leftSection={<Copy size={14} />} onClick={onClone}>Clone type</Button>}</Group></Stack>;
}

function ShelfLevelRow({ shelf, index, onChange, onRemove }: { shelf: RackShelf; index: number; onChange: (patch: Partial<RackShelf>) => void; onRemove: () => void }) {
  const [baseHeight, setBaseHeight] = useState(String(mmToCm(shelf.altitude)));
  const [clearHeight, setClearHeight] = useState(String(mmToCm(shelf.usableHeight)));
  useEffect(() => setBaseHeight(String(mmToCm(shelf.altitude))), [shelf.altitude]);
  useEffect(() => setClearHeight(String(mmToCm(shelf.usableHeight))), [shelf.usableHeight]);
  const commit = (value: string, field: 'altitude' | 'usableHeight') => {
    const centimetres = Number(value);
    const valid = Number.isFinite(centimetres) && (field === 'altitude' ? centimetres >= 0 : centimetres > 0);
    if (valid) onChange({ [field]: cmToMm(centimetres) });
    else if (field === 'altitude') setBaseHeight(String(mmToCm(shelf.altitude)));
    else setClearHeight(String(mmToCm(shelf.usableHeight)));
  };
  return <div className="rack-shelf-row"><TextInput size="xs" aria-label={`Shelf ${index + 1} name`} value={shelf.name} onChange={(event) => onChange({ name: event.currentTarget.value })} /><NumberInput size="xs" aria-label={`${shelf.name || `Shelf ${index + 1}`} base height in centimetres`} step={0.5} decimalScale={1} value={baseHeight} onChange={(value) => setBaseHeight(String(value))} onBlur={() => commit(baseHeight, 'altitude')} /><NumberInput size="xs" aria-label={`${shelf.name || `Shelf ${index + 1}`} clear height in centimetres`} step={0.5} decimalScale={1} value={clearHeight} onChange={(value) => setClearHeight(String(value))} onBlur={() => commit(clearHeight, 'usableHeight')} /><ActionIcon aria-label={`Remove ${shelf.name || `shelf ${index + 1}`}`} color="red" variant="subtle" onClick={onRemove}><Trash2 size={14} /></ActionIcon></div>;
}

function TypeList({ title, createLabel, count, items, selectedId, onSelect, onCreate }: { title: string; createLabel: string; count: number; items: { id: string; name: string; detail: string; badge?: number }[]; selectedId?: string; onSelect: (id: string) => void; onCreate: () => void }) {
  return <aside className="type-list"><Group justify="space-between" className="pane-title"><span>{title}</span><Badge size="xs" variant="default">{count}</Badge></Group><Button size="xs" variant="light" m="xs" leftSection={<Plus size={14} />} onClick={onCreate}>{createLabel}</Button><ScrollArea className="type-list-scroll" type="hover"><Stack gap={2} p="xs">{items.map((item) => <Paper key={item.id} className="type-list-item" data-selected={selectedId === item.id || undefined} p="xs" withBorder onClick={() => onSelect(item.id)}><Group justify="space-between" wrap="nowrap"><Box><Text size="sm" fw={600}>{item.name}</Text><Text size="xs" c="dimmed">{item.detail}</Text></Box>{item.badge !== undefined && <Badge size="xs">{item.badge}</Badge>}</Group></Paper>)}{!items.length && <Text size="xs" c="dimmed" p="xs">No types yet. Create one here, then reuse it throughout the layout.</Text>}</Stack></ScrollArea></aside>;
}

function InstantiateOrganizer({ snapshot, templates, onSnapshot, onError, onOpenLayout }: { snapshot: Snapshot; templates: OrganizerTemplate[]; onSnapshot: (snapshot: Snapshot) => void; onError: (message: string) => void; onOpenLayout: (nodeId: string) => void }) {
  const parents = snapshot.nodes.filter((node) => node.allowedChildKinds.includes('organizer'));
  const form = useForm({ initialValues: { organizerName: '', parentId: parents[0]?.id ?? '', templateId: templates[0]?.id ?? '' } });
  useEffect(() => { if (!form.values.parentId && parents[0]) form.setFieldValue('parentId', parents[0].id); if (!form.values.templateId && templates[0]) form.setFieldValue('templateId', templates[0].id); }, [parents[0]?.id, templates[0]?.id]);
  return <Paper withBorder p="sm"><Group gap={6}><Factory size={15} /><Text size="sm" fw={700}>Add organizer from type</Text></Group><Text size="xs" c="dimmed" mt={4}>Choose the reusable layout and where this physical organizer belongs. You can drag it into place afterwards.</Text><Box component="form" onSubmit={form.onSubmit(async (values) => { try { const previousIds = new Set(snapshot.nodes.map((node) => node.id)); const next = await invokeSnapshot('create_organizer_from_template', { ...values, placement: { x: 0, y: 0, z: 0 } }); onSnapshot(next); const organizer = next.nodes.find((node) => node.kind === 'organizer' && !previousIds.has(node.id)); if (organizer) onOpenLayout(organizer.id); } catch (reason) { onError(String(reason)); } })}><TextInput label="Organizer name" size="xs" mt="sm" {...form.getInputProps('organizerName')} /><Select label="Organizer type" size="xs" mt="xs" data={templates.map((template) => ({ value: template.id, label: template.name }))} {...form.getInputProps('templateId')} /><Select label="Put inside" size="xs" mt="xs" data={parents.map((node) => ({ value: node.id, label: node.path }))} {...form.getInputProps('parentId')} /><Button type="submit" size="xs" mt="sm">Add organizer</Button></Box></Paper>;
}

function SavePhysicalOrganizer({ organizers, onSnapshot, onError }: { organizers: Snapshot['nodes']; onSnapshot: (snapshot: Snapshot) => void; onError: (message: string) => void }) {
  const form = useForm({ initialValues: { organizerNodeId: organizers[0]?.id ?? '', templateName: '' } });
  useEffect(() => { if (!form.values.organizerNodeId && organizers[0]) form.setFieldValue('organizerNodeId', organizers[0].id); }, [organizers[0]?.id]);
  return <Paper withBorder p="sm" mt="sm"><Group gap={6}><Copy size={15} /><Text size="sm" fw={700}>Make an organizer type from this organizer</Text></Group><Text size="xs" c="dimmed" mt={4}>Captures its current layout as a new reusable type, then switches this organizer to it.</Text><Box component="form" onSubmit={form.onSubmit(async (values) => { try { onSnapshot(await invokeSnapshot('save_organizer_as_template', values)); } catch (reason) { onError(String(reason)); } })}><Select label="Existing organizer" size="xs" mt="sm" data={organizers.map((organizer) => ({ value: organizer.id, label: organizer.path }))} {...form.getInputProps('organizerNodeId')} /><TextInput label="New organizer type name" size="xs" mt="xs" {...form.getInputProps('templateName')} /><Button type="submit" size="xs" mt="sm">Create organizer type</Button></Box></Paper>;
}

function InstantiateRack({ snapshot, templates, onSnapshot, onError, onOpenLayout }: { snapshot: Snapshot; templates: RackTemplate[]; onSnapshot: (snapshot: Snapshot) => void; onError: (message: string) => void; onOpenLayout: (nodeId: string) => void }) {
  const rooms = snapshot.nodes.filter((node) => node.kind === 'room');
  const form = useForm({ initialValues: { rackName: '', roomId: rooms[0]?.id ?? '', templateId: templates[0]?.id ?? '' } });
  useEffect(() => { if (!form.values.roomId && rooms[0]) form.setFieldValue('roomId', rooms[0].id); if (!form.values.templateId && templates[0]) form.setFieldValue('templateId', templates[0].id); }, [rooms[0]?.id, templates[0]?.id]);
  return <Paper withBorder p="sm"><Group gap={6}><Factory size={15} /><Text size="sm" fw={700}>Add rack from type</Text></Group><Text size="xs" c="dimmed" mt={4}>Create a rack and its shelf levels in one step. Drag the rack into place in the Layout tab.</Text><Box component="form" onSubmit={form.onSubmit(async (values) => { try { const previousIds = new Set(snapshot.nodes.map((node) => node.id)); const next = await invokeSnapshot('instantiate_rack_template', { ...values, placement: { x: 0, y: 0, z: 0 } }); onSnapshot(next); const rack = next.nodes.find((node) => node.kind === 'rack' && !previousIds.has(node.id)); if (rack) onOpenLayout(rack.id); } catch (reason) { onError(String(reason)); } })}><TextInput label="Rack name" size="xs" mt="sm" {...form.getInputProps('rackName')} /><Select label="Rack type" size="xs" mt="xs" data={templates.map((template) => ({ value: template.id, label: template.name }))} {...form.getInputProps('templateId')} /><Select label="Put in room" size="xs" mt="xs" data={rooms.map((room) => ({ value: room.id, label: room.path }))} {...form.getInputProps('roomId')} /><Button type="submit" size="xs" mt="sm">Add rack</Button></Box></Paper>;
}

function SavePhysicalRack({ physicalRacks, onSnapshot, onError }: { physicalRacks: Snapshot['nodes']; onSnapshot: (snapshot: Snapshot) => void; onError: (message: string) => void }) {
  const form = useForm({ initialValues: { rackId: physicalRacks[0]?.id ?? '', templateName: '' } });
  useEffect(() => { if (!form.values.rackId && physicalRacks[0]) form.setFieldValue('rackId', physicalRacks[0].id); }, [physicalRacks[0]?.id]);
  return <Paper withBorder p="sm" mt="sm"><Group gap={6}><Copy size={15} /><Text size="sm" fw={700}>Make a rack type from this rack</Text></Group><Text size="xs" c="dimmed" mt={4}>Copies the rack’s size and shelf levels into a reusable type.</Text><Box component="form" onSubmit={form.onSubmit(async (values) => { try { onSnapshot(await invokeSnapshot('save_rack_as_template', values)); } catch (reason) { onError(String(reason)); } })}><Select label="Existing rack" size="xs" mt="sm" data={physicalRacks.map((rack) => ({ value: rack.id, label: rack.path }))} {...form.getInputProps('rackId')} /><TextInput label="New rack type name" size="xs" mt="xs" {...form.getInputProps('templateName')} /><Button type="submit" size="xs" mt="sm">Create rack type</Button></Box></Paper>;
}

function EmptyTypeEditor({ onCreate }: { onCreate: () => void }) { return <Box className="empty-type-editor"><Text c="dimmed">Choose a reusable type from the library or start a new one.</Text><Button size="xs" mt="sm" onClick={onCreate}>New type</Button></Box>; }
function defaultOrganizerDraft(): OrganizerDraft { return { name: 'New organizer', outerSize: { width: 300, depth: 200, height: 50 }, cells: [{ row: 0, column: 0, rect: { x: 0, y: 0, width: 300, depth: 200 } }] }; }
function toOrganizerDraft(template: OrganizerTemplate): OrganizerDraft { return { id: template.id, name: template.name, outerSize: { ...template.outerSize }, cells: template.cells.map((cell) => ({ ...cell, rect: { ...cell.rect } })) }; }
function defaultRackDraft(): RackDraft { return { name: 'New rack', outerSize: { width: 900, depth: 450, height: 1800 }, shelves: [{ name: 'Shelf 1', altitude: 0, usableHeight: 300 }] }; }
function toRackDraft(template: RackTemplate): RackDraft { return { id: template.id, name: template.name, outerSize: { ...template.outerSize }, shelves: template.shelves.map((shelf) => ({ ...shelf })) }; }
function organizerInput(draft: OrganizerDraft) { return { templateId: draft.id, name: draft.name, outerSize: draft.outerSize, cells: draft.cells }; }
function rackInput(draft: RackDraft) { return { templateId: draft.id, name: draft.name, outerSize: draft.outerSize, shelves: draft.shelves }; }
function cellKey(cell: OrganizerCell) { return `${cell.row}:${cell.column}`; }
