import { Component, type ErrorInfo, type ReactNode } from 'react';

// A render error anywhere in the window (one message's own is caught by Said.tsx's Safe): main
// is told its name, its message and the components it went through, nothing else (no props, no
// state, no stack), keeps it as a crash line and reloads the window once (docs/contracts/engine.md,
// ENGINE-20). Until then the page shows nothing.
export default class RenderError extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() { return { failed: true }; }
  componentDidCatch(error: unknown, info: ErrorInfo) {
    let name: string = typeof error, message = '';
    try {
      if (error instanceof Error) { name = String(error.name); message = String(error.message); }
      else message = String(error);
    } catch { /* a value that can't be read is sent as what was read of it */ }
    window.app?.renderError?.(name, message, info.componentStack ?? '');
  }
  render() { return this.state.failed ? null : this.props.children; }
}
