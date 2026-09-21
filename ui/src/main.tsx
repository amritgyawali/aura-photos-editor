import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';

import { startUiAudit } from './audit/log';
import { App } from './App';
import './styles.css';

// Before React renders: a failure during the very first paint should already have
// somewhere to be written.
startUiAudit();

const container = document.getElementById('root');
if (container) {
  createRoot(container).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
}
