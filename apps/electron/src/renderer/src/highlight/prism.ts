import './manual';
import type PrismType from 'prismjs';
// Prism's core only: not its bundle, whose plugins fetch files a page names (prism-file-highlight)
import core from 'prismjs/components/prism-core';

// Prism's languages add themselves to a global Prism: this one, wherever it put itself (the window
// in the app, Node's global in a test)
const Prism = core as typeof PrismType;
(globalThis as { Prism?: typeof PrismType }).Prism = Prism;
export default Prism;
