/** a page's address the window loads: the web, or a blank page */
export const pageOk = (url: unknown) => typeof url === 'string' && /^(https?:\/\/|about:blank$)/.test(url);
