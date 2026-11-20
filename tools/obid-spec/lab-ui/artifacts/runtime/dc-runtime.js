/*
 * Oby's runtime of the .dc.html format (Claude Design's boards), Apache-2.0.
 *
 * Written from the format's description alone (holes, sc-for, sc-if, dc-import, x-import, helmet,
 * links between boards, `class Component extends DCLogic`, renderVals), never from another
 * runtime's code. It runs on React 18.3.1 (vendor/, MIT), with no Babel and nothing from a CDN:
 * every file it needs is served beside the board, so obid's artifact CSP (`script-src 'self'`,
 * `connect-src 'none'`) holds.
 *
 * What the server gives it, beside the board (src/lab/artifactview.rs):
 *   support.js          this file, after React and ReactDOM;
 *   __dc/boards.js      every board of the version: its template, its helmet, its logic's file;
 *   __dc/logic/<b>.js   each board's logic, `class Component extends DCLogic`, as a script of its
 *                       own, so one board's mistake never stops another.
 *
 * What it tells whoever shows it: `window.__OBY_DC` (ready, errors, the board playing, the links
 * that asked to open outside) and, inside a frame, the same as `postMessage({oby: …})` to the
 * parent. A link never navigates the page: a board's link plays that board in place, `#id`
 * scrolls, anything else asks the app (`oby: "open-outside"`), which confirms before it opens.
 */
