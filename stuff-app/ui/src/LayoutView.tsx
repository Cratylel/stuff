import {
  ActionIcon, Badge, Box, Button, Divider, Group, Modal, NumberInput, Paper, ScrollArea, Select,
  Stack, Text, TextInput, Tooltip,
} from '@mantine/core';
import { useForm } from '@mantine/form';
import { invoke } from '@tauri-apps/api/core';
import {
  Archive, Armchair, Box as BoxIcon, Boxes, ChevronDown, ChevronRight, Copy, FolderTree,
  ArrowLeft, ArrowRight, CornerLeftUp, Layers3, PackageOpen, PanelTop, Plus, Trash2, Warehouse,
} from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { invokeSnapshot } from './api';
import { SpatialCanvas } from './SpatialCanvas';
import { childrenOf, cmToMm, mmToCm, workspaceFor } from './projection';
import type { Snapshot, StorageNode } from './types';

interface Props { snapshot: Snapshot; focusNodeId?: string; onSnapshot: (snapshot: Snapshot) => void; onOpenTypes: () => void; }
interface ContextTarget { nodeId: string; x: number; y: number; }
interface EmptyContext { parentId: string; x: number; y: number; }

export function LayoutView({ snapshot, focusNodeId, onSnapshot, onOpenTypes }: Props) {
  const firstRoom = snapshot.nodes.find((node) => node.kind === 'room')?.id;
  const [workspaceId, setWorkspaceId] = useState<string | undefined>(firstRoom);
  const [workspaceHistory, setWorkspaceHistory] = useState<string[]>(firstRoom ? [firstRoom] : []);
  const [historyIndex, setHistoryIndex] = useState(0);
  const [selectedId, setSelectedId] = useState<string | undefined>(firstRoom);
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set(snapshot.nodes.filter((node) => node.kind === 'room').map((node) => node.id)));
  const [error, setError] = useState<string>();
  const [depthLimit, setDepthLimit] = useState(6);
  const [contextTarget, setContextTarget] = useState<ContextTarget>();
  const [kindPickerId, setKindPickerId] = useState<string>();
  const [emptyContext, setEmptyContext] = useState<EmptyContext>();
  useEffect(() => {
    if (!workspaceId || !snapshot.nodes.some((node) => node.id === workspaceId)) setWorkspaceId(firstRoom);
    if (selectedId && !snapshot.nodes.some((node) => node.id === selectedId)) setSelectedId(firstRoom);
  }, [snapshot, workspaceId, selectedId, firstRoom]);
  useEffect(() => { if (firstRoom && !workspaceHistory.length) { setWorkspaceHistory([firstRoom]); setHistoryIndex(0); } }, [firstRoom, workspaceHistory.length]);
  useEffect(() => {
    if (!focusNodeId) return;
    const node = snapshot.nodes.find((candidate) => candidate.id === focusNodeId);
    if (!node) return;
    setSelectedId(node.id);
    setWorkspaceId(workspaceFor(snapshot, node.id)?.id ?? firstRoom);
  }, [focusNodeId, snapshot, firstRoom]);
  const workspace = snapshot.nodes.find((node) => node.id === workspaceId) ?? snapshot.nodes.find((node) => node.kind === 'room');
  const selected = snapshot.nodes.find((node) => node.id === selectedId);
  const goToWorkspace = (id: string, record = true) => {
    setWorkspaceId(id);
    if (record) {
      setWorkspaceHistory((current) => {
        const trimmed = current.slice(0, historyIndex + 1);
        return trimmed.at(-1) === id ? trimmed : [...trimmed, id];
      });
      setHistoryIndex((current) => workspaceHistory.slice(0, current + 1).at(-1) === id ? current : current + 1);
    }
  };
  const openWorkspace = (id: string) => {
    const workspaceNode = workspaceFor(snapshot, id);
    if (workspaceNode?.id === id) goToWorkspace(id);
    else if (['room', 'rack', 'shelf', 'drawer'].includes(snapshot.nodes.find((node) => node.id === id)?.kind ?? '')) goToWorkspace(id);
    setSelectedId(id);
  };
  const goBack = () => { if (historyIndex <= 0) return; const next = historyIndex - 1; setHistoryIndex(next); setWorkspaceId(workspaceHistory[next]); };
  const goForward = () => { if (historyIndex >= workspaceHistory.length - 1) return; const next = historyIndex + 1; setHistoryIndex(next); setWorkspaceId(workspaceHistory[next]); };
  const goUp = () => { if (!workspace) return; const parent = snapshot.nodes.find((node) => node.id === workspace.parentId); const target = parent && workspaceFor(snapshot, parent.id); if (target) { goToWorkspace(target.id); setSelectedId(target.id); } };
  const showContextMenu = (nodeId: string, x: number, y: number) => {
    setSelectedId(nodeId);
    setContextTarget({ nodeId, x, y });
  };
  const moveInTree = async (nodeId: string, parentId: string) => {
    try {
      onSnapshot(await invokeSnapshot('move_node', { nodeId, parentId }));
      setSelectedId(nodeId);
    } catch (reason) { setError(String(reason)); }
  };
  return (
    <div className="layout-page" onClick={() => { setContextTarget(undefined); setEmptyContext(undefined); }} onContextMenu={(event) => { event.preventDefault(); if (workspace) setEmptyContext({ parentId: workspace.id, x: event.clientX, y: event.clientY }); }}>
      <aside className="layout-tree"><div className="pane-title"><FolderTree size={15} /><span>Physical tree</span><Tooltip label="Show this many levels of the tree"><NumberInput size="xs" w={58} min={1} max={12} value={depthLimit} onChange={(value) => setDepthLimit(Math.max(1, Math.min(12, Number(value) || 1)))} aria-label="Tree depth limit" /></Tooltip></div><ScrollArea className="tree-scroll" type="hover"><StorageTree snapshot={snapshot} depthLimit={depthLimit} selectedId={selectedId} expanded={expanded} onToggle={(id) => setExpanded((current) => { const next = new Set(current); next.has(id) ? next.delete(id) : next.add(id); return next; })} onSelect={setSelectedId} onOpen={openWorkspace} onMove={moveInTree} onContextMenu={showContextMenu} /></ScrollArea></aside>
      <main className="layout-center">
        {error && <div className="inline-error">{error}</div>}
        <Group className="workspace-navigation" gap={3}><Tooltip label="Back"><ActionIcon variant="subtle" disabled={historyIndex <= 0} onClick={goBack}><ArrowLeft size={15} /></ActionIcon></Tooltip><Tooltip label="Forward"><ActionIcon variant="subtle" disabled={historyIndex >= workspaceHistory.length - 1} onClick={goForward}><ArrowRight size={15} /></ActionIcon></Tooltip><Tooltip label="Up to parent workspace"><ActionIcon variant="subtle" disabled={!workspace?.parentId} onClick={goUp}><CornerLeftUp size={15} /></ActionIcon></Tooltip></Group>
        {workspace ? <SpatialCanvas snapshot={snapshot} workspace={workspace} selectedId={selectedId} onSelect={(id) => setSelectedId(id || undefined)} onOpenWorkspace={openWorkspace} onNodeContextMenu={showContextMenu} onSnapshot={onSnapshot} onError={setError} /> : <CreateRoom onSnapshot={onSnapshot} onError={setError} />}
      </main>
      <aside className="layout-inspector"><Inspector snapshot={snapshot} node={selected} onSnapshot={onSnapshot} onError={setError} /></aside>
      {contextTarget && <NodeActionsMenu snapshot={snapshot} target={contextTarget} onSnapshot={onSnapshot} onError={setError} onOpen={openWorkspace} onPickKind={setKindPickerId} onOpenTypes={onOpenTypes} onClose={() => setContextTarget(undefined)} />}
      {emptyContext && <EmptySpaceMenu snapshot={snapshot} target={emptyContext} onSnapshot={onSnapshot} onError={setError} onCreated={openWorkspace} onClose={() => setEmptyContext(undefined)} />}
      <StorageKindPicker snapshot={snapshot} nodeId={kindPickerId} onSnapshot={onSnapshot} onError={setError} onClose={() => setKindPickerId(undefined)} />
    </div>
  );
}

