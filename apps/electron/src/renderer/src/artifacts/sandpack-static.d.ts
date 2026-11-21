// Sandpack's static client alone, as vite.config.ts aliases it (its chunk, not the whole client)
declare module 'oby-sandpack-static' {
  export { SandpackStatic } from '@codesandbox/sandpack-client/clients/static';
}