(function () {
  'use strict';
  var React = window.React;
  var ReactDOM = window.ReactDOM;
  var OBY = (window.__OBY_DC = window.__OBY_DC || {});
  OBY.runtime = 'oby-dc 1';
  OBY.errors = OBY.errors || [];
  OBY.asked = OBY.asked || [];
  OBY.ready = false;
  OBY.logic = OBY.logic || {};
  OBY.boards = OBY.boards || {};

  // The raw template stays hidden until it is drawn.
  try {
    var hide = document.createElement('style');
    hide.textContent = 'x-dc{display:none!important}';
    (document.head || document.documentElement).appendChild(hide);
  } catch (_) {}

  function tell(message) {
    try {
      if (window.parent && window.parent !== window) window.parent.postMessage(message, '*');
    } catch (_) {}
  }

  /** A mistake in words an agent can act on: its kind and what it says (`SyntaxError: …`). */
  function report(board, error) {
    var message =
      error && typeof error === 'object' && error.message
        ? (error.name && error.name !== 'Error' ? error.name + ': ' : '') + error.message
        : String(error);
    OBY.errors.push({ board: board, message: message });
    try {
      console.error('[oby-dc] ' + board + ': ' + message);
    } catch (_) {}
    tell({ oby: 'error', board: board, message: message });
  }

  // A script's mistake (a logic file that doesn't parse, a handler that throws), told by its board.
  window.addEventListener('error', function (e) {
    var file = String(e.filename || '');
    var at = file.indexOf('/__dc/logic/');
    var board = at >= 0 ? decodeURIComponent(file.slice(at + 12).replace(/\.js$/, '')) : OBY.playing || '?';
    report(board, e.error || e.message);
  });

  // ── Holes ───────────────────────────────────────────────────────────────────────────────────

  var HOLE = /\{\{\s*([^{}]+?)\s*\}\}/g;
  var WHOLE = /^\s*\{\{\s*([^{}]+?)\s*\}\}\s*$/;

  /** `{{ a.b }}` is a dotted lookup in scope; `true`, `false`, `null`, numbers and quoted words are
   *  themselves. Anything else (an expression) is nothing, as the format says. */
  function lookup(path, scope) {
    var p = path.trim();
    if (p === 'true') return true;
    if (p === 'false') return false;
    if (p === 'null') return null;
    if (p === 'undefined') return undefined;
    if (/^-?\d+(\.\d+)?$/.test(p)) return Number(p);
    var quoted = /^'([^']*)'$|^"([^"]*)"$/.exec(p);
    if (quoted) return quoted[1] !== undefined ? quoted[1] : quoted[2];
    if (!/^[A-Za-z_$][\w$]*(\.[\w$]+)*$/.test(p)) return undefined;
    var parts = p.split('.');
    var value = scope[parts[0]];
    for (var i = 1; i < parts.length && value != null; i++) value = value[parts[i]];
    return value;
  }

  function text(value) {
    return value == null || typeof value === 'function' || typeof value === 'object' ? '' : String(value);
  }

  function interpolate(raw, scope) {
    return raw.replace(HOLE, function (_, p) {
      return text(lookup(p, scope));
    });
  }

  function attrValue(raw, scope) {
    var whole = WHOLE.exec(raw);
    if (whole) return lookup(whole[1], scope);
    return raw.indexOf('{{') < 0 ? raw : interpolate(raw, scope);
  }

  // ── Attributes to React's props ─────────────────────────────────────────────────────────────

  var RENAME = {
    class: 'className', for: 'htmlFor', tabindex: 'tabIndex', readonly: 'readOnly',
    maxlength: 'maxLength', minlength: 'minLength', colspan: 'colSpan', rowspan: 'rowSpan',
    contenteditable: 'contentEditable', crossorigin: 'crossOrigin', autocomplete: 'autoComplete',
    autofocus: 'autoFocus', spellcheck: 'spellCheck', srcset: 'srcSet', inputmode: 'inputMode',
    enterkeyhint: 'enterKeyHint', playsinline: 'playsInline', 'accept-charset': 'acceptCharset',
    'http-equiv': 'httpEquiv', datetime: 'dateTime', allowfullscreen: 'allowFullScreen',
    usemap: 'useMap', novalidate: 'noValidate', cellpadding: 'cellPadding', cellspacing: 'cellSpacing',
    frameborder: 'frameBorder', referrerpolicy: 'referrerPolicy',
  };
  var BOOLEAN = /^(disabled|checked|selected|hidden|readonly|required|multiple|autofocus|open|muted|autoplay|loop|controls|playsinline|novalidate|allowfullscreen|reversed)$/;
  var EVENTS = {};
  [
    'Click', 'DoubleClick:dblclick', 'Change', 'Input', 'Submit', 'KeyDown', 'KeyUp', 'KeyPress',
    'Focus', 'Blur', 'MouseEnter', 'MouseLeave', 'MouseOver', 'MouseOut', 'MouseDown', 'MouseUp',
    'MouseMove', 'PointerDown', 'PointerUp', 'PointerMove', 'PointerEnter', 'PointerLeave',
    'Wheel', 'Scroll', 'TouchStart', 'TouchEnd', 'TouchMove', 'ContextMenu', 'DragStart',
    'DragOver', 'Drop', 'DragEnd', 'Load', 'Error', 'AnimationEnd', 'TransitionEnd',
  ].forEach(function (e) {
    var named = e.split(':');
    EVENTS['on' + (named[1] || named[0].toLowerCase())] = 'on' + named[0];
  });

  function camel(name) {
    return name.replace(/-([a-z])/g, function (_, c) {
      return c.toUpperCase();
    });
  }

  /** A `style` attribute's text as React's object: custom properties kept, the rest camelCase. */
  function styleObject(css) {
    var out = {};
    var depth = 0;
    var quote = '';
    var start = 0;
    function take(decl) {
      var colon = decl.indexOf(':');
      if (colon < 0) return;
      var prop = decl.slice(0, colon).trim();
      var value = decl.slice(colon + 1).trim();
      if (!prop || !value) return;
      if (prop.indexOf('--') === 0) out[prop] = value;
      else out[camel(prop.toLowerCase().replace(/^-ms-/, 'ms-'))] = value;
    }
    for (var i = 0; i < css.length; i++) {
      var ch = css[i];
      if (quote) {
        if (ch === quote) quote = '';
      } else if (ch === '"' || ch === "'") quote = ch;
      else if (ch === '(') depth++;
      else if (ch === ')') depth--;
      else if (ch === ';' && depth <= 0) {
        take(css.slice(start, i));
        start = i + 1;
      }
    }
    take(css.slice(start));
    return out;
  }

  // ── Templates ───────────────────────────────────────────────────────────────────────────────

  var parsed = {};

  /** A board's template as a tree, parsed once by the browser's own HTML parser. */
  function tree(name) {
    if (parsed[name]) return parsed[name];
    var board = OBY.boards[name];
    var t = document.createElement('template');
    t.innerHTML = board ? board.template : '';
    parsed[name] = Array.prototype.slice.call(t.content.childNodes);
    return parsed[name];
  }

  function folderOf(board) {
    var i = board.lastIndexOf('/');
    return i < 0 ? '' : board.slice(0, i + 1);
  }

  /** A link's target among the version's files: relative to the board, or to the root with `/`. */
  function boardPath(from, href) {
    var path = href.split('#')[0].split('?')[0];
    var parts = path.charAt(0) === '/' ? [] : folderOf(from).split('/').filter(Boolean);
    path.split('/').forEach(function (p) {
      if (p === '..') parts.pop();
      else if (p && p !== '.') parts.push(p);
    });
    return parts.join('/');
  }

  function linkClick(board, href, own) {
    return function (e) {
      if (typeof own === 'function') own(e);
      e.preventDefault();
      if (!href) return;
      if (href.charAt(0) === '#') {
        var el = document.getElementById(href.slice(1));
        if (el) el.scrollIntoView();
        return;
      }
      if (!/^[a-z][a-z0-9+.-]*:/i.test(href) && /\.dc\.html$/i.test(href.split('#')[0])) {
        OBY.play(boardPath(board, href));
        return;
      }
      OBY.askOutside(href);
    };
  }

  var hasOwn = Object.prototype.hasOwnProperty;

  function propsOf(el, scope, board) {
    var props = {};
    var tag = el.localName;
    for (var i = 0; i < el.attributes.length; i++) {
      var a = el.attributes[i];
      var name = a.name;
      var value = attrValue(a.value, scope);
      if (name.indexOf('hint-') === 0) continue;
      if (EVENTS[name]) {
        // A handler is a function the logic gave; text is never run as code.
        if (typeof value === 'function') props[EVENTS[name]] = value;
        continue;
      }
      if (name === 'style') {
        props.style = typeof value === 'string' ? styleObject(value) : value;
        continue;
      }
      if (BOOLEAN.test(name) && a.value.indexOf('{{') < 0) value = true;
      if (name === 'value' && a.value.indexOf('{{') < 0 && /^(input|textarea|select)$/.test(tag)) name = 'defaultValue';
      if (name === 'checked' && a.value.indexOf('{{') < 0) name = 'defaultChecked';
      props[hasOwn.call(RENAME, name) ? RENAME[name] : name] = value;
    }
    if (tag === 'a' && typeof props.href === 'string') props.onClick = linkClick(board, props.href, props.onClick);
    if (tag === 'form') {
      var submit = props.onSubmit;
      props.onSubmit = function (e) {
        e.preventDefault();
        if (typeof submit === 'function') submit(e);
      };
    }
    return props;
  }

  /** The props a dc-import or x-import passes: every attribute but its own, kebab to camel. */
  function passedProps(el, scope, skip) {
    var props = {};
    for (var i = 0; i < el.attributes.length; i++) {
      var a = el.attributes[i];
      if (a.name === skip || a.name.indexOf('hint-') === 0) continue;
      props[camel(a.name)] = attrValue(a.value, scope);
    }
    return props;
  }

  function nodes(list, scope, board, keyBase) {
    var out = [];
    for (var i = 0; i < list.length; i++) {
      var n = draw(list[i], scope, board, keyBase + '.' + i);
      if (n !== null && n !== undefined) out.push(n);
    }
    return out;
  }

  function draw(node, scope, board, key) {
    if (node.nodeType === 3) return node.data.indexOf('{{') < 0 ? node.data : interpolate(node.data, scope);
    if (node.nodeType !== 1) return null;
    var tag = node.localName;
    var kids = function (s) {
      return nodes(node.childNodes, s || scope, board, key);
    };
    if (tag === 'helmet' || tag === 'script') return null;
    if (tag === 'sc-for') {
      var list = attrValue(node.getAttribute('list') || '', scope);
      var as = node.getAttribute('as') || 'item';
      if (!list || typeof list.length !== 'number') return null;
      var each = [];
      for (var j = 0; j < list.length; j++) {
        var inner = Object.create(scope);
        inner[as] = list[j];
        inner.$index = j;
        each.push(React.createElement(React.Fragment, { key: key + ':' + j }, nodes(node.childNodes, inner, board, key + ':' + j)));
      }
      return React.createElement(React.Fragment, { key: key }, each);
    }
    if (tag === 'sc-if') {
      var shown = attrValue(node.getAttribute('value') || '', scope);
      return shown ? React.createElement(React.Fragment, { key: key }, kids()) : null;
    }
    if (tag === 'dc-import') {
      var target = boardPath(board, (node.getAttribute('name') || '') + '.dc.html');
      var props = Object.assign({}, defaultsOf(target), passedProps(node, scope, 'name'));
      props.key = key;
      return React.createElement(Board, { key: key, name: target, props: props });
    }
    if (tag === 'x-import') {
      var where = node.getAttribute('component-from-global-scope') || '';
      var comp = where.split('.').reduce(function (o, k) {
        return o == null ? o : o[k];
      }, window);
      if (typeof comp !== 'function' && !(comp && comp.$$typeof)) {
        report(board, where + ' is no component the design system defines');
        return null;
      }
      var xp = passedProps(node, scope, 'component-from-global-scope');
      xp.key = key;
      return React.createElement(comp, xp, node.childNodes.length ? kids() : undefined);
    }
    if (tag.indexOf('sc-') === 0) return React.createElement(React.Fragment, { key: key }, kids());
    var p = propsOf(node, scope, board);
    p.key = key;
    var children = tag === 'template' ? [] : kids();
    return React.createElement(tag, p, children.length ? children : undefined);
  }

  // ── Boards ──────────────────────────────────────────────────────────────────────────────────

  /** A board's tweaks as its `data-props` default them: Oby v1 shows them, and doesn't edit them. */
  function defaultsOf(name) {
    var b = OBY.boards[name];
    return Object.assign({}, (b && b.defaults) || {});
  }

  function DCLogic(props) {
    React.Component.call(this, props);
    this.state = {};
  }
  DCLogic.prototype = Object.create(React.Component.prototype);
  DCLogic.prototype.constructor = DCLogic;
  DCLogic.prototype.isReactComponent = {};
  DCLogic.prototype.renderVals = function () {
    return {};
  };
  DCLogic.prototype.render = function () {
    var name = this.constructor.__obyBoard;
    var vals = this.renderVals() || {};
    var scope = Object.create(null);
    Object.keys(vals).forEach(function (k) {
      scope[k] = vals[k];
    });
    return React.createElement(React.Fragment, null, nodes(tree(name), scope, name, name));
  };
  window.DCLogic = DCLogic;

  var classes = {};

  /** A board's component: its logic's class, or a plain one when it has none or it failed. */
  function componentFor(name) {
    if (classes[name]) return classes[name];
    var made = null;
    var make = OBY.logic[name];
    if (typeof make === 'function') {
      try {
        made = make(DCLogic, React);
      } catch (e) {
        report(name, e);
      }
    }
    if (typeof made !== 'function') {
      if (OBY.boards[name] && OBY.boards[name].logic && typeof make !== 'function') report(name, 'its logic did not load');
      made = function Plain(props) {
        DCLogic.call(this, props);
      };
      made.prototype = Object.create(DCLogic.prototype);
      made.prototype.constructor = made;
    } else if (!(made.prototype instanceof DCLogic)) {
      report(name, 'its Component does not extend DCLogic');
    }
    made.__obyBoard = name;
    helmet(name);
    classes[name] = made;
    return made;
  }

  var helmets = {};

  /** A board's <helmet> (its styles, its fonts) into the document's head, once. */
  function helmet(name) {
    if (helmets[name] || !OBY.boards[name] || !OBY.boards[name].helmet) return;
    helmets[name] = true;
    var t = document.createElement('template');
    t.innerHTML = OBY.boards[name].helmet;
    Array.prototype.slice.call(t.content.childNodes).forEach(function (n) {
      if (n.nodeType === 1 && n.localName !== 'script') document.head.appendChild(document.importNode(n, true));
    });
  }

  /** One board, its mistakes caught and told by its name: the others still draw. */
  function Board(props) {
    React.Component.call(this, props);
    this.state = { failed: null };
  }
  Board.prototype = Object.create(React.Component.prototype);
  Board.prototype.constructor = Board;
  Board.prototype.isReactComponent = {};
  Board.getDerivedStateFromError = function (e) {
    return { failed: String((e && e.message) || e) };
  };
  Board.prototype.componentDidCatch = function (e) {
    report(this.props.name, e);
  };
  Board.prototype.render = function () {
    if (this.state.failed !== null) return null;
    if (!OBY.boards[this.props.name]) {
      report(this.props.name, 'there is no such board');
      return null;
    }
    return React.createElement(componentFor(this.props.name), this.props.props || {});
  };

  function Root(props) {
    React.Component.call(this, props);
  }
  Root.prototype = Object.create(React.Component.prototype);
  Root.prototype.constructor = Root;
  Root.prototype.isReactComponent = {};
  Root.prototype.componentDidMount = function () {
    settled(this.props.name);
  };
  Root.prototype.componentDidUpdate = function () {
    settled(this.props.name);
  };
  Root.prototype.render = function () {
    return React.createElement(Board, { key: this.props.name, name: this.props.name, props: defaultsOf(this.props.name) });
  };

  function settled(name) {
    var done = function () {
      var height = Math.max(document.documentElement.scrollHeight, document.body ? document.body.scrollHeight : 0);
      var width = Math.max(document.documentElement.scrollWidth, document.body ? document.body.scrollWidth : 0);
      OBY.ready = true;
      OBY.size = { width: width, height: height };
      document.documentElement.setAttribute('data-oby-dc', 'ready');
      tell({ oby: 'ready', board: name, width: width, height: height, errors: OBY.errors.slice() });
    };
    // A timer, not animation frames: Chrome holds those back in a sandboxed frame off screen.
    var frames = function () {
      setTimeout(done, 30);
    };
    if (document.fonts && document.fonts.ready) document.fonts.ready.then(frames, frames);
    else frames();
  }

  var root = null;

  OBY.play = function (name) {
    if (!OBY.boards[name]) {
      report(name, 'there is no such board');
      return;
    }
    OBY.ready = false;
    OBY.playing = name;
    tell({ oby: 'play', board: name });
    loadLogic(needed(name), function () {
      root.render(React.createElement(Root, { name: name }));
    });
  };

  /** A link out of the design. In a frame, the canvas around it asks; alone, the page tries to go
   *  there, which the app never lets happen: it stops the navigation and asks the person whether to
   *  open the address outside Oby (src/lab/artifactview.rs, `navigation`). */
  OBY.askOutside = function (href) {
    OBY.asked.push(href);
    if (window.parent && window.parent !== window) tell({ oby: 'open-outside', url: href });
    else
      try {
        location.href = href;
      } catch (_) {}
  };

  /** The boards a board draws: itself and every board it imports, however deep. */
  function needed(name) {
    var seen = {};
    var todo = [name];
    var IMPORT = /<dc-import\b[^>]*\bname\s*=\s*["']([^"']+)["']/gi;
    while (todo.length) {
      var n = todo.pop();
      if (seen[n] || !OBY.boards[n]) continue;
      seen[n] = true;
      var m;
      IMPORT.lastIndex = 0;
      while ((m = IMPORT.exec(OBY.boards[n].template))) todo.push(boardPath(n, m[1] + '.dc.html'));
    }
    return Object.keys(seen);
  }

  var loaded = {};

  /** Each board's logic, once, only for the boards drawn: another board's mistake is never this
   *  one's. Loaded with CORS, so a mistake in it keeps its file's name in a sandboxed page. */
  function loadLogic(names, then) {
    var wanted = names.filter(function (n) {
      return OBY.boards[n] && OBY.boards[n].logic && !loaded[n];
    });
    var left = wanted.length;
    if (!left) return then();
    wanted.forEach(function (n) {
      loaded[n] = true;
      var s = document.createElement('script');
      s.crossOrigin = 'anonymous';
      s.src = '/__dc/logic/' + encodeURIComponent(n) + '.js';
      s.onload = s.onerror = function () {
        if (--left === 0) then();
      };
      document.head.appendChild(s);
    });
  }

  function boot() {
    var name = decodeURIComponent(location.pathname.replace(/^\/+/, ''));
    var xdc = document.querySelector('x-dc');
    var mount = document.createElement('div');
    mount.setAttribute('data-oby-dc-root', '');
    if (xdc && xdc.parentNode) xdc.parentNode.replaceChild(mount, xdc);
    else document.body.appendChild(mount);
    if (!OBY.boards[name]) report(name, 'this page is no board of the design');
    root = ReactDOM.createRoot(mount);
    OBY.play(name);
  }

  function start() {
    var s = document.createElement('script');
    s.crossOrigin = 'anonymous';
    s.src = '/__dc/boards.js';
    s.onload = boot;
    s.onerror = function () {
      report('?', 'the boards did not load');
    };
    document.head.appendChild(s);
  }

  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', start);
  else start();
})();