function CreateRoom({ onSnapshot, onError }: { onSnapshot: (snapshot: Snapshot) => void; onError: (message: string) => void }) {
  const form = useForm({ initialValues: { name: 'Bedroom', width: 400, depth: 350, height: 250 } });
  const create = form.onSubmit(async (values) => {
    try {
      onSnapshot(await invokeSnapshot('create_node', {
        name: values.name,
        kind: 'room',
        size: {
          width: cmToMm(Number(values.width)),
          depth: cmToMm(Number(values.depth)),
          height: cmToMm(Number(values.height)),
        },
      }));
    } catch (reason) { onError(String(reason)); }
  });
  return <Box className="empty-canvas"><Paper withBorder p="md" className="room-bootstrap" component="form" onSubmit={create}><Group gap={7}><Warehouse size={17} /><Text fw={700}>First physical space</Text></Group><Text size="xs" c="dimmed" mt={5}>Set the usable room bounds first. You can then add racks, wall shelves, and fixture blocks.</Text><TextInput label="Room name" size="xs" mt="sm" {...form.getInputProps('name')} /><Text size="xs" c="dimmed" mt="sm">Room dimensions — unit: cm</Text><div className="compact-number-grid"><NumberInput label="Width" size="xs" min={1} step={1} {...form.getInputProps('width')} /><NumberInput label="Depth" size="xs" min={1} step={1} {...form.getInputProps('depth')} /><NumberInput label="Ceiling height" size="xs" min={1} step={1} {...form.getInputProps('height')} /></div><Button type="submit" size="xs" mt="sm" leftSection={<Plus size={14} />}>Create room</Button></Paper></Box>;
}

