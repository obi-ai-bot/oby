// MIT License
//
// Copyright (c) 2024 Hassan El Mghari
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

// From Nutlope/llamacoder, `lib/preview/html.ts` (`ERROR_BRIDGE`, `waitForTailwindReady`) at commit 5d57b70
// (docs/contracts/artifacts-app.md, ARTIFACTSAPP-22): put into every page of the preview by Sandpack
// (`externalResources`, artifacts/host.ts), served by main's relay at /__csb_oby/bridge.js. It tells the
// host page the page's errors, and once that the page has drawn: its load, its fonts, Tailwind's
// compile and React's first render. Every wait is on timers: rAF is fully suspended in hidden tabs, and
// a card's picture is taken in a view never shown.
(function () {
  var MOST = 4000;
  // Recharts' harmless loop, as LibreChat leaves it out
  var RESIZE = /^ResizeObserver loop (?:limit exceeded|completed with undelivered notifications)\.?$/;
  function post(oby, message) {
    message = String(message);
    if (RESIZE.test(message)) return;
    parent.postMessage({ oby: oby, message: message.slice(0, MOST) }, '*');
  }

  // in the capture phase, so a <script>, <link> or <img> that failed to load (its error doesn't bubble) is heard too
  window.addEventListener('error', function (e) {
    var t = e.target;
    if (t && t !== window && t.tagName) post('error', 'Failed to load ' + t.tagName.toLowerCase() + ' ' + (t.src || t.href || ''));
    else post('error', (e.error && e.error.stack) || e.message);
  }, true);
  window.addEventListener('unhandledrejection', function (e) {
    var r = e.reason;
    post('error', (r && (r.stack || r.message)) || String(r));
  });
  var originalError = console.error;
  console.error = function () {
    var args = Array.prototype.slice.call(arguments);
    post('console-error', args.map(function (arg) {
      if (arg instanceof Error) return arg.stack || arg.message;
      if (typeof arg === 'object') {
        try { return JSON.stringify(arg); } catch (_) { return String(arg); }
      }
      return String(arg);
    }).join(' '));
    return originalError.apply(console, args);
  };

  // React's first render (artifacts/react.ts): a call before the load counts
  var rendered = new Promise(function (done) { window.__obyRendered = function () { done(); }; });
  // Tailwind's browser compile: its probe kept on the page until its classes apply, or 5 s pass
  function tailwindReady() {
    return new Promise(function (done) {
      var startedAt = Date.now();
      var probe = document.createElement('div');
      probe.className = 'grid rounded-md bg-zinc-50 p-6';
      probe.style.position = 'fixed';
      probe.style.left = '-9999px';
      probe.style.top = '0';
      document.body.appendChild(probe);
      (function check() {
        var style = window.getComputedStyle(probe);
        if ((style.display === 'grid' && style.paddingTop === '24px') || Date.now() - startedAt >= 5000) { probe.remove(); done(); } else setTimeout(check, 50);
      })();
    });
  }
  new Promise(function (done) { window.addEventListener('load', function () { setTimeout(done, 0); }); })
    .then(function () { return document.fonts && document.fonts.ready; })
    .then(function () { return document.querySelector('script[src$="tailwindcss-browser/index.global.js"]') && tailwindReady(); })
    .then(function () { return window.__obyReact && rendered; })
    .then(function () {
      var error = document.querySelector('[data-oby-error]');
      if (error) post('error', error.textContent);
      else parent.postMessage({ oby: 'drawn' }, '*');
    });
})();
