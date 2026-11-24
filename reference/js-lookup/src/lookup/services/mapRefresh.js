'use strict';

/**
 * Map refresh helper shared by the player / alliance / state panels.
 *
 * The website exposes a public queue: status → POST → poll until the job
 * leaves `running`. Once it finishes, `/api/map` already contains the new city
 * coordinates, so we re-fetch it (bypassing the short TTL cache) and let the
 * panels redraw their map cards from the fresh payload.
 */

const dataClient = require('./dataClient');

const POLL_MS = 2500;
const MAX_WAIT_MS = 40 * 1000;
let token = dataClient.MAP_UPDATE_TOKEN;

const inFlight = new Map(); // kid -> Promise<result>

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

function fmtCooldown(sec) {
  const s = Math.max(0, Math.floor(Number(sec) || 0));
  if (!s) return 'a moment';
  const m = Math.floor(s / 60);
  const r = s % 60;
  return m ? m + 'm ' + String(r).padStart(2, '0') + 's' : s + 's';
}

function jobActive(st) {
  const job = st && st.job;
  return !!(job && (job.status === 'queued' || job.status === 'running'));
}

/** Poll the public status endpoint until the refresh job we care about ends. */
async function waitForJob(kid, before = null, activeSeen = false) {
  const deadline = Date.now() + MAX_WAIT_MS;
  let last = null;
  let sawActive = activeSeen;
  while (Date.now() < deadline) {
    await sleep(POLL_MS);
    const res = await dataClient.getMapUpdateStatus(kid, token);
    if (!res.ok || !res.data) continue;
    last = res.data;
    if (jobActive(last)) { sawActive = true; continue; }
    const finished = Number(last.last_finished_at || 0);
    // `job.status === 'done'` is set briefly by the site, otherwise the job is
    // simply cleared once the write finishes (and last_finished_at advances).
    if (sawActive || (before != null && finished > before)) {
      return { ok: true, status: last };
    }
  }
  return { ok: false, status: last };
}

async function run(kid) {
  const id = Number(kid);
  if (!Number.isFinite(id)) {
    return { ok: false, reason: 'bad-kid', message: 'That is not a valid state.' };
  }

  const initial = await dataClient.getMapUpdateStatus(id, token);
  if (!initial.ok || !initial.data) {
    return { ok: false, reason: 'unreachable', message: 'The map service is not responding right now.' };
  }
  const st = initial.data;

  if (st.updates_paused) {
    return { ok: false, reason: 'paused', message: 'Map updates are paused right now.' };
  }

  // An update requested elsewhere (another user / the website) is already in
  // the queue — wait for that one instead of stacking a second request.
  if (jobActive(st)) {
    const waited = await waitForJob(id, Number(st.last_finished_at || 0), true);
    if (!waited.ok) {
      return { ok: false, reason: 'timeout', message: 'The map is still updating — try again in a minute.' };
    }
    const map = await dataClient.getStateMap(id, { fresh: true });
    return map
      ? { ok: true, map, status: waited.status }
      : { ok: false, reason: 'fetch', message: 'The map finished updating but could not be loaded.' };
  }

  const cooldown = Number(st.cooldown_remaining_sec || 0);
  if (cooldown > 0 || st.update_available === false) {
    return {
      ok: false,
      reason: 'cooldown',
      cooldown,
      message: cooldown
        ? 'The map was updated recently — try again in ' + fmtCooldown(cooldown) + '.'
        : 'The map was updated recently — try again later.',
    };
  }

  const before = Number(st.last_finished_at || 0);
  const req = await dataClient.requestMapUpdate(id, token);
  // The site can hand back its own request token; adopt it for the status polls.
  if (req && req.token) token = req.token;
  const d = (req && req.data) || {};

  if (d.error === 'ip_cooldown' || d.error === 'cooldown') {
    const left = Number(
      d.error === 'ip_cooldown'
        ? (d.ip_cooldown_remaining_sec || 0)
        : (d.cooldown_remaining_sec || 0)
    );
    return {
      ok: false,
      reason: d.error,
      cooldown: left,
      message: d.message || ('Too many map updates right now — try again in ' + fmtCooldown(left) + '.'),
    };
  }

  if (!req.ok) {
    return {
      ok: false,
      reason: 'request',
      message: (d && d.detail) || 'The map update could not be started.',
    };
  }

  const waited = await waitForJob(id, before);
  if (!waited.ok) {
    return {
      ok: false,
      reason: 'timeout',
      message: 'The map update is taking longer than expected — try again shortly.',
    };
  }

  const map = await dataClient.getStateMap(id, { fresh: true });
  if (!map) {
    return { ok: false, reason: 'fetch', message: 'The map updated but fresh data could not be loaded.' };
  }
  return { ok: true, map, status: waited.status };
}

/**
 * Refresh the map for one state. Concurrent clicks for the same state share a
 * single update request.
 * @param {number|string} kid
 * @returns {Promise<{ok:boolean, map?:object, status?:object, message?:string, reason?:string, cooldown?:number}>}
 */
function refreshStateMap(kid) {
  const id = Number(kid);
  if (!Number.isFinite(id)) {
    return Promise.resolve({ ok: false, reason: 'bad-kid', message: 'That is not a valid state.' });
  }
  if (inFlight.has(id)) return inFlight.get(id);
  const p = run(id).finally(() => inFlight.delete(id));
  inFlight.set(id, p);
  return p;
}

/** Find a player's city entry (coordinates + power) in a fresh map payload. */
function findCity(mapData, player = {}) {
  const cities = (mapData && mapData.cities) || [];
  if (player.uid != null) {
    const byUid = cities.find((c) => String(c.uid) === String(player.uid));
    if (byUid) return byUid;
  }
  const name = player.name ? String(player.name).toLowerCase() : '';
  if (name) {
    return cities.find((c) => String(c.nick_name || '').toLowerCase() === name) || null;
  }
  return null;
}

module.exports = { refreshStateMap, findCity, POLL_MS, MAX_WAIT_MS };