function StorageTree({ snapshot, depthLimit, selectedId, expanded, onToggle, onSelect, onOpen, onMove, onContextMenu }: { snapshot: Snapshot; depthLimit: number; selectedId?: string; expanded: Set<string>; onToggle: (id: string) => void; onSelect: (id: string) => void; onOpen: (id: string) => void; onMove: (nodeId: string, parentId: string) => void; onContextMenu: (id: string, x: number, y: number) => void }) {
  const roots = snapshot.nodes.filter((node) => !node.parentId);
  return <Stack gap={1} p="xs">{roots.map((node) => <TreeRow key={node.id} snapshot={snapshot} node={node} depth={0} depthLimit={depthLimit} selectedId={selectedId} expanded={expanded} onToggle={onToggle} onSelect={onSelect} onOpen={onOpen} onMove={onMove} onContextMenu={onContextMenu} />)}</Stack>;
}

function TreeRow({ snapshot, node, depth, depthLimit, selectedId, expanded, onToggle, onSelect, onOpen, onMove, onContextMenu }: { snapshot: Snapshot; node: StorageNode; depth: number; depthLimit: number; selectedId?: string; expanded: Set<string>; onToggle: (id: string) => void; onSelect: (id: string) => void; onOpen: (id: string) => void; onMove: (nodeId: string, parentId: string) => void; onContextMenu: (id: string, x: number, y: number) => void }) {
  const children = childrenOf(snapshot, node.id).sort((left, right) => {
    if (node.kind === 'rack' && left.kind === 'shelf' && right.kind === 'shelf') {
      return (right.placement?.z ?? 0) - (left.placement?.z ?? 0);
    }
    return left.name.localeCompare(right.name);
  });
  const canExpand = children.length > 0 && depth < depthLimit;
  const isOpen = expanded.has(node.id);
  return <>
    <Group className="tree-row" data-selected={selectedId === node.id || undefined} gap={3} wrap="nowrap" pl={depth * 12} draggable onDragStart={(event) => { event.dataTransfer.setData('application/x-stuff-node', node.id); event.dataTransfer.effectAllowed = 'move'; }} onDragOver={(event) => { if (event.dataTransfer.types.includes('application/x-stuff-node') && node.allowedChildKinds.length) { event.preventDefault(); event.dataTransfer.dropEffect = 'move'; } }} onDrop={(event) => { const sourceId = event.dataTransfer.getData('application/x-stuff-node'); if (sourceId && sourceId !== node.id) onMove(sourceId, node.id); }} onClick={() => onSelect(node.id)} onContextMenu={(event) => { event.preventDefault(); event.stopPropagation(); onContextMenu(node.id, event.clientX, event.clientY); }} onDoubleClick={() => onOpen(node.id)} title={node.path}>
      <ActionIcon size={17} variant="subtle" color="gray" disabled={!canExpand} onClick={(event) => { event.stopPropagation(); if (canExpand) onToggle(node.id); }}>{canExpand && (isOpen ? <ChevronDown size={13} /> : <ChevronRight size={13} />)}</ActionIcon>
      <TreeIcon kind={node.kind} /><Text size="xs" truncate>{node.name}</Text>{node.kind === 'fixture' && <Text c="dimmed" size="xs">fixture</Text>}
    </Group>
    {canExpand && isOpen && children.map((child) => <TreeRow key={child.id} snapshot={snapshot} node={child} depth={depth + 1} depthLimit={depthLimit} selectedId={selectedId} expanded={expanded} onToggle={onToggle} onSelect={onSelect} onOpen={onOpen} onMove={onMove} onContextMenu={onContextMenu} />)}
    {children.length > 0 && depth === depthLimit && <Text size="xs" c="dimmed" pl={(depth + 1) * 12}>Descendants hidden by depth limit</Text>}
  </>;
}

