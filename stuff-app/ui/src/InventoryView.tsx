import {
  ActionIcon, Autocomplete, Badge, Box, Button, Divider, Group, Modal, NumberInput, Paper, ScrollArea, Select,
  Stack, Table, TagsInput, Text, TextInput, Tooltip,
} from '@mantine/core';
import { useForm } from '@mantine/form';
import { ChevronDown, ChevronRight, CircleMinus, CirclePlus, PackagePlus, Search, X } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { invokeSnapshot } from './api';
import { cmToMm, mmToCm } from './projection';
import type { InventoryItem, Snapshot } from './types';

interface Props {
  snapshot: Snapshot;
  onSnapshot: (snapshot: Snapshot) => void;
}

interface LocationOption {
  value: string;
  label: string;
  nodeId: string;
  organizerId?: string;
  compartmentKey?: string;
  spatial: boolean;
}

export function InventoryView({ snapshot, onSnapshot }: Props) {
  const [query, setQuery] = useState('');
  const [category, setCategory] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string>();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const [contextItem, setContextItem] = useState<{ item: InventoryItem; x: number; y: number }>();
  const selected = snapshot.items.find((item) => item.id === selectedId) ?? snapshot.items[0];
  const categories = useMemo(
    () => [...new Set(snapshot.items.map((item) => item.category).filter(Boolean) as string[])].sort(),
    [snapshot.items],
  );
  const manufacturers = useMemo(
    () => [...new Set(snapshot.items.map((item) => item.manufacturer).filter(Boolean) as string[])].sort(),
    [snapshot.items],
  );
  const locations = useMemo(() => inventoryLocations(snapshot), [snapshot]);
  const visible = snapshot.items.filter((item) => {
    const q = query.trim().toLowerCase();
    const searchable = [item.name, item.manufacturerPartNumber, item.category, ...item.tags].filter(Boolean).join(' ').toLowerCase();
    return (!q || searchable.includes(q)) && (!category || item.category === category);
  });

  const mutate = async (command: string, input: Record<string, unknown>) => {
    setBusy(true);
    try {
      onSnapshot(await invokeSnapshot(command, input));
      setError(undefined);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="inventory-page" onClick={() => setContextItem(undefined)}>
      <div className="section-toolbar">
        <Group gap="xs" wrap="nowrap">
          <TextInput value={query} onChange={(event) => setQuery(event.currentTarget.value)} leftSection={<Search size={15} />} placeholder="Search name, part number, tags…" className="inventory-search" size="xs" />
          <Select value={category} onChange={setCategory} data={categories} placeholder="All categories" clearable size="xs" w={175} />
          <Badge variant="default" size="sm">{visible.length} types</Badge>
        </Group>
      </div>
      {error && <div className="inline-error">{error}</div>}
      <Paper className="inventory-table-wrap" withBorder radius={0}>
        <ScrollArea className="inventory-scroll" type="hover">
          <Table highlightOnHover stickyHeader horizontalSpacing="md" verticalSpacing={7} className="inventory-table">
            <Table.Thead>
              <Table.Tr><Table.Th>Item</Table.Th><Table.Th>Part number</Table.Th><Table.Th>Category / tags</Table.Th><Table.Th ta="right">Total</Table.Th><Table.Th>Locations</Table.Th></Table.Tr>
            </Table.Thead>
            <Table.Tbody>
              {visible.map((item) => (
                <Table.Tr key={item.id} data-selected={selected?.id === item.id || undefined} onClick={() => setSelectedId(item.id)} onContextMenu={(event) => { event.preventDefault(); event.stopPropagation(); setSelectedId(item.id); setContextItem({ item, x: event.clientX, y: event.clientY }); }}>
                  <Table.Td><Text size="sm" fw={600}>{item.name}</Text><Text size="xs" c="dimmed">{item.manufacturer ?? '—'}</Text></Table.Td>
                  <Table.Td><Text ff="monospace" size="xs">{item.manufacturerPartNumber ?? '—'}</Text></Table.Td>
                  <Table.Td><Group gap={4}>{item.category && <Badge size="xs" variant="outline">{item.category}</Badge>}{item.tags.map((tag) => <Badge key={tag} size="xs" variant="light" color="gray">{tag}</Badge>)}</Group></Table.Td>
                  <Table.Td ta="right"><Text fw={700}>{item.totalQuantity}</Text></Table.Td>
                  <Table.Td><Text size="xs" c="dimmed">{item.locations.map((location) => location.path).join(' · ') || 'No stock location'}</Text></Table.Td>
                </Table.Tr>
              ))}
              {!visible.length && <Table.Tr><Table.Td colSpan={5}><Text ta="center" c="dimmed" py="xl">No matching inventory. Add an item below; this browser stays put.</Text></Table.Td></Table.Tr>}
            </Table.Tbody>
          </Table>
        </ScrollArea>
      </Paper>
      <div className="inventory-bottom">
        <ItemDetail item={selected} locations={locations} busy={busy} onAdjust={(lotId, delta) => void mutate('adjust_stock', { lotId, delta })} onMove={(input) => void mutate('move_stock', input)} />
        <AddStockForm locations={locations} manufacturers={manufacturers} categories={categories} busy={busy} onSubmit={(input) => void mutate('add_or_create_item', input)} />
      </div>
      {contextItem && <Paper className="inventory-actions-menu" withBorder shadow="md" style={{ left: Math.min(contextItem.x, window.innerWidth - 220), top: Math.min(contextItem.y, window.innerHeight - 130) }} onClick={(event) => event.stopPropagation()}><Text size="xs" fw={700}>{contextItem.item.name}</Text><Button size="xs" variant="subtle" fullWidth justify="flex-start" onClick={() => { setSelectedId(contextItem.item.id); setContextItem(undefined); }}>Show locations</Button>{contextItem.item.locations.length === 1 && <Button size="xs" variant="subtle" fullWidth justify="flex-start" onClick={() => { void mutate('adjust_stock', { lotId: contextItem.item.locations[0].lotId, delta: 1 }); setContextItem(undefined); }}>Add one at this location</Button>}<Button size="xs" variant="subtle" fullWidth justify="flex-start" onClick={() => { void navigator.clipboard?.writeText(contextItem.item.name); setContextItem(undefined); }}>Copy item name</Button></Paper>}
    </div>
  );
}

function ItemDetail({ item, locations, busy, onAdjust, onMove }: { item?: InventoryItem; locations: LocationOption[]; busy: boolean; onAdjust: (lotId: string, delta: number) => void; onMove: (input: Record<string, unknown>) => void }) {
  if (!item) return <Paper withBorder className="detail-pane empty-pane"><Text c="dimmed">Select an inventory row to inspect stock lots.</Text></Paper>;
  return (
    <Paper withBorder className="detail-pane">
      <Group justify="space-between" align="start"><Box><Text fw={700}>{item.name}</Text><Text size="xs" c="dimmed">{item.manufacturerPartNumber ?? 'No part number'} · total {item.totalQuantity}{item.size && ` · ${mmToCm(item.size.width)} × ${mmToCm(item.size.depth)} × ${mmToCm(item.size.height)} cm`}</Text></Box><Badge color="cyan" variant="light">{item.locations.length} lots</Badge></Group>
      <Divider my="sm" />
      <LocationTree locations={item.locations} busy={busy} onAdjust={onAdjust} />
      {!!item.locations.length && <MoveStockForm item={item} locations={locations} busy={busy} onMove={onMove} />}
    </Paper>
  );
}

type LocationBranch = { children: Map<string, LocationBranch>; locations: InventoryItem['locations'] };

function LocationTree({ locations, busy, onAdjust }: { locations: InventoryItem['locations']; busy: boolean; onAdjust: (lotId: string, delta: number) => void }) {
  const tree = useMemo(() => {
    const root: LocationBranch = { children: new Map(), locations: [] };
    for (const location of locations) {
      let branch = root;
      for (const part of location.path.split(' / ')) {
        if (!branch.children.has(part)) branch.children.set(part, { children: new Map(), locations: [] });
        branch = branch.children.get(part)!;
      }
      branch.locations.push(location);
    }
    return root;
  }, [locations]);
  return <Stack gap={3}>{[...tree.children.entries()].map(([name, branch]) => <LocationBranchView key={name} name={name} branch={branch} depth={0} busy={busy} onAdjust={onAdjust} />)}{!locations.length && <Text size="sm" c="dimmed">No stock lots yet.</Text>}</Stack>;
}

function LocationBranchView({ name, branch, depth, busy, onAdjust }: { name: string; branch: LocationBranch; depth: number; busy: boolean; onAdjust: (lotId: string, delta: number) => void }) {
  const onlyLot = branch.locations.length === 1 && branch.children.size === 0 ? branch.locations[0] : undefined;
  return <Box className="location-tree-row" pl={depth * 12}><Group justify="space-between" wrap="nowrap"><Text size="xs" fw={onlyLot ? 600 : 500}>{name}</Text>{onlyLot && <LotControls location={onlyLot} busy={busy} onAdjust={onAdjust} />}</Group>{[...branch.children.entries()].map(([childName, child]) => <LocationBranchView key={childName} name={childName} branch={child} depth={depth + 1} busy={busy} onAdjust={onAdjust} />)}{branch.locations.length > 1 && branch.locations.map((location) => <Box key={location.lotId} pl={12}><LotControls location={location} busy={busy} onAdjust={onAdjust} /></Box>)}</Box>;
}

function LotControls({ location, busy, onAdjust }: { location: InventoryItem['locations'][number]; busy: boolean; onAdjust: (lotId: string, delta: number) => void }) {
  return <Group gap={5} wrap="nowrap"><Tooltip label="Remove one from this location"><ActionIcon variant="subtle" color="red" size="sm" disabled={busy || location.quantity === 0} onClick={() => onAdjust(location.lotId, -1)}><CircleMinus size={15} /></ActionIcon></Tooltip><Text fw={700} miw={22} ta="center" size="xs">{location.quantity}</Text><Tooltip label="Add one to this location"><ActionIcon variant="subtle" color="cyan" size="sm" disabled={busy} onClick={() => onAdjust(location.lotId, 1)}><CirclePlus size={15} /></ActionIcon></Tooltip></Group>;
}

function MoveStockForm({ item, locations, busy, onMove }: { item: InventoryItem; locations: LocationOption[]; busy: boolean; onMove: (input: Record<string, unknown>) => void }) {
  const form = useForm({
    initialValues: { fromLotId: item.locations[0]?.lotId ?? '', destinationKey: locations[0]?.value ?? '', quantity: 1, width: mmToCm(item.size?.width ?? 100), depth: mmToCm(item.size?.depth ?? 100), height: mmToCm(item.size?.height ?? 100) },
    validate: (values) => {
      const spatial = locations.find((location) => location.value === values.destinationKey)?.spatial;
      return spatial && (!item.size && [values.width, values.depth, values.height].some((value) => Number(value) <= 0))
        ? { width: 'Positive dimensions are required', depth: 'Positive dimensions are required', height: 'Positive dimensions are required' }
        : {};
    },
  });
  useEffect(() => { if (!item.locations.some((location) => location.lotId === form.values.fromLotId)) form.setFieldValue('fromLotId', item.locations[0]?.lotId ?? ''); }, [item.id, item.locations[0]?.lotId]);
  useEffect(() => form.setValues({ ...form.getValues(), width: mmToCm(item.size?.width ?? 100), depth: mmToCm(item.size?.depth ?? 100), height: mmToCm(item.size?.height ?? 100) }), [item.id, item.size?.width, item.size?.depth, item.size?.height]);
  const submit = form.onSubmit((values) => {
    const destination = locations.find((location) => location.value === values.destinationKey);
    if (!destination) return;
    onMove({ fromLotId: values.fromLotId, toNodeId: destination.nodeId, toOrganizerId: destination.organizerId, toCompartmentKey: destination.compartmentKey, quantity: Number(values.quantity), itemSize: destination.spatial ? (item.size ?? { width: cmToMm(Number(values.width)), depth: cmToMm(Number(values.depth)), height: cmToMm(Number(values.height)) }) : undefined });
  });
  const sources = item.locations.map((location) => ({ value: location.lotId, label: `${location.path} · ${location.quantity}` }));
  const destination = locations.find((location) => location.value === form.values.destinationKey);
  return <Box component="form" onSubmit={submit} mt="sm"><Text size="xs" fw={700} tt="uppercase" c="dimmed">Move stock</Text><Stack gap={7} mt={6}><StorageLocationPicker label="Take from" value={form.values.fromLotId} locations={sources} onChange={(value) => form.setFieldValue('fromLotId', value)} /><StorageLocationPicker label="Put in" value={form.values.destinationKey} locations={locations} onChange={(value) => form.setFieldValue('destinationKey', value)} />{destination?.spatial && (item.size ? <Text size="xs" c="dimmed">Workspace size: {mmToCm(item.size.width)} × {mmToCm(item.size.depth)} × {mmToCm(item.size.height)} cm</Text> : <Group grow><NumberInput label="Width (cm)" size="xs" min={0.1} {...form.getInputProps('width')} /><NumberInput label="Depth (cm)" size="xs" min={0.1} {...form.getInputProps('depth')} /><NumberInput label="Height (cm)" size="xs" min={0.1} {...form.getInputProps('height')} /></Group>)}<Group justify="space-between" align="end"><NumberInput label="Quantity to move" size="xs" min={1} w={145} {...form.getInputProps('quantity')} /><Button type="submit" size="xs" loading={busy}>Move stock</Button></Group></Stack></Box>;
}

function completeWithTab(event: React.KeyboardEvent<HTMLInputElement>, values: string[], setValue: (value: string) => void, current: string) {
  if (event.key !== 'Tab' || !current.trim()) return;
  const match = values.find((value) => value.toLowerCase().startsWith(current.trim().toLowerCase()));
  if (match && match !== current) setValue(match);
}

type PickerLocation = Pick<LocationOption, 'value' | 'label'>;
type PickerBranch = { children: Map<string, PickerBranch>; option?: PickerLocation };

function StorageLocationPicker({ label, value, locations, onChange }: { label: string; value: string; locations: PickerLocation[]; onChange: (value: string) => void }) {
  const [opened, setOpened] = useState(false);
  const selected = locations.find((location) => location.value === value);
  return <><Button variant="default" size="xs" className="location-picker-button" onClick={() => setOpened(true)}>{label}: {selected?.label ?? 'Choose location'}</Button><Modal opened={opened} onClose={() => setOpened(false)} title={label} size="md"><Text size="sm" c="dimmed" mb="sm">Open only the branch you need. A branch with one valid location can be selected directly.</Text><LocationPickerTree locations={locations} onSelect={(next) => { onChange(next); setOpened(false); }} /></Modal></>;
}

function LocationPickerTree({ locations, onSelect }: { locations: PickerLocation[]; onSelect: (value: string) => void }) {
  const root = useMemo(() => {
    const branch: PickerBranch = { children: new Map() };
    for (const location of locations) {
      let current = branch;
      for (const segment of location.label.split(' / ')) {
        if (!current.children.has(segment)) current.children.set(segment, { children: new Map() });
        current = current.children.get(segment)!;
      }
      current.option = location;
    }
    return branch;
  }, [locations]);
  return <Stack gap={2}>{[...root.children.entries()].map(([name, branch]) => <LocationPickerBranch key={name} name={name} branch={branch} onSelect={onSelect} />)}</Stack>;
}

function LocationPickerBranch({ name, branch, onSelect }: { name: string; branch: PickerBranch; onSelect: (value: string) => void }) {
  const [expanded, setExpanded] = useState(false);
  const options = collectPickerOptions(branch);
  const direct = branch.option;
  const canPickBranch = options.length === 1;
  const hasChildren = branch.children.size > 0;
  return <Box className="location-picker-node"><Group gap={4} wrap="nowrap"><ActionIcon size="sm" variant="subtle" color="gray" disabled={!hasChildren} onClick={() => hasChildren && setExpanded((value) => !value)}>{hasChildren && (expanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />)}</ActionIcon><Button variant="subtle" size="xs" justify="flex-start" onClick={() => { if (direct) onSelect(direct.value); else if (canPickBranch) onSelect(options[0].value); else setExpanded(true); }}>{name}{canPickBranch && !direct && <Text component="span" size="xs" c="dimmed"> · one location</Text>}</Button></Group>{expanded && <Stack gap={2} pl={22}>{direct && hasChildren && <Button variant="subtle" size="xs" justify="flex-start" onClick={() => onSelect(direct.value)}>Use this storage</Button>}{[...branch.children.entries()].map(([childName, child]) => <LocationPickerBranch key={childName} name={childName} branch={child} onSelect={onSelect} />)}</Stack>}</Box>;
}

function collectPickerOptions(branch: PickerBranch): PickerLocation[] {
  return [
    ...(branch.option ? [branch.option] : []),
    ...[...branch.children.values()].flatMap(collectPickerOptions),
  ];
}

function AddStockForm({ locations, manufacturers, categories, busy, onSubmit }: { locations: LocationOption[]; manufacturers: string[]; categories: string[]; busy: boolean; onSubmit: (input: Record<string, unknown>) => void }) {
  const form = useForm({
    initialValues: { name: '', manufacturer: '', manufacturerPartNumber: '', category: '', tags: [] as string[], quantity: 1, locationKey: locations[0]?.value ?? '', width: 10, depth: 10, height: 10 },
    validate: (values) => {
      const errors: Record<string, string> = {};
      if (!values.name.trim()) errors.name = 'Name is required';
      if (!values.locationKey) errors.locationKey = 'Choose a location';
      if (Number(values.quantity) <= 0) errors.quantity = 'Quantity must be positive';
      if (locations.find((location) => location.value === values.locationKey)?.spatial) {
        if (Number(values.width) <= 0) errors.width = 'Required';
        if (Number(values.depth) <= 0) errors.depth = 'Required';
        if (Number(values.height) <= 0) errors.height = 'Required';
      }
      return errors;
    },
  });
  useEffect(() => { if (!locations.some((location) => location.value === form.values.locationKey)) form.setFieldValue('locationKey', locations[0]?.value ?? ''); }, [locations[0]?.value]);
  const submit = form.onSubmit((values) => {
    const location = locations.find((option) => option.value === values.locationKey);
    if (!location) return;
    onSubmit({
      name: values.name,
      manufacturer: values.manufacturer,
      manufacturerPartNumber: values.manufacturerPartNumber,
      category: values.category,
      tags: values.tags,
      quantity: Number(values.quantity),
      nodeId: location.nodeId,
      organizerId: location.organizerId,
      compartmentKey: location.compartmentKey,
      size: location.spatial ? { width: cmToMm(Number(values.width)), depth: cmToMm(Number(values.depth)), height: cmToMm(Number(values.height)) } : undefined,
    });
  });
  const selectedLocation = locations.find((location) => location.value === form.values.locationKey);
  return (
    <Paper withBorder className="add-stock-pane" component="form" onSubmit={submit}>
      <Group justify="space-between"><Group gap={7}><PackagePlus size={16} /><Text fw={700} size="sm">Add stock</Text></Group><Text size="xs" c="dimmed">Uses an existing item identity when it matches.</Text></Group>
      <div className="stock-form-grid">
        <TextInput label="Item name" size="xs" withAsterisk {...form.getInputProps('name')} />
        <TextInput label="Part number" size="xs" {...form.getInputProps('manufacturerPartNumber')} />
        <Autocomplete label="Manufacturer" size="xs" data={manufacturers} autoSelectOnBlur onKeyDown={(event) => completeWithTab(event, manufacturers, (value) => form.setFieldValue('manufacturer', value), form.values.manufacturer)} {...form.getInputProps('manufacturer')} />
        <Autocomplete label="Category" size="xs" data={categories} autoSelectOnBlur onKeyDown={(event) => completeWithTab(event, categories, (value) => form.setFieldValue('category', value), form.values.category)} {...form.getInputProps('category')} />
        <TagsInput label="Tags" size="xs" placeholder="Enter tag" {...form.getInputProps('tags')} />
        <NumberInput label="Quantity" size="xs" min={1} clampBehavior="strict" {...form.getInputProps('quantity')} />
        <StorageLocationPicker label="Stock location" value={form.values.locationKey} locations={locations} onChange={(value) => form.setFieldValue('locationKey', value)} />
        {selectedLocation?.spatial && <><NumberInput label="Item width (cm)" size="xs" min={0.1} withAsterisk {...form.getInputProps('width')} /><NumberInput label="Item depth (cm)" size="xs" min={0.1} withAsterisk {...form.getInputProps('depth')} /><NumberInput label="Item height (cm)" size="xs" min={0.1} withAsterisk {...form.getInputProps('height')} /><Text size="xs" c="dimmed">Direct stock is placed in this workspace and can be dragged in Layout.</Text></>}
        <Button type="submit" size="xs" loading={busy} leftSection={<PackagePlus size={14} />}>Add stock</Button>
      </div>
    </Paper>
  );
}

function inventoryLocations(snapshot: Snapshot): LocationOption[] {
  const locations: LocationOption[] = snapshot.nodes
    .filter((node) => node.acceptsInventory && !snapshot.organizers.some((organizer) => organizer.nodeId === node.id && organizer.compartments.length > 0))
    .map((node) => ({ value: `node:${node.id}`, label: node.path, nodeId: node.id, spatial: node.spatialStock }));
  for (const organizer of snapshot.organizers) {
    const node = snapshot.nodes.find((candidate) => candidate.id === organizer.nodeId);
    if (!node) continue;
    for (const compartment of organizer.compartments) {
      locations.push({
        value: `comp:${organizer.id}:${compartment.key}`,
        label: `${node.path} / cell ${compartment.key}`,
        nodeId: organizer.nodeId,
        organizerId: organizer.id,
        compartmentKey: compartment.key,
        spatial: false,
      });
    }
  }
  return locations;
}
