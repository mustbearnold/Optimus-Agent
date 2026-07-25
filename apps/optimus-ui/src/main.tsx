import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import '@fontsource-variable/dm-sans';
import { App } from './App';
import './styles.css';
import './codex-shell.css';
import './workbench-shell.css';

const el = document.getElementById('root');
if (!el) throw new Error('root missing');
createRoot(el).render(
  <StrictMode>
    <App />
  </StrictMode>
);