function NodeActionsMenu({ snapshot, target, onSnapshot, onError, onOpen, onPickKind, onOpenTypes, onClose }: { snapshot: Snapshot; target: ContextTarget; onSnapshot: (snapshot: Snapshot) => void; onError: (message: string) => void; onOpen: (id: string) => void; onPickKind: (id: string) => void; onOpenTypes: () => void; onClose: () => void }) {
  const node = snapshot.nodes.find((candidate) => candidate.id === target.nodeId);
  if (!node) return null;
  const openable = node.kind !== 'fixture';
  const descendants = countSubtreeNodes(snapshot, node.id) - 1;
  const left = Math.max(8, Math.min(target.x, window.innerWidth - 236));
  const top = Math.max(8, Math.min(target.y, window.innerHeight - 170));
  const duplicate = async () => {
    try { onSnapshot(await invoke<Snapshot>('clone_storage_subtree', { nodeIdValue: node.id })); onClose(); } catch (reason) { onError(String(reason)); }
  };
  const rename = async () => {
    const name = window.prompt('Storage name', node.name);
    if (!name || name === node.name) return;
    try { onSnapshot(await invokeSnapshot('update_node', { nodeId: node.id, name })); onClose(); } catch (reason) { onError(String(reason)); }
  };
  const captureType = async () => {
    if (node.kind !== 'rack' && node.kind !== 'organizer') return;
    const templateName = window.prompt('New reusable type name', `${node.name} type`);
    if (!templateName) return;
    try { onSnapshot(await invokeSnapshot(node.kind === 'rack' ? 'save_rack_as_template' : 'save_organizer_as_template', node.kind === 'rack' ? { rackId: node.id, templateName } : { organizerNodeId: node.id, templateName })); onClose(); onOpenTypes(); } catch (reason) { onError(String(reason)); }
  };
  const removeWithChildren = async () => {
    const scope = descendants ? ` and its ${descendants} nested ${descendants === 1 ? 'object' : 'objects'}` : '';
    if (!window.confirm(`Delete “${node.name}”${scope}?\n\nStuff will refuse if any stock, reservations, or occupancy data are still inside it.`)) return;
    try { onSnapshot(await invoke<Snapshot>('delete_storage_subtree', { nodeIdValue: node.id })); onClose(); } catch (reason) { onError(String(reason)); }
  };
  const orphanChildren = async () => {
    if (!window.confirm(`Delete “${node.name}” but keep its direct children?\n\nThe children move to this object’s parent and become unplaced.`)) return;
    try { onSnapshot(await invoke<Snapshot>('delete_storage_orphan_children', { nodeIdValue: node.id })); onClose(); } catch (reason) { onError(String(reason)); }
  };
  const directChildren = snapshot.nodes.filter((candidate) => candidate.parentId === node.id).length;
  return <Paper className="node-actions-menu" withBorder shadow="md" style={{ left, top }} onClick={(event) => event.stopPropagation()}><Text size="xs" fw={700} lineClamp={1}>{node.name}</Text><Text size="xs" c="dimmed" mb={7}>{node.path}</Text>{openable && <Button size="xs" variant="subtle" fullWidth justify="flex-start" onClick={() => { onOpen(node.id); onClose(); }}>Open workspace</Button>}<Button size="xs" variant="subtle" fullWidth justify="flex-start" onClick={() => void rename()}>Rename…</Button><Button size="xs" variant="subtle" fullWidth justify="flex-start" onClick={() => { onPickKind(node.id); onClose(); }}>Change storage kind…</Button>{(node.kind === 'rack' || node.kind === 'organizer') && <Button size="xs" variant="subtle" fullWidth justify="flex-start" onClick={() => void captureType()}>Make reusable type…</Button>}<Button size="xs" variant="subtle" fullWidth justify="flex-start" leftSection={<Copy size={13} />} onClick={() => void duplicate()}>Duplicate layout</Button><Divider my={5} />{descendants > 0 ? <Button size="xs" variant="subtle" color="red" fullWidth justify="flex-start" leftSection={<Trash2 size={13} />} onClick={() => void removeWithChildren()}>Delete with children</Button> : <Button size="xs" variant="subtle" color="red" fullWidth justify="flex-start" leftSection={<Trash2 size={13} />} onClick={() => void removeWithChildren()}>Delete</Button>}{directChildren > 0 && <Button size="xs" variant="subtle" color="orange" fullWidth justify="flex-start" onClick={() => void orphanChildren()}>Delete, orphan children</Button>}</Paper>;
}

function EmptySpaceMenu({ snapshot, target, onSnapshot, onError, onCreated, onClose }: { snapshot: Snapshot; target: EmptyContext; onSnapshot: (snapshot: Snapshot) => void; onError: (message: string) => void; onCreated: (id: string) => void; onClose: () => void }) {
  const parent = snapshot.nodes.find((node) => node.id === target.parentId);
  const organizerTypes = snapshot.organizerTemplates;
  const rackTypes = snapshot.rackTemplates;
  const form = useForm({ initialValues: { kind: parent?.allowedChildKinds[0] ?? '', name: '', templateId: '' } });
  useEffect(() => form.setValues({ kind: parent?.allowedChildKinds[0] ?? '', name: '', templateId: '' }), [parent?.id]);
  if (!parent) return null;
  const create = form.onSubmit(async (values) => {
    try {
      const before = new Set(snapshot.nodes.map((node) => node.id));
      let next: Snapshot;
      if (values.kind === 'rack' && parent.kind === 'room') next = await invokeSnapshot('instantiate_rack_template', { rackName: values.name, roomId: parent.id, templateId: values.templateId, placement: { x: 0, y: 0, z: 0 } });
      else if (values.kind === 'organizer') next = await invokeSnapshot('create_organizer_from_template', { organizerName: values.name, parentId: parent.id, templateId: values.templateId, placement: { x: 0, y: 0, z: 0 } });
      else next = await invokeSnapshot('create_node', { name: values.name, kind: values.kind, parentId: parent.id, size: { width: 300, depth: 200, height: 100 }, placement: { x: 0, y: 0, z: 0 } });
      onSnapshot(next);
      const created = next.nodes.find((node) => !before.has(node.id));
      if (created) onCreated(created.id);
      onClose();
    } catch (reason) { onError(String(reason)); }
  });
  const typeChoices = form.values.kind === 'rack' ? rackTypes : organizerTypes;
  const needsType = form.values.kind === 'rack' || form.values.kind === 'organizer';
  useEffect(() => { if (needsType && !form.values.templateId && typeChoices[0]) form.setFieldValue('templateId', typeChoices[0].id); }, [form.values.kind, typeChoices[0]?.id]);
  const left = Math.max(8, Math.min(target.x, window.innerWidth - 290));
  const top = Math.max(8, Math.min(target.y, window.innerHeight - 260));
  return <Paper className="empty-actions-menu" withBorder shadow="md" style={{ left, top }} onClick={(event) => event.stopPropagation()}><Text size="xs" fw={700}>Add to {parent.name}</Text><Box component="form" onSubmit={create}><Select label="Storage kind" size="xs" mt="sm" data={parent.allowedChildKinds.map((kind) => ({ value: kind, label: storageKindLabel(kind) }))} {...form.getInputProps('kind')} />{needsType && <Select label="Reusable type" size="xs" mt="xs" data={typeChoices.map((type) => ({ value: type.id, label: type.name }))} {...form.getInputProps('templateId')} />}<TextInput label="Name" size="xs" mt="xs" {...form.getInputProps('name')} /><Button type="submit" size="xs" mt="sm" leftSection={<Plus size={13} />}>Add</Button></Box></Paper>;
}

