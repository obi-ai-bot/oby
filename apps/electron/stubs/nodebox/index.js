// Oby's stand-in for @codesandbox/nodebox (README.md): Sandpack's `node` client, which Oby never uses.
class Nodebox { constructor() { throw new Error('Nodebox is not part of Oby.'); } }
module.exports = { Nodebox, OBY_NODEBOX_STUB: true };
