# Making an Oby artifact

Oby keeps what you make for the person you work for, and draws it in a tab of its own. Put the
files in one folder; Oby publishes the folder. This guide is Oby's own, written for any agent.

## Which kind

- **A page:** a folder with an `index.html` (and its CSS, scripts and pictures beside it), or one
  `.html` file.
- **A document:** one `.md` file.
- **A picture or a PDF:** one `.png`, `.jpg`, `.gif`, `.webp`, `.svg` or `.pdf`.
- **A design:** a folder with a `canvas.json` and one `.dc.html` file per board. Use it for
  screens, flows and anything with more than one view side by side. It opens in Claude Design too.

Everything draws with no network: no CDN, no web font, no `fetch`. Use files of the folder.
Limits: 16 MB a text file, 15 MB a binary one, 2 MB an SVG, 512 files and 256 MB in all.

## A design

`canvas.json` says where each board sits:

```json
{
  "v": 3,
  "title": "Sign in",
  "pages": [{ "id": "screens", "name": "Screens" }],
  "boards": {
    "SignIn.dc.html": { "x": 0, "y": 0, "w": 390, "h": 844, "title": "Sign in", "page": "screens", "is_interactive": true },
    "Error.dc.html": { "x": 450, "y": 0, "w": 390, "h": 844, "title": "Wrong password", "page": "screens" }
  },
  "order": ["SignIn.dc.html", "Error.dc.html"]
}
```

- `v` is always 3. Every key of `boards` is a `.dc.html` file of the folder; `w` and `h` are its
  size in pixels. `order` names only boards; a page's `id` is 1 to 40 letters, digits, `_` or `-`.
- `is_interactive: true` gives a board Play. `"expand": "fill"` makes a board a page that fills the
  window and scrolls.

A board is one HTML file:

```html
<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Sign in</title>
<script src="./support.js"></script>
</head>
<body>
<x-dc>
<helmet>
<style>body { margin: 0; font-family: system-ui, sans-serif; }</style>
</helmet>
<div style="width: 390px; height: 844px; padding: 32px; box-sizing: border-box;">
  <h1>{{heading}}</h1>
  <sc-for list="{{fields}}" as="field">
    <label>{{field.label}}<input type="{{field.type}}"></label>
  </sc-for>
  <sc-if value="{{tried}}"><p role="alert">Try again.</p></sc-if>
  <button type="button" onClick="{{signIn}}">Sign in</button>
  <a href="Error.dc.html">See the error state</a>
</div>
</x-dc>
<script type="text/x-dc" data-dc-script data-props='{"$preview":{"width":390,"height":844}}'>
class Component extends DCLogic {
  constructor(props) { super(props); this.state = { tried: false }; }
  renderVals() {
    return {
      heading: 'Welcome back',
      fields: [{ label: 'Email', type: 'email' }, { label: 'Password', type: 'password' }],
      tried: this.state.tried,
      signIn: () => this.setState({ tried: true }),
    };
  }
}
</script>
</body>
</html>
```

- Keep `<script src="./support.js"></script>` as it is: Oby serves its runtime there. Don't add the
  file.
- Inside `<x-dc>` is the template; `<helmet>` holds the board's styles.
- `{{name}}` looks a value up in what `renderVals()` returns (`{{field.label}}` goes deeper). It is
  never an expression: compute in `renderVals()` and name the result.
- `<sc-for list="{{items}}" as="item">` repeats its inside; `{{$index}}` is the position.
  `<sc-if value="{{flag}}">` shows its inside when the value is true.
- Events take a function from `renderVals()`: `onClick="{{signIn}}"`. Never write code in an
  attribute.
- `<dc-import name="Card" title="{{t}}"></dc-import>` draws the sibling board `Card.dc.html` inside
  this one, its attributes as its props (`this.props.title`).
- A link to another board (`href="Error.dc.html"`) plays it in place; `#id` scrolls. Anything else
  is asked of the person before it opens.
- The logic is plain JavaScript, one `class Component extends DCLogic` with React's class lifecycle
  (`state`, `setState`, `componentDidMount`), no `render()`, no imports, no TypeScript.
- `data-props` defaults become the board's props: `{"accent": {"editor": "color", "default": "#0a84ff"}}`
  gives `this.props.accent`.

## Check it before you publish

`oby_artifact_check` draws your folder as Oby will and answers a picture of each board and each
board's mistake by its name. Fix what it names, then publish.
