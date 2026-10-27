import { useLayoutEffect, useRef, useState } from 'react';
import { createRoot } from 'react-dom/client';
import './tokens.css';
import './app.css';
import Home from './Home';
import Welcome from './Welcome';
import Splash from './Splash';
import { everything, onObid } from './lean';
import { connect, useLive } from './live';

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
  // on obid: the splash until it answers and its status and conversations are in, then the chat
  // fades in (app.css, .is-revealed)
  const { status } = useLive();
  const ready = !onObid || !!status;
  const splashed = useRef(false);
  if (!ready) splashed.current = true;
  useLayoutEffect(() => { if (ready && splashed.current) document.getElementById('root')!.classList.add('is-revealed'); }, [ready]);
  if (!ready) return <Splash />;
  return welcome ? <Welcome onDone={done} /> : <Home />;
}

if (onObid) connect();
createRoot(document.getElementById('root')!).render(<App />);
// dev builds only: the control center (dev/ControlCenter.tsx), left out of a release build
if (import.meta.env.DEV) import('./dev/ControlCenter');
