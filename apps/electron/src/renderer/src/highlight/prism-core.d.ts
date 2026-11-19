// Prism's core as a module of its own (highlight/prism.ts): its type is Prism's
declare module 'prismjs/components/prism-core' {
  import type Prism from 'prismjs';
  const core: typeof Prism;
  export default core;
}
