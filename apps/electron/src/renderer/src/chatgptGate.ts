// Continue with ChatGPT's gate (docs/contracts/chatgpt-sign-in.md, "The gate"): the one thing that
// opens a new ChatGPT sign-in in a release. Its real sign-in has never run against OpenAI, so this
// stays '' until it has. Once "One real sign-in" has really run on a real account, set it to the link
// of the comment on #1879 that reports it. scripts/package.sh reads this line as it stands, and asks
// GitHub whether that comment exists before it builds anything.
export const REAL_CHATGPT_SIGN_IN_REPORT = '';
