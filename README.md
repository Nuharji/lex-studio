# Lex Studio (Rust / Tauri)

> **Status: Pre-Alpha** — early version (0.x): it works, but things may still change or break. Version 1.0 will be the first stable one.

A small pop-up dictionary for translating — the dictionary of the Studio family. Copy a word anywhere, press the key
combination (default **⌘⌥⇧W** / **Ctrl+Alt+Shift+W**), and a little window comes up at the mouse pointer with the word
already looked up. **Escape** hides it again; the app stays in the tray / menu bar so the key keeps working.
The native shell is Rust ([Tauri 2](https://tauri.app)); the UI is plain HTML/JS in `ui/`, with the same themes and
layouts as the other Studio apps.

## Languages
Any pair of: **Latin, Ancient Greek, English, German, French, Spanish, Russian, Italian, Swedish, Japanese**.

## Features
- **Pop-up** — global key combination (changeable), opens at the mouse pointer, looks up the copied word (a word or a
  short phrase), Escape hides; optional: hide when clicking elsewhere, pin on top, stay in the background when closed
- **Translations, meanings, forms** — translation tables, meanings (in German or English), inflection tables; links in a
  meaning look that word up; click a translation to copy it, Ctrl / ⌘-click or double-click to look it up the other way
- **Inflected forms** lead to the basic form (canem → canis, amare → amo); Japanese kana lead to the kanji words
  (いぬ → 犬)
- **Through English** — when Wiktionary has no direct translation (e.g. Latin → French), the English meanings are
  translated (marked „through English: dog“)
- **Loose search** — accents, breathings and macrons are not needed (λογος → λόγος, ανθρωπος → ἄνθρωπος); similar words
  when nothing is found; suggestions while typing
- **Typing other scripts with a normal keyboard** — logos → λογος, sobaka → собака, inu → いぬ; the script of a pasted word
  switches the source language (Greek → Ancient Greek, Cyrillic → Russian, kana / kanji → Japanese)
- **Word list** — save words with a star, export as CSV (for Plan Studio's vocabulary trainer, Anki, Excel) or copy
- **Offline** — every answer is kept (up to 3000, refreshed after 30 days); without internet the stored one is shown
- All Studio themes and layouts, German / English; theme, layout and language shared with the other Studio apps

## Where the words come from
[Wiktionary](https://www.wiktionary.org) (CC BY-SA 4.0): the German one for German meanings and German words, the
English one for everything else. Each answer links to its pages. Lex Studio sends only the word you look up to
Wiktionary; nothing else leaves the computer.

## MCP
Lex Studio is also an MCP server (started with `--mcp`, no window, MCP on stdin / stdout):

- `lookup` — word, from, to → translations, meanings and forms as text (`from` may be left out for Greek, Cyrillic or
  Japanese script).
- `suggest` — dictionary words starting with some letters.
- `languages` — the ten languages and their codes (`la`, `grc`, `en`, `de`, `fr`, `es`, `ru`, `it`, `sv`, `ja`).

Claude Code: `claude mcp add lex-studio -- "/Applications/Lex Studio.app/Contents/MacOS/lex-studio" --mcp`.

## Layout
```
ui/                 index.html, app.js — the window; translit.js — Latin letters → Greek / Cyrillic / kana;
                    native.js, applogo.js, shared.js, layouts.css (the same in every Studio app)
src-tauri/src/      main.rs — the pop-up (key, tray, placing at the pointer) and the commands; lookup.rs — which pages to
                    read and putting the answer together; parse.rs — Wiktionary pages (German and English) and the REST
                    definitions; wikitext.rs — templates, links, HTML; lang.rs — the languages and text folding;
                    cache.rs — answers on disk; mcp.rs, mcp_tools.rs — MCP server; shared.rs — settings shared by the
                    Studio apps; appicon.rs — Dock icon follows the theme
src-tauri/i18n/     en.json, de.json — all texts
```

## Build
Install Rust and Node, then `npx @tauri-apps/cli@2 icon app-icon.png` once and `npx @tauri-apps/cli@2 dev`.
`cargo test` in `src-tauri` runs the tests (parsing German / English pages, through English, basic forms, kana, loose
search, offline, cache, MCP); `cargo test -- --ignored live` asks the real Wiktionary.
Release builds for Windows / macOS / Linux come from GitHub Actions when you push a tag like `v0.1.0`.
