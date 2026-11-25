/* Typing Greek, Russian and Japanese with a normal keyboard: Latin letters become the other script.
   Translit.convert(text, lang) → the converted word, or null when there is nothing to convert (wrong language, or the
   text is not plain Latin letters).
   - Ancient Greek: logos → λογος, anthropos → ανθρωπος, angelos → αγγελος (th θ, ph φ, ch/kh χ, ps ψ, x ξ, h/ē η,
     w/ō ω, y/u υ, c/k κ; an h at the start of a word is the rough breathing and is left out; final σ → ς).
     Accents and breathings are not needed: Lex Studio finds λόγος and ἄνθρωπος from them.
   - Russian: sobaka → собака, zhizn' → жизнь, khorosho → хорошо (shch щ, zh ж, kh х, ts ц, ch ч, sh ш, ya я, yu ю,
     yo ё, y ы, j й, ' ь, " ъ).
   - Japanese: inu → いぬ, neko → ねこ, gakkou → がっこう, konnichiha → こんにちは (Hepburn and Kunrei; nn / n' → ん). */
(() => {
  'use strict';
  const GREEK = [['rh', 'ρ'], ['th', 'θ'], ['ph', 'φ'], ['ch', 'χ'], ['kh', 'χ'], ['ps', 'ψ'], ['ng', 'γγ'], ['nk', 'γκ'], ['nx', 'γξ'],
    ['a', 'α'], ['b', 'β'], ['g', 'γ'], ['d', 'δ'], ['e', 'ε'], ['ē', 'η'], ['z', 'ζ'], ['h', 'η'], ['i', 'ι'], ['k', 'κ'], ['c', 'κ'],
    ['l', 'λ'], ['m', 'μ'], ['n', 'ν'], ['x', 'ξ'], ['o', 'ο'], ['ō', 'ω'], ['w', 'ω'], ['p', 'π'], ['r', 'ρ'], ['s', 'σ'], ['t', 'τ'],
    ['u', 'υ'], ['y', 'υ'], ['f', 'φ'], ['q', 'θ'], ['v', 'β']];
  const RUSSIAN = [['shch', 'щ'], ['sch', 'щ'], ['zh', 'ж'], ['kh', 'х'], ['ts', 'ц'], ['ch', 'ч'], ['sh', 'ш'], ['ya', 'я'], ['ja', 'я'],
    ['yu', 'ю'], ['ju', 'ю'], ['yo', 'ё'], ['jo', 'ё'], ['ye', 'е'], ['a', 'а'], ['b', 'б'], ['v', 'в'], ['w', 'в'], ['g', 'г'], ['d', 'д'],
    ['e', 'е'], ['z', 'з'], ['i', 'и'], ['j', 'й'], ['k', 'к'], ['q', 'к'], ['l', 'л'], ['m', 'м'], ['n', 'н'], ['o', 'о'], ['p', 'п'],
    ['r', 'р'], ['s', 'с'], ['t', 'т'], ['u', 'у'], ['f', 'ф'], ['h', 'х'], ['x', 'кс'], ['c', 'ц'], ['y', 'ы'], ["'", 'ь'], ['"', 'ъ']];

  /* Japanese: the syllables, built from rows of the kana table. */
  const KANA = {a: 'あ', i: 'い', u: 'う', e: 'え', o: 'お', '-': 'ー'};
  const ROWS = {k: 'かきくけこ', g: 'がぎぐげご', s: 'さしすせそ', z: 'ざじずぜぞ', t: 'たちつてと', d: 'だぢづでど', n: 'なにぬねの',
    h: 'はひふへほ', b: 'ばびぶべぼ', p: 'ぱぴぷぺぽ', m: 'まみむめも', r: 'らりるれろ', l: 'らりるれろ'};
  for (const [c, row] of Object.entries(ROWS)) [...'aiueo'].forEach((v, i) => { KANA[c + v] = row[i]; });
  Object.assign(KANA, {ya: 'や', yu: 'ゆ', yo: 'よ', wa: 'わ', wo: 'を', wi: 'うぃ', we: 'うぇ', shi: 'し', chi: 'ち', tsu: 'つ', fu: 'ふ',
    ji: 'じ', fa: 'ふぁ', fi: 'ふぃ', fe: 'ふぇ', fo: 'ふぉ', va: 'ゔぁ', vi: 'ゔぃ', vu: 'ゔ', ve: 'ゔぇ', vo: 'ゔぉ', "n'": 'ん'});
  const SMALL = {a: 'ゃ', u: 'ゅ', o: 'ょ'};
  for (const [c, i] of Object.entries({k: 'き', g: 'ぎ', n: 'に', h: 'ひ', b: 'び', p: 'ぴ', m: 'み', r: 'り', s: 'し', z: 'じ', t: 'ち', d: 'ぢ'}))
    for (const v of 'auo') KANA[c + 'y' + v] = i + SMALL[v];
  for (const v of 'auo') { KANA['sh' + v] = 'し' + SMALL[v]; KANA['ch' + v] = 'ち' + SMALL[v]; KANA['j' + v] = 'じ' + SMALL[v]; }
  KANA.she = 'しぇ'; KANA.che = 'ちぇ'; KANA.je = 'じぇ'; KANA.ti = 'ち'; KANA.tu = 'つ'; KANA.si = 'し'; KANA.zi = 'じ'; KANA.hu = 'ふ';

  function japanese(s) {
    let out = '', i = 0;
    while (i < s.length) {
      const c = s[i], n = s[i + 1];
      // A doubled consonant is a small tsu (gakkou → がっこう); nn and n' are ん.
      if (c === n && c !== 'n' && /[bcdfghjkmprstvwz]/.test(c)) { out += 'っ'; i++; continue; }
      if (c === 't' && n === 'c' && s[i + 2] === 'h') { out += 'っ'; i++; continue; }
      // nn: ん — before a vowel the second n starts the next syllable (konnichiha → こんにちは, onna → おんな).
      if (c === 'n' && n === 'n') { out += 'ん'; i += /[aiueoy]/.test(s[i + 2] || '') ? 1 : 2; continue; }
      let hit = '';
      for (const len of [3, 2, 1]) { const part = s.slice(i, i + len); if (KANA[part]) { hit = part; break; } }
      // n before a consonant (not y) or at the end is ん.
      if ((!hit || hit === 'n') && c === 'n' && (!n || !/[aiueoy]/.test(n))) { out += 'ん'; i++; continue; }
      if (!hit) return null;
      out += KANA[hit]; i += hit.length;
    }
    return out;
  }

  function table(s, pairs) {
    let out = '', i = 0;
    while (i < s.length) {
      const p = pairs.find(([k]) => s.startsWith(k, i));
      if (!p) { if (s[i] === ' ' || s[i] === '-') { out += s[i]; i++; continue; } return null; }
      out += p[1]; i += p[0].length;
    }
    return out;
  }

  function greek(s) {
    // The rough breathing: an h at the start of a word before a vowel (hodos → οδος).
    const g = table(s.replace(/(^|[\s-])h(?=[aeiouyēōw])/g, '$1'), GREEK);
    return g && g.replace(/σ(?=$|[\s-])/g, 'ς');
  }

  function convert(text, lang) {
    const s = String(text || '').trim().toLowerCase();
    if (!s || !/^[a-zēō' "-]+$/.test(s) || !/[a-zēō]/.test(s)) return null;
    if (lang === 'grc') return greek(s);
    if (lang === 'ru') return table(s, RUSSIAN);
    if (lang === 'ja') return japanese(s);
    return null;
  }

  window.Translit = {convert, languages: ['grc', 'ru', 'ja']};
})();
