// The app's own look, read from apps/electron (read-only), then the browser's proposals on top.
import { createRoot } from 'react-dom/client';
import '@app/tokens.css';
import '@app/app.css';
import '@app/liveTask.css';
import './browser.css';
import './boards.css';
import './frame.css';
import App from './App';

createRoot(document.getElementById('root')!).render(<App />);
