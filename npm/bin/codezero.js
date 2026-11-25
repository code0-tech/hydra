#!/usr/bin/env node
'use strict';

const { spawnSync } = require('child_process');
const { ensureBinary } = require('../lib/install.js');

ensureBinary()
  .then((binPath) => {
    const result = spawnSync(binPath, process.argv.slice(2), { stdio: 'inherit' });
    if (result.error) throw result.error;
    process.exit(result.status === null ? 1 : result.status);
  })
  .catch((err) => {
    console.error(`codezero: ${err.message}`);
    process.exit(1);
  });
