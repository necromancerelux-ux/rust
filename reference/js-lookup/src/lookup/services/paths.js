'use strict';

/**
 * Resolve shared bot modules no matter where lookup files are installed
 * (src/lookup/..., src/components/..., etc.).
 */

const path = require('path');
const fs = require('fs');

function tryRequire(candidates) {
  const errors = [];
  for (const c of candidates) {
    try {
      return require(c);
    } catch (e) {
      if (e.code !== 'MODULE_NOT_FOUND') throw e;
      errors.push(c);
    }
  }
  const err = new Error(
    `Cannot find module. Tried:\n  - ${errors.join('\n  - ')}`
  );
  err.code = 'MODULE_NOT_FOUND';
  throw err;
}

/** From a file under lookup (any depth), resolve shared packages. */
function fromLookupFile(dirname) {
  // Walk up until we find src/ or package root markers
  const roots = [];
  let cur = dirname;
  for (let i = 0; i < 10; i++) {
    roots.push(cur);
    const parent = path.dirname(cur);
    if (parent === cur) break;
    cur = parent;
  }
  // Common install roots on hosts
  try {
    roots.push(process.cwd());
    roots.push(path.join(process.cwd(), 'src'));
  } catch { /* ignore */ }

  function candidates(relParts) {
    const list = [];
    for (const root of roots) {
      list.push(path.join(root, ...relParts));
    }
    // also classic relatives from dirname
    list.push(path.join(dirname, '..', ...relParts));
    list.push(path.join(dirname, '../..', ...relParts));
    list.push(path.join(dirname, '../../..', ...relParts));
    list.push(path.join(dirname, '../../../..', ...relParts));
    return list;
  }

  return {
    theme() {
      return tryRequire([
        ...candidates(['utils', 'theme']),
        ...candidates(['giftcode', 'utils', 'theme']),
        path.join(dirname, '../../utils/theme'),
        path.join(dirname, '../utils/theme'),
        path.join(dirname, '../../giftcode/utils/theme'),
        path.join(dirname, '../giftcode/utils/theme'),
      ]);
    },
    db() {
      return tryRequire([
        ...candidates(['database', 'db']),
        path.join(dirname, '../../database/db'),
        path.join(dirname, '../database/db'),
        path.join(dirname, '../../../database/db'),
      ]);
    },
    permissions() {
      return tryRequire([
        ...candidates(['permissions', 'permissionManager']),
        path.join(dirname, '../../permissions/permissionManager'),
        path.join(dirname, '../permissions/permissionManager'),
        path.join(dirname, '../../../permissions/permissionManager'),
      ]);
    },
  };
}

module.exports = { tryRequire, fromLookupFile };
