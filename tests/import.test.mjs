import test from 'node:test';
import assert from 'node:assert/strict';
import { csvRows, importTracks } from '../packages/shared/src/import.mjs';
import { completion, mergeTracks, spotifyTrack } from '../apps/extension/src/spotify-collector.mjs';

test('CSV handles BOM, CRLF, commas, escaped quotes and embedded newlines', () => {
  assert.deepEqual(csvRows('\uFEFFtitle,artist\r\n"One, two","A ""B"""\r\n"Line\nbreak",C'), [['title', 'artist'], ['One, two', 'A "B"'], ['Line\nbreak', 'C']]);
});
test('CSV rejects a truncated quoted field instead of importing corrupted names', () => {
  assert.throws(() => importTracks('title,artist\n"Incomplete,A', 'x.csv'), /Незакрытые/);
  assert.throws(() => importTracks('album,year\nUnknown,2020', 'x.csv'), /title или url/);
});
test('CSV accepts title/artist and URL-only rows without destroying duplicates', () => {
  const tracks = importTracks('title,artist,url,duration\nSong,A,,123\nSong,A,,123', 'x.csv');
  assert.equal(tracks.length, 2); assert.equal(tracks[0].artist, 'A'); assert.equal(tracks[0].duration, 123);
  assert.equal(importTracks('url\nhttps://youtu.be/abcdefghijk', 'x.csv')[0].url, 'https://youtu.be/abcdefghijk');
});
test('TXT preserves dashes inside titles and separates the artist', () => {
  assert.deepEqual(importTracks('Artist — Song - Remix\n\nhttps://youtu.be/abcdefghijk')[0], { id: 'import-0', title: 'Song - Remix', artist: 'Artist', url: '', duration: undefined, position: 1 });
  assert.equal(importTracks('Artist - Song\nPlain title')[1].title, 'Plain title');
});
test('virtualized rows merge across scrolls but retain repeated playlist positions', () => {
  const rows = new Map(); const a = { id: 'a', position: 1 }; const b = { id: 'a', position: 2 };
  mergeTracks(rows, [a, b]); assert.equal(mergeTracks(rows, [a, { id: 'c', position: 3 }]).length, 3);
});
test('collector never declares an unknown or partial collection complete', () => {
  assert.equal(completion(100, undefined, true, 10), false);
  assert.equal(completion(99, 100, true, 10), false);
  assert.equal(completion(100, 100, false, 10), false);
  assert.equal(completion(100, 100, true, 3), true);
});
test('collector reads track identity, artists and duration from a row', () => {
  const element = (text, attrs = {}) => ({ textContent: text, getAttribute: key => attrs[key] || null });
  const row = { querySelector: selector => selector.includes('/track/') ? element('Song', { href: '/track/1234567890123456789012' }) : element('3'), querySelectorAll: () => [element('Artist')], innerText: '3 Song Artist 3:42', getAttribute: () => null };
  assert.deepEqual(spotifyTrack(row), { id: '1234567890123456789012', title: 'Song', artist: 'Artist', url: 'https://open.spotify.com/track/1234567890123456789012', duration: 222, position: 3 });
});
