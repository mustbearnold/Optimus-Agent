import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import '@fontsource-variable/dm-sans';
import { App } from './App';
// One entry point for styling. `tailwind.css` imports the three app
// stylesheets into a named cascade layer alongside Tailwind's own — importing
// them here as well would load them a second time, unlayered, where they would
// outrank every utility (ADR-0050).
import './tailwind.css';

const el = document.getElementById('root');
if (!el) throw new Error('root missing');
createRoot(el).render(
  <StrictMode>
    <App />
  </StrictMode>
);
