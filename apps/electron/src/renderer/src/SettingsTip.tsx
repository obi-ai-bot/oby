import { useEffect, useState, useSyncExternalStore } from 'react';
import { X } from 'lucide-react';

// Over Home's message box, from the first landing: where more tools and the default model
// are. Gone for good once Settings has been opened (from here or the gear), it's dismissed, or
// it has been up 10 minutes (counted from when it first showed, across launches: it fades out)
const KEY = 'oby.settingsTip';
const SINCE = 'oby.settingsTipSince';
const FOR = 10 * 60_000;
const OUT_MS = 400;  // its fade (app.css, .settings-tip.is-leaving)
const seen = () => { try { return localStorage.getItem(KEY) === 'done'; } catch { return false; } };
let done = seen();
const listeners = new Set<() => void>();
const subscribe = (l: () => void) => { listeners.add(l); return () => listeners.delete(l); };
export const dismissSettingsTip = () => {
  if (done) return;
  done = true;
  try { localStorage.setItem(KEY, 'done'); } catch { /* it shows again next launch, that's all */ }
  listeners.forEach((l) => l());
};
// setting up again (a dev build, every launch): back for the landing after
export const resetSettingsTip = () => {
  try { localStorage.removeItem(KEY); localStorage.removeItem(SINCE); } catch { /* fine */ }
  done = false;
  listeners.forEach((l) => l());
};

// when it first showed: now, the first time
const since = () => {
  try {
    const at = Number(localStorage.getItem(SINCE));
    if (at) return at;
    localStorage.setItem(SINCE, String(Date.now()));
  } catch { /* counted from now, this launch */ }
  return Date.now();
};

export default function SettingsTip({ onOpen }: { onOpen: () => void }) {
  const gone = useSyncExternalStore(subscribe, () => done);
  const [leaving, setLeaving] = useState(false);
  // up its 10 minutes already (a launch since): gone without showing
  const [over] = useState(() => !done && since() + FOR <= Date.now());
  useEffect(() => { if (over) dismissSettingsTip(); }, [over]);
  // its 10 minutes up while it shows: it fades out, then it's gone for good
  useEffect(() => {
    if (gone || over) return;
    let t = window.setTimeout(() => {
      setLeaving(true);
      t = window.setTimeout(dismissSettingsTip, OUT_MS);
    }, Math.max(0, since() + FOR - Date.now()));
    return () => clearTimeout(t);
  }, [gone, over]);
  if (gone || over) return null;
  // the whole bar opens Settings; the × at its end closes it
  return (
    <div className={`settings-tip${leaving ? ' is-leaving' : ''}`}>
      <button type="button" className="settings-tip-open" onClick={onOpen}>
        Add more tools or set your default model in Settings
      </button>
      <button type="button" className="settings-tip-close" aria-label="Dismiss" onClick={dismissSettingsTip}><X size={16} strokeWidth={1.6} /></button>
    </div>
  );
}