function StorageKindPicker({ snapshot, nodeId, onSnapshot, onError, onClose }: { snapshot: Snapshot; nodeId?: string; onSnapshot: (snapshot: Snapshot) => void; onError: (message: string) => void; onClose: () => void }) {
  const node = snapshot.nodes.find((candidate) => candidate.id === nodeId);
  const parent = snapshot.nodes.find((candidate) => candidate.id === node?.parentId);
  const kinds = node ? (parent?.allowedChildKinds ?? (node.kind === 'room' ? ['room'] : [])) : [];
  const currentTemplateId = snapshot.organizers.find((instance) => instance.nodeId === node?.id)?.templateId ?? '';
  const form = useForm({ initialValues: { kind: node?.kind ?? '', templateId: currentTemplateId } });
  useEffect(() => form.setValues({ kind: node?.kind ?? '', templateId: currentTemplateId }), [node?.id, currentTemplateId]);
  const submit = form.onSubmit(async (values) => {
    if (!node) return;
    try {
      if (values.kind === 'organizer' && !values.templateId) throw new Error('Choose an organizer type.');
      const next = values.kind === 'organizer'
        ? await invokeSnapshot('assign_organizer_template', { organizerNodeId: node.id, templateId: values.templateId })
        : await invokeSnapshot('update_node', { nodeId: node.id, kind: values.kind });
      onSnapshot(next);
      onClose();
    } catch (reason) { onError(String(reason)); }
  });
  return <Modal opened={Boolean(node)} onClose={onClose} title="Change storage kind" size="sm"><Text size="sm" c="dimmed">A cell-less organizer is one undivided storage area. A typed organizer uses the cells from a reusable organizer type.</Text><Box component="form" onSubmit={submit}><Select label="Storage kind" mt="sm" data={kinds.map((kind) => ({ value: kind, label: storageKindLabel(kind) }))} {...form.getInputProps('kind')} />{form.values.kind === 'organizer' && <Select label="Organizer type" mt="sm" data={snapshot.organizerTemplates.map((template) => ({ value: template.id, label: template.name }))} {...form.getInputProps('templateId')} />}<Group justify="end" mt="md"><Button variant="default" onClick={onClose}>Cancel</Button><Button type="submit">Change kind</Button></Group></Box></Modal>;
}

function countSubtreeNodes(snapshot: Snapshot, rootId: string): number {
  const pending = [rootId];
  let count = 0;
  while (pending.length) {
    const id = pending.pop()!;
    count += 1;
    pending.push(...snapshot.nodes.filter((node) => node.parentId === id).map((node) => node.id));
  }
  return count;
}

function Inspector({ snapshot, node, onSnapshot, onError }: { snapshot: Snapshot; node?: StorageNode; onSnapshot: (snapshot: Snapshot) => void; onError: (message: string) => void }) {
  if (!node) return <Box className="inspector-empty"><PanelTop size={20} /><Text size="sm" c="dimmed">Select one physical object.</Text></Box>;
  return <ScrollArea className="inspector-scroll" type="hover"><div className="pane-title"><TreeIcon kind={node.kind} /><span>Properties</span></div><Box p="sm"><Badge variant="light" color={node.kind === 'fixture' ? 'gray' : 'cyan'}>{storageKindLabel(node.kind)}</Badge><Text fw={700} mt={6}>{node.name}</Text><Text size="xs" c="dimmed" lineClamp={2}>{node.path}</Text><Text size="xs" c="dimmed" mt="sm">Right-click this object in the tree or a view for structural actions: move, change kind, duplicate, capture a type, or delete.</Text><Divider my="sm" /><NodeEditor snapshot={snapshot} node={node} onSnapshot={onSnapshot} onError={onError} /></Box></ScrollArea>;
}

