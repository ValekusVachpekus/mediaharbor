// Explicit UI transport fixture; never imported by the product.
export function installFixture() {
    const candidate = { id: 'abcdefghijk', title: 'Artist — Song', artist: 'Artist', url: 'https://www.youtube.com/watch?v=abcdefghijk', duration: 222 };
    let settings = { outputDir: '/tmp/MediaHarbor', theme: 'dark', spotifyClientId: '', toolsDir: '' };
    const jobs = [];
    window.__TAURI__ = { core: { invoke: async (command, args) => {
      if (command === 'pick_files') return ['/tmp/tone.wav'];
      if (command === 'pick_directory') return '/tmp/MediaHarbor';
      if (command !== 'rpc') return null;
      const { op, payload, requestId } = args.request;
      let data;
      if (op === 'settings.get') data = settings;
      else if (op === 'settings.set') { settings = payload; data = true; }
      else if (op === 'jobs.list') data = { items: jobs, total: jobs.length };
      else if (op === 'media.search') data = [candidate];
      else if (op === 'media.analyze') data = {items:[candidate],total:1,collectionId:'fixture',nextOffset:null};
      else if (op === 'media.probe') data = { format: { size: '176400', duration: '2' } };
      else if (op === 'spotify.status') data = { connected: false };
      else if (op === 'tools.check') data = [{ name: 'ffmpeg', available: true, version: 'ffmpeg fixture' }];
      else if (op === 'jobs.create') { data = payload.items.map(input => ({ id: crypto.randomUUID(), input, status: 'queued', stage: 'В очереди', progress: 0, createdAt: Date.now(), updatedAt: Date.now() })); jobs.push(...data); }
      else throw Error(`Unexpected fixture operation: ${op}`);
      return { version: 1, requestId, ok: true, data };
    } } };
}
