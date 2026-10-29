// Parse quoted RFC 4180 fields, including embedded commas and newlines.
export function csvRows(text) {
  const rows = []; let row = [], field = '', quoted = false;
  text = text.replace(/^\uFEFF/, '');
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (c === '"') {
      if (quoted && text[i + 1] === '"') { field += '"'; i++; }
      else if (quoted || field === '') quoted = !quoted;
      else throw new Error('Некорректные кавычки CSV');
    } else if (c === ',' && !quoted) { row.push(field); field = ''; }
    else if ((c === '\n' || c === '\r') && !quoted) {
      if (c === '\r' && text[i + 1] === '\n') i++;
      row.push(field); if (row.some(v => v.trim())) rows.push(row);
      row = []; field = '';
    } else field += c;
  }
  if (quoted) throw new Error('Незакрытые кавычки CSV');
  row.push(field); if (row.some(v => v.trim())) rows.push(row);
  return rows;
}
export function importTracks(text, filename = 'tracks.txt') {
  if (text.length > 10_000_000) throw new Error('Файл импорта больше 10 МБ');
  const make = (title, artist, url, duration, index) => ({ id: `import-${index}`, title: title.trim(), artist: artist?.trim() || undefined, url: url?.trim() || '', duration: Number(duration) > 0 ? Number(duration) : undefined, position: index + 1 });
  if (filename.toLowerCase().endsWith('.csv')) {
    const [header, ...rows] = csvRows(text);
    if (!header) return [];
    const keys = header.map(h => h.trim().toLowerCase());
    if (!keys.includes('title') && !keys.includes('url')) throw new Error('CSV должен содержать колонку title или url');
    const get = (row, key) => row[keys.indexOf(key)] || '';
    return rows.map((row, i) => make(get(row, 'title') || get(row, 'url'), get(row, 'artist'), get(row, 'url'), get(row, 'duration'), i));
  }
  return text.split(/\r?\n/).map(s => s.trim()).filter(Boolean).map((line, i) => {
    if (/^https:\/\//i.test(line)) return make(line, '', line, '', i);
    const separator = /\s+[—–-]\s+/.exec(line);
    return separator ? make(line.slice(separator.index + separator[0].length), line.slice(0, separator.index), '', '', i) : make(line, '', '', '', i);
  });
}