function RackFromType({ snapshot, room, onSnapshot, onError, onCreated }: { snapshot: Snapshot; room: StorageNode; onSnapshot: (snapshot: Snapshot) => void; onError: (message: string) => void; onCreated: (id: string) => void }) {
  const templates = snapshot.rackTemplates;
  const form = useForm({ initialValues: { rackName: '', templateId: templates[0]?.id ?? '' } });
  useEffect(() => { if (!form.values.templateId && templates[0]) form.setFieldValue('templateId', templates[0].id); }, [templates[0]?.id]);
  const create = form.onSubmit(async (values) => {
    try {
      const previousIds = new Set(snapshot.nodes.map((node) => node.id));
      const next = await invokeSnapshot('instantiate_rack_template', { ...values, roomId: room.id, placement: { x: 0, y: 0, z: 0 } });
      onSnapshot(next);
      const rack = next.nodes.find((node) => node.kind === 'rack' && !previousIds.has(node.id));
      if (rack) onCreated(rack.id);
    } catch (reason) { onError(String(reason)); }
  });
  if (!templates.length) return <Box><Text size="xs" fw={700} tt="uppercase" c="dimmed">Add rack from type</Text><Text size="xs" c="dimmed" mt={4}>Create a rack type in Types first, then return here to place it.</Text></Box>;
  return <Box component="form" onSubmit={create}><Text size="xs" fw={700} tt="uppercase" c="dimmed">Add rack from type</Text><Text size="xs" c="dimmed" mt={4}>This creates the rack and all of its shelf levels here. Drag the rack into place afterwards.</Text><TextInput label="Rack name" size="xs" mt="sm" {...form.getInputProps('rackName')} /><Select label="Rack type" size="xs" mt="xs" data={templates.map((template) => ({ value: template.id, label: `${template.name} · ${mmToCm(template.outerSize.width)} × ${mmToCm(template.outerSize.depth)} × ${mmToCm(template.outerSize.height)} cm` }))} {...form.getInputProps('templateId')} /><Button type="submit" size="xs" mt="sm" leftSection={<Plus size={14} />}>Add rack</Button></Box>;
}

function SaveNodeAsType({ node, onSnapshot, onError, onOpenTypes }: { node: StorageNode; onSnapshot: (snapshot: Snapshot) => void; onError: (message: string) => void; onOpenTypes: () => void }) {
  const form = useForm({ initialValues: { templateName: `${node.name} type` } });
  useEffect(() => form.setFieldValue('templateName', `${node.name} type`), [node.id]);
  const save = form.onSubmit(async (values) => {
    try {
      const command = node.kind === 'rack' ? 'save_rack_as_template' : 'save_organizer_as_template';
      const input = node.kind === 'rack' ? { rackId: node.id, templateName: values.templateName } : { organizerNodeId: node.id, templateName: values.templateName };
      onSnapshot(await invokeSnapshot(command, input));
      onOpenTypes();
    } catch (reason) { onError(String(reason)); }
  });
  const noun = node.kind === 'rack' ? 'rack' : 'organizer';
  return <Box component="form" onSubmit={save}><Text size="xs" fw={700} tt="uppercase" c="dimmed">Make reusable {noun} type</Text><Text size="xs" c="dimmed" mt={4}>Captures this physical {noun} and switches it to the new type, even if it already used another type.</Text><TextInput label="New type name" size="xs" mt="sm" {...form.getInputProps('templateName')} /><Button type="submit" size="xs" mt="sm" leftSection={<Copy size={14} />}>Create type</Button></Box>;
}

