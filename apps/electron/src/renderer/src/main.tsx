import { useState } from 'react';
import { createRoot } from 'react-dom/client';
import './tokens.css';
import './app.css';
import Home from './Home';
import Welcome from './Welcome';
import { everything } from './lean';

// Line the title bar up with the traffic lights, where macOS actually put them
const chrome = window.app?.chrome;
if (chrome) {
  const root = document.documentElement.style;
  root.setProperty('--bar', `${Math.round(chrome.lightsCenterY * 2)}px`);
  root.setProperty('--after-lights', `${Math.round(chrome.lightsRight + 10)}px`);
}

// the first launch shows Welcome once; ?welcome shows it again. Lean (lean.ts):
// no scenes, Oby sets you up in its first conversation (onboarding.ts)
function App() {
  const [welcome, setWelcome] = useState(() => {
    if (!everything) return false;
    if (new URLSearchParams(location.search).has('welcome')) return true;
    try { return !localStorage.getItem('oby.welcomed'); } catch { return true; }
  });
  const done = () => {
    try { localStorage.setItem('oby.welcomed', '1'); } catch { /* shown again next time, that's all */ }
    setWelcome(false);
  };
  return welcome ? <Welcome onDone={done} /> : <Home />;
}

createRoot(document.getElementById('root')!).render(<App />);
