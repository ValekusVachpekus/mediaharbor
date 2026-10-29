export function spotifyTrack(row) {
  const link = row.querySelector('a[href*="/track/"]');
  if (!link) return null;
  let url;
  try { url = new URL(link.getAttribute('href'), 'https://open.spotify.com'); } catch { return null; }
  const match = url.pathname.match(/\/(?:track)\/([a-zA-Z0-9]{22})$/);
  if (!match || url.hostname !== 'open.spotify.com') return null;
  const artist = [...row.querySelectorAll('a[href*="/artist/"]')].map(a => a.textContent.trim()).filter(Boolean).join(', ');
  const text = row.innerText || row.textContent || '';
  const times = text.match(/\b\d{1,2}:\d{2}(?::\d{2})?\b/g);
  const duration = times?.length ? times.at(-1).split(':').reduce((sum, n) => sum * 60 + Number(n), 0) : undefined;
  const indexText = row.querySelector('[aria-colindex="1"]')?.textContent.trim();
  const position = Number(row.getAttribute('aria-rowindex')) || (/^\d+$/.test(indexText || '') ? Number(indexText) : undefined);
  return { id: match[1], title: link.textContent.trim() || link.getAttribute('aria-label') || 'Без названия', artist: artist || undefined, url: `https://open.spotify.com/track/${match[1]}`, duration, position };
}
export function mergeTracks(target, incoming) {
  for (const track of incoming) {
    if (!track) continue;
    const key = track.position ? `${track.position}:${track.id}` : track.id;
    target.set(key, track);
  }
  return [...target.values()].sort((a, b) => (a.position || 0) - (b.position || 0));
}
export function completion(found, expected, reachedEnd, stable) {
  return Boolean(expected && found === expected && reachedEnd && stable >= 3);
}
