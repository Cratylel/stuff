import '@mantine/core/styles.css';
import './styles.css';

import { createTheme, MantineProvider } from '@mantine/core';
import { createRoot } from 'react-dom/client';
import { App } from './App';

const theme = createTheme({
  fontFamily: 'Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
  primaryColor: 'cyan',
  defaultRadius: 'sm',
  cursorType: 'pointer',
});

createRoot(document.getElementById('root')!).render(
  <MantineProvider theme={theme} defaultColorScheme="dark">
    <App />
  </MantineProvider>,
);