function NodeEditor({ snapshot, node, onSnapshot, onError }: { snapshot: Snapshot; node: StorageNode; onSnapshot: (snapshot: Snapshot) => void; onError: (message: string) => void }) {
  const form = useForm({ initialValues: nodeValues(node) });
  useEffect(() => form.setValues(nodeValues(node)), [node.id]);
  const organizerTemplate = node.kind === 'organizer' ? snapshot.organizerTemplates.find((template) => snapshot.organizers.find((organizer) => organizer.nodeId === node.id)?.templateId === template.id) : undefined;
  const parent = snapshot.nodes.find((candidate) => candidate.id === node.parentId);
  const rackShelf = node.kind === 'shelf' && parent?.kind === 'rack';
  const save = async (values = form.getValues()) => {
    try {
      const input: Record<string, unknown> = { nodeId: node.id, name: values.name };
      if (node.parentId) {
        input.placement = rackShelf
          ? { x: node.placement?.x ?? 0, y: node.placement?.y ?? 0, z: cmToMm(Number(values.z)) }
          : { x: cmToMm(Number(values.x)), y: cmToMm(Number(values.y)), z: cmToMm(Number(values.z)) };
      }
      if (node.kind !== 'organizer') {
        input.size = rackShelf
          ? { width: node.size?.width ?? 1, depth: node.size?.depth ?? 1, height: cmToMm(Number(values.height)) }
          : { width: cmToMm(Number(values.width)), depth: cmToMm(Number(values.depth)), height: cmToMm(Number(values.height)) };
      }
      onSnapshot(await invokeSnapshot('update_node', input));
    } catch (reason) { onError(String(reason)); }
  };
  const sizeHeading = node.dimensionSemantics === 'usable' ? 'Usable dimensions' : node.dimensionSemantics === 'roomBounds' ? 'Room dimensions' : 'Outer dimensions';
  return <Box onBlurCapture={(event) => { const input = event.target as HTMLInputElement; if (input.name) void save({ ...form.getValues(), [input.name]: input.value }); }}><TextInput label="Name" size="xs" {...form.getInputProps('name')} />{node.kind === 'organizer' ? <Text size="xs" c="dimmed" mt="sm">Size comes from organizer type <b>{organizerTemplate?.name ?? 'unknown'}</b>{organizerTemplate && ` · ${mmToCm(organizerTemplate.outerSize.width)} × ${mmToCm(organizerTemplate.outerSize.depth)} × ${mmToCm(organizerTemplate.outerSize.height)} cm`}</Text> : rackShelf ? <><Text size="xs" c="dimmed" mt="sm">This shelf uses the rack’s inset footprint: {mmToCm(node.size?.width ?? 0)} × {mmToCm(node.size?.depth ?? 0)} cm.</Text><Text size="xs" c="dimmed" mt="sm">Shelf level — unit: cm</Text><div className="compact-number-grid"><NumberInput label="Base height" size="xs" min={0} step={0.5} decimalScale={1} {...form.getInputProps('z')} /><NumberInput label="Clear height" size="xs" min={0.5} step={0.5} decimalScale={1} {...form.getInputProps('height')} /></div></> : <><Text size="xs" c="dimmed" mt="sm">{sizeHeading} — unit: cm</Text><div className="compact-number-grid"><NumberInput label="Width" size="xs" min={0.5} step={0.5} decimalScale={1} {...form.getInputProps('width')} /><NumberInput label="Depth" size="xs" min={0.5} step={0.5} decimalScale={1} {...form.getInputProps('depth')} /><NumberInput label={node.kind === 'room' ? 'Ceiling height' : 'Height'} size="xs" min={0.5} step={0.5} decimalScale={1} {...form.getInputProps('height')} /></div></>}{node.parentId && !rackShelf && <PlacementInputs form={form} parentName={parent?.name} />}<Text size="xs" c="dimmed" mt="sm">Edits apply when you leave a field.</Text></Box>;
}

