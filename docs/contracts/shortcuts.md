# Keyboard shortcuts: the window's keys

The business behavior contract for the Electron window's keyboard shortcuts, from [the template](../business-contract-template.md), as T3 Code keeps its own (one table of keys, one listener, every label from the table). Scenarios are numbered `SHORTCUTS-n`; the tests that prove them are named after them (`shortcuts_1_…` to `shortcuts_6_…` in `apps/electron/src/renderer/src/test/shortcuts.test.tsx`).

## Feature and status

- **Outcome:** The owner reaches what they use most without the mouse: a new conversation, the search over every conversation, the words in the conversation in front of them, and Settings. Esc closes what's open over the page.
- **Status:** **Implemented** in the Electron app. The keys are T3 Code's (`mod+n`, `mod+k`, `mod+f`) and the Mac's own for Settings (`⌘,`). Sangam's screens have none yet: the menu bar items and the hints are proposed words, his to change.
- **Scope:** the window's page and the Mac's menu bar. **Out of scope:** keys the owner can change (T3 Code reads them from a file; Oby has one table), a command palette (⌘K opens the side panel's search, Sangam's search), and the message box's own keys.

## Behavior

- **Actors and ownership:** Only the app. obid isn't asked anything new: ⌘K's search asks obid's `GET /v1/conversations?q=` as the panel's search does (SEARCH-1 to 3, chat-first.md).
- **The keys** (`shared/shortcuts.ts`, the one table; ⌘ on a Mac, Ctrl elsewhere, no other modifier):

  | Key | What it does |
  |---|---|
  | ⌘N | A new conversation: back to the empty chat, its box focused (as New conversation) |
  | ⌘K | The side panel's search: the panel opens if it's shut, on Chats, its field focused with what was typed selected |
  | ⌘F | Find in the conversation shown: a field over its top right, `2 of 5`, Return and the arrows to go through them (⇧Return back), each in view and lit; Esc closes it |
  | ⌘, | Settings, on General |
  | Esc | Closes what's open over the page: a menu, a sheet, the find bar, a page (as before) |

- **How a key is heard:** one listener on the window runs the newest screen's command for that key (T3 Code's scopes): ⌘F is the conversation's while one is shown, and nothing with a page over it. The page takes a key it answered, so its menu item doesn't run it again; a key the page leaves alone reaches the Mac's menu bar, whose item sends the same command back. A word being composed (an input method) and an alert dialog keep their keys. The keys work in the message box too, as T3 Code's do: none of them types text.
- **Where they show:** the Mac's menu bar (Oby › Settings…, File › New Conversation, Edit › Find › Search and Find in Conversation), each with its key, from the same table; the side panel's New conversation row (its key as the pointer is on it), Settings' tip and the account menu's Settings, and the Search button's tip.
- **Invariants:** a menu never shows a key the window doesn't answer, and the window answers no key a menu doesn't show (but Esc). No key sends anything to obid by itself.

## Acceptance scenarios

- **SHORTCUTS-1:** ⌘N from Settings closes it and leaves the owner on the empty chat; Ctrl+N off a Mac does the same; ⇧⌘N and a key pressed while an input method composes do nothing. (`shortcuts_1_2_4_new_conversation_search_and_settings_from_anywhere`, `shortcuts_1_ctrl_stands_for_cmd_off_a_mac_and_an_ime_word_is_left_alone`)
- **SHORTCUTS-2:** ⌘K opens the side panel's search, its field focused, from anywhere. (`shortcuts_1_2_4_new_conversation_search_and_settings_from_anywhere`)
- **SHORTCUTS-3:** In a conversation, ⌘F opens its find bar; `retry` in a thread holding it three times says `1 of 3`, Return `2 of 3`, ⇧Return twice `3 of 3`; words it doesn't hold say `No results`; Esc closes it. With a page over the conversation, ⌘F isn't its. (`shortcuts_3_cmd_f_finds_the_words_in_the_conversation_and_esc_closes_it`, `shortcuts_3_with_a_page_over_the_conversation_cmd_f_is_not_its`)
- **SHORTCUTS-4:** ⌘, opens Settings, and the page takes the key. (`shortcuts_1_2_4_new_conversation_search_and_settings_from_anywhere`)
- **SHORTCUTS-5:** Esc closes an open menu. (`shortcuts_5_esc_closes_an_open_menu`)
- **SHORTCUTS-6:** The Mac's menu bar has an item for each key with that very key, and only those carry one; the side panel shows ⌘N on New conversation, `Settings ⌘,` and `Search ⌘K` as tips. (`shortcuts_6_the_menus_show_the_keys_the_window_answers`)

**Test it when it's done (real Mac):** open Oby; press ⌘, (Settings), ⌘N (the empty chat), ⌘K (the search field, focused), type a word from an old conversation and see it listed; open a conversation, ⌘F, type a word, Return through the matches, Esc. Open the menu bar's Oby, File and Edit › Find menus and see the same keys.
