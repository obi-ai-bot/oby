// A conversation's title, as obid names it (core/crates/obid/src/conversation.rs, `titled` and
// `short_title`): kept the same here for the mock, and for the row a new conversation shows before
// obid has listed it, so its title doesn't change under the owner's eyes (docs/contracts/chat-first.md,
// CHATFIRST-36). obid's is the one that's kept.

// the first line, at most 48 characters (45 and … when longer), its first letter upper-case
export function titled(text: string) {
  const line = (text.trim().split(/[\n\r\u2028\u2029]/)[0] ?? '').replace(/\p{Cc}/gu, ' ').trim();
  const chars = [...line];
  const t = chars.length > 48 ? `${chars.slice(0, 45).join('').trimEnd()}…` : line;
  return t ? t[0].toUpperCase() + t.slice(1) : 'New conversation';
}

// what a first message often opens with that says nothing of what it's about
const OPENERS = [
  'hey oby', 'hi oby', 'hello oby', 'oby', 'hey', 'hi', 'hello', 'ok', 'okay', 'so', 'and', 'also',
  'please', 'pls', 'plz', 'can you', 'could you', 'would you', 'will you', 'can u', 'could u',
  'can we', 'could we', 'i want you to', "i'd like you to", 'i would like you to', 'i need you to',
  'i want to', "i'd like to", 'i would like to', 'i need to', 'we need to', 'need to', "let's",
  'lets', 'let us', 'help me', 'go ahead and', 'quickly', 'take a look at', 'have a look at',
  'look at', 'look into', 'a', 'an', 'the',
].map((o) => o.split(' '));
// words a title cut short shouldn't end on
const DANGLING = new Set(['a', 'an', 'the', 'and', 'or', 'but', 'to', 'of', 'in', 'on', 'for', 'with', 'at', 'from', 'by',
  'is', 'are', 'that', 'this', 'my', 'our', 'your', 'its', 'please', 'so']);
const TITLE_WORDS = 6;
const bare = (w: string) => w.replace(/^[^\p{L}\p{N}'’]+|[^\p{L}\p{N}'’]+$/gu, '').replaceAll('’', "'").toLowerCase();
const ENDS = /[.?!;,]$/;

// A new conversation's title from the owner's first words: a few words, by rule, never a model call
export function shortTitle(text: string) {
  const line = (text.trim().split(/[\n\r\u2028\u2029]/)[0] ?? '').replace(/\p{Cc}/gu, ' ');
  let words = line.split(/\s+/).filter(Boolean);
  for (;;) {
    const o = OPENERS.find((o) => words.length > o.length && o.every((x, i) => bare(words[i]) === x));
    if (!o) break;
    words = words.slice(o.length);
  }
  const end = words.findIndex((w) => ENDS.test(w));
  if (end >= 0) words = words.slice(0, end + 1);
  const cut = words.length > TITLE_WORDS;
  words = words.slice(0, TITLE_WORDS);
  if (words.length) words[words.length - 1] = words[words.length - 1].replace(/[.?!;,:]+$/, '');
  while (cut && words.length > 1 && DANGLING.has(bare(words[words.length - 1]))) words.pop();
  const title = words.join(' ');
  return title.trim() ? titled(title) : titled(text);
}