function CreateChild({ snapshot, node, onSnapshot, onError, onOpenTypes }: { snapshot: Snapshot; node: StorageNode; onSnapshot: (snapshot: Snapshot) => void; onError: (message: string) => void; onOpenTypes: () => void }) {
  const options = node.allowedChildKinds.map((kind) => ({ value: kind, label: storageKindLabel(kind) }));
  const organizerTypes = snapshot.organizerTemplates;
  const form = useForm({ initialValues: childValues(node, organizerTypes[0]?.id) });
  useEffect(() => form.setValues(childValues(node, organizerTypes[0]?.id)), [node.id, organizerTypes[0]?.id]);
  if (!options.length) return <Text size="xs" c="dimmed">This object cannot contain physical storage.</Text>;
  const create = form.onSubmit(async (values) => {
    try {
      const size = { width: cmToMm(Number(values.width)), depth: cmToMm(Number(values.depth)), height: cmToMm(Number(values.height)) };
      const placement = { x: cmToMm(Number(values.x)), y: cmToMm(Number(values.y)), z: cmToMm(Number(values.z)) };
      if (values.kind === 'shelf' && node.kind === 'rack') {
        onSnapshot(await invokeSnapshot('create_shelf', { name: values.name, parentId: node.id, placementKind: 'rack', altitude: cmToMm(Number(values.altitude)), usableHeight: cmToMm(Number(values.usableHeight)) }));
      } else if (values.kind === 'shelf' && node.kind === 'room') {
        onSnapshot(await invokeSnapshot('create_shelf', { name: values.name, parentId: node.id, placementKind: 'room', usableSize: size, placement }));
      } else if (values.kind === 'organizer') {
        if (!values.templateId) { onError('Create or select an organizer type first.'); return; }
        onSnapshot(await invokeSnapshot('create_organizer_from_template', { organizerName: values.name, parentId: node.id, templateId: values.templateId, placement }));
      } else {
        onSnapshot(await invokeSnapshot('create_node', { name: values.name, kind: values.kind, parentId: node.id, size, placement }));
      }
      form.setFieldValue('name', '');
    } catch (reason) { onError(String(reason)); }
  });
  const kind = form.values.kind;
  const rackShelf = kind === 'shelf' && node.kind === 'rack';
  const roomShelf = kind === 'shelf' && node.kind === 'room';
  return <Box component="form" onSubmit={create}><Text size="xs" fw={700} tt="uppercase" c="dimmed" mb={6}>Add storage here</Text><TextInput label="Name" placeholder={`e.g. ${kind === 'shelf' ? 'Parts shelf' : kind === 'fixture' ? 'Workbench' : 'Cable box'}`} size="xs" {...form.getInputProps('name')} /><Select label="Storage kind" data={options} size="xs" mt="xs" {...form.getInputProps('kind')} />{rackShelf ? <><Text size="xs" c="dimmed" mt="sm">Shelf width and depth are inherited from this rack: {mmToCm(Math.max(0, (node.size?.width ?? 0) - 10))} × {mmToCm(Math.max(0, (node.size?.depth ?? 0) - 10))} cm.</Text><Text size="xs" c="dimmed" mt="sm">Shelf level (cm)</Text><div className="compact-number-grid"><NumberInput label="Base height" size="xs" min={0} step={0.5} decimalScale={1} {...form.getInputProps('altitude')} /><NumberInput label="Clear height" size="xs" min={0.5} step={0.5} decimalScale={1} {...form.getInputProps('usableHeight')} /></div></> : kind === 'organizer' ? <>{organizerTypes.length ? <Select label="Organizer type" size="xs" mt="xs" data={organizerTypes.map((template) => ({ value: template.id, label: `${template.name} · ${mmToCm(template.outerSize.width)} × ${mmToCm(template.outerSize.depth)} × ${mmToCm(template.outerSize.height)} cm` }))} {...form.getInputProps('templateId')} /> : <Button variant="light" size="xs" mt="sm" onClick={onOpenTypes}>Make an organizer type</Button>}<PlacementInputs form={form} parentName={node.name} /></> : <><Text size="xs" c="dimmed" mt="sm">{kind === 'shelf' ? 'Usable shelf dimensions' : 'Outer dimensions'} (cm)</Text><div className="compact-number-grid"><NumberInput label="Width" size="xs" min={0.5} step={0.5} decimalScale={1} {...form.getInputProps('width')} /><NumberInput label="Depth" size="xs" min={0.5} step={0.5} decimalScale={1} {...form.getInputProps('depth')} /><NumberInput label="Height" size="xs" min={0.5} step={0.5} decimalScale={1} {...form.getInputProps('height')} /></div>{(roomShelf || kind !== 'shelf') && <PlacementInputs form={form} parentName={node.name} />}</>}<Button type="submit" size="xs" mt="sm" leftSection={<Plus size={14} />} disabled={kind === 'organizer' && !organizerTypes.length}>Add {storageKindLabel(kind).toLowerCase()}</Button></Box>;
}

function PlacementInputs({ form, parentName }: { form: any; parentName?: string }) {
  return <><Text size="xs" c="dimmed" mt="sm">Position inside {parentName ?? 'parent'} (cm)</Text><div className="compact-number-grid"><NumberInput label="Left" size="xs" step={0.5} decimalScale={1} {...form.getInputProps('x')} /><NumberInput label="From front" size="xs" step={0.5} decimalScale={1} {...form.getInputProps('y')} /><NumberInput label="Base height" size="xs" step={0.5} decimalScale={1} {...form.getInputProps('z')} /></div></>;
}

function childValues(node: StorageNode, templateId?: string) {
  return { name: '', kind: node.allowedChildKinds[0] ?? '', width: 30, depth: 20, height: 10, x: 0, y: 0, z: 0, altitude: 0, usableHeight: 10, templateId: templateId ?? '' };
}

function storageKindLabel(kind: string) {
  return ({ rack: 'Rack', shelf: 'Shelf', drawer: 'Drawer', fixture: 'Fixture block', cell_less_organizer: 'Cell-less organizer', organizer: 'Organizer (from type)', box: 'Box', bin: 'Bin', loose: 'Loose items area', storage: 'Generic storage' } as Record<string, string>)[kind] ?? kind;
}

function nodeValues(node: StorageNode) {
  return { name: node.name, width: mmToCm(node.size?.width ?? 100), depth: mmToCm(node.size?.depth ?? 100), height: mmToCm(node.size?.height ?? 100), x: mmToCm(node.placement?.x ?? 0), y: mmToCm(node.placement?.y ?? 0), z: mmToCm(node.placement?.z ?? 0) };
}

function TreeIcon({ kind }: { kind: string }) {
  const props = { size: 14, strokeWidth: 1.7 };
  if (kind === 'room') return <Warehouse {...props} />;
  if (kind === 'rack') return <Layers3 {...props} />;
  if (kind === 'shelf') return <Text component="span" size="sm" c="dimmed">—</Text>;
  if (kind === 'drawer') return <Archive {...props} />;
  if (kind === 'fixture') return <Armchair {...props} />;
  if (kind === 'organizer' || kind === 'cell_less_organizer') return <Boxes {...props} />;
  if (kind === 'box' || kind === 'bin') return <BoxIcon {...props} />;
  return <PackageOpen {...props} />;
}
