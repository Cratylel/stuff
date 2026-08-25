import { AppShell, Badge, Box, Center, Group, Loader, Tabs, Text, Tooltip } from '@mantine/core';
import { listen } from '@tauri-apps/api/event';
import { Boxes, LayoutPanelTop, PackageSearch, Shapes } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { getSnapshot } from './api';
import { InventoryView } from './InventoryView';
import { LayoutView } from './LayoutView';
import { TypesView } from './TypesView';
import type { MainTab, Snapshot } from './types';

export function App() {
  const [tab, setTab] = useState<MainTab>('inventory');
  const [typeTab, setTypeTab] = useState<'organizers' | 'racks'>('organizers');
  const [snapshot, setSnapshot] = useState<Snapshot>();
  const [layoutFocusId, setLayoutFocusId] = useState<string>();
  const [error, setError] = useState<string>();

  const refresh = useCallback(async () => {
    try {
      setSnapshot(await getSnapshot());
      setError(undefined);
    } catch (reason) {
      setError(String(reason));
    }
  }, []);

  useEffect(() => {
    void refresh();
    let unlisten: (() => void) | undefined;
    void listen('stuff://changed', () => void refresh()).then((dispose) => {
      unlisten = dispose;
    });
    return () => unlisten?.();
  }, [refresh]);

  useEffect(() => {
    const suppressBrowserMenu = (event: MouseEvent) => event.preventDefault();
    window.addEventListener('contextmenu', suppressBrowserMenu);
    return () => window.removeEventListener('contextmenu', suppressBrowserMenu);
  }, []);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (!(event.ctrlKey || event.metaKey)) return;
      if (event.key === '1') setTab('inventory');
      if (event.key === '2') setTab('layout');
      if (event.key === '3') setTab('types');
      if (event.key.toLowerCase() === 'r') void refresh();
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [refresh]);

  if (!snapshot) {
    return (
      <Center h="100vh" className="boot-screen">
        <Group gap="xs"><Loader size="sm" /><Text c="dimmed">Opening canonical inventory…</Text></Group>
      </Center>
    );
  }

  const openLayout = (nodeId: string) => {
    setLayoutFocusId(nodeId);
    setTab('layout');
  };

  return (
    <AppShell header={{ height: 52 }} padding={0} className="stuff-shell">
      <AppShell.Header className="product-header" data-tauri-drag-region>
        <Group h="100%" px="md" gap="lg" wrap="nowrap">
          <Group gap={8} className="brand" data-tauri-drag-region>
            <Box className="brand-mark"><Boxes size={17} /></Box>
            <Text fw={700} size="sm" tt="uppercase" lts={1.2}>Stuff</Text>
          </Group>
          <Tabs value={tab} onChange={(value) => value && setTab(value as MainTab)} variant="pills" className="main-tabs">
            <Tabs.List>
              <Tabs.Tab value="inventory" leftSection={<PackageSearch size={15} />}>Inventory</Tabs.Tab>
              <Tabs.Tab value="layout" leftSection={<LayoutPanelTop size={15} />}>Layout</Tabs.Tab>
              <Tabs.Tab value="types" leftSection={<Shapes size={15} />}>Types</Tabs.Tab>
            </Tabs.List>
          </Tabs>
          <Box style={{ flex: 1 }} />
          <Tooltip label={snapshot.backend.connectionNote ?? 'Connected to STUFF_DATABASE_URL'}>
            <Badge variant="light" color={snapshot.backend.mode === 'live' ? 'teal' : 'yellow'} size="sm">
              {snapshot.backend.mode === 'live' ? 'LIVE' : 'DEMO'}
            </Badge>
          </Tooltip>
          <Text size="xs" c="dimmed" visibleFrom="lg">⌘/Ctrl 1–3 switch tabs · ⌘/Ctrl R refresh</Text>
        </Group>
      </AppShell.Header>
      <AppShell.Main className="app-main">
        {error && <Box className="connection-error">{error}</Box>}
        {tab === 'inventory' && <InventoryView snapshot={snapshot} onSnapshot={setSnapshot} />}
        {tab === 'layout' && <LayoutView snapshot={snapshot} focusNodeId={layoutFocusId} onSnapshot={setSnapshot} onOpenTypes={() => setTab('types')} />}
        {tab === 'types' && <TypesView snapshot={snapshot} activeTab={typeTab} onTabChange={setTypeTab} onSnapshot={setSnapshot} onOpenLayout={openLayout} />}
      </AppShell.Main>
    </AppShell>
  );
}
